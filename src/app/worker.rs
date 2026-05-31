use std::sync::Arc;
use std::hint;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crate::app::winutil::{
    current_monitor_refresh_hz, cursor_position, is_cursor_over_window, send_left_click,
    send_left_click_burst, set_cursor_position,
};

const RANDOM_DELAY_WINDOW: Duration = Duration::from_secs(5);
const JITTER_PAUSE_ON_MANUAL_MOVE: Duration = Duration::from_millis(700);
const MAXIMUM_YIELD_STRIDE: u32 = 8_192;
const MAXIMUM_CONTROL_POLL_STRIDE: u32 = 64;
const APP_WINDOW_TITLE: &str = "Hermes Rapid Clicker";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClickRate {
    Slow,
    Normal,
    Fast,
    Rapid,
    Turbo,
    Extreme,
    Maximum,
    Fps,
}

impl ClickRate {
    pub const ALL: [ClickRate; 7] = [
        ClickRate::Slow,
        ClickRate::Normal,
        ClickRate::Fast,
        ClickRate::Rapid,
        ClickRate::Turbo,
        ClickRate::Extreme,
        ClickRate::Maximum,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ClickRate::Slow => "Slow \u{2014} 1/s",
            ClickRate::Normal => "Normal \u{2014} 6/s",
            ClickRate::Fast => "Fast \u{2014} 15/s",
            ClickRate::Rapid => "Rapid \u{2014} 30/s",
            ClickRate::Turbo => "Turbo \u{2014} 60/s",
            ClickRate::Extreme => "Extreme \u{2014} 120/s",
            ClickRate::Maximum => "Maximum \u{2014} \u{221E}/s",
            ClickRate::Fps => "FPS \u{2014} 1 per frame",
        }
    }

    pub fn compact_label(self) -> &'static str {
        match self {
            ClickRate::Slow => "Slow (1/s)",
            ClickRate::Normal => "Normal (6/s)",
            ClickRate::Fast => "Fast (15/s)",
            ClickRate::Rapid => "Rapid (30/s)",
            ClickRate::Turbo => "Turbo (60/s)",
            ClickRate::Extreme => "Extreme (120/s)",
            ClickRate::Maximum => "Maximum (\u{221E})",
            ClickRate::Fps => "FPS (1/frame)",
        }
    }

    pub fn as_config_value(self) -> &'static str {
        match self {
            ClickRate::Slow => "slow",
            ClickRate::Normal => "normal",
            ClickRate::Fast => "fast",
            ClickRate::Rapid => "rapid",
            ClickRate::Turbo => "turbo",
            ClickRate::Extreme => "extreme",
            ClickRate::Maximum => "maximum",
            ClickRate::Fps => "fps",
        }
    }

    pub fn from_config_value(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "slow" => Some(ClickRate::Slow),
            "normal" => Some(ClickRate::Normal),
            "fast" => Some(ClickRate::Fast),
            "rapid" | "very_fast" => Some(ClickRate::Rapid),
            "turbo" => Some(ClickRate::Turbo),
            "extreme" => Some(ClickRate::Extreme),
            "maximum" | "unsafe" => Some(ClickRate::Maximum),
            "fps" => Some(ClickRate::Fps),
            _ => None,
        }
    }

    fn fixed_cps(self) -> Option<f64> {
        match self {
            ClickRate::Slow => Some(1.0),
            ClickRate::Normal => Some(6.0),
            ClickRate::Fast => Some(15.0),
            ClickRate::Rapid => Some(30.0),
            ClickRate::Turbo => Some(60.0),
            ClickRate::Extreme => Some(120.0),
            ClickRate::Maximum | ClickRate::Fps => None,
        }
    }

    fn as_u8(self) -> u8 {
        match self {
            ClickRate::Slow => 0,
            ClickRate::Normal => 1,
            ClickRate::Fast => 2,
            ClickRate::Rapid => 3,
            ClickRate::Turbo => 4,
            ClickRate::Extreme => 5,
            ClickRate::Maximum => 6,
            ClickRate::Fps => 7,
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            0 => ClickRate::Slow,
            1 => ClickRate::Normal,
            2 => ClickRate::Fast,
            3 => ClickRate::Rapid,
            4 => ClickRate::Turbo,
            5 => ClickRate::Extreme,
            6 => ClickRate::Maximum,
            7 => ClickRate::Fps,
            _ => ClickRate::Normal,
        }
    }
}

struct ClickWorkerState {
    running: AtomicBool,
    shutdown: AtomicBool,
    mode: AtomicU8,
    humanize_random_delay: AtomicBool,
    humanize_cursor_jitter: AtomicBool,
    maximum_burst: AtomicU8,
    click_count: AtomicU64,
}

pub struct ClickWorker {
    state: Arc<ClickWorkerState>,
    handle: Option<thread::JoinHandle<()>>,
}

impl ClickWorker {
    pub fn new(
        initial_mode: ClickRate,
        maximum_burst: u8,
        humanize_random_delay: bool,
        humanize_cursor_jitter: bool,
    ) -> Self {
        let state = Arc::new(ClickWorkerState {
            running: AtomicBool::new(false),
            shutdown: AtomicBool::new(false),
            mode: AtomicU8::new(initial_mode.as_u8()),
            humanize_random_delay: AtomicBool::new(humanize_random_delay),
            humanize_cursor_jitter: AtomicBool::new(humanize_cursor_jitter),
            maximum_burst: AtomicU8::new(maximum_burst.clamp(1, 5)),
            click_count: AtomicU64::new(0),
        });
        let worker_state = Arc::clone(&state);
        let handle = thread::spawn(move || click_loop(worker_state));
        Self {
            state,
            handle: Some(handle),
        }
    }

    pub fn set_running(&self, running: bool) {
        self.state.running.store(running, Ordering::Relaxed);
    }

    pub fn toggle_running(&self) -> bool {
        let next = !self.state.running.load(Ordering::Relaxed);
        self.state.running.store(next, Ordering::Relaxed);
        next
    }

    pub fn is_running(&self) -> bool {
        self.state.running.load(Ordering::Relaxed)
    }

    pub fn set_mode(&self, mode: ClickRate) {
        self.state.mode.store(mode.as_u8(), Ordering::Relaxed);
    }

    pub fn set_humanize_random_delay(&self, enabled: bool) {
        self.state
            .humanize_random_delay
            .store(enabled, Ordering::Relaxed);
    }

    pub fn set_humanize_cursor_jitter(&self, enabled: bool) {
        self.state
            .humanize_cursor_jitter
            .store(enabled, Ordering::Relaxed);
    }

    pub fn set_maximum_burst(&self, burst: u8) {
        self.state
            .maximum_burst
            .store(burst.clamp(1, 5), Ordering::Relaxed);
    }

    pub fn click_count(&self) -> u64 {
        self.state.click_count.load(Ordering::Relaxed)
    }
}

impl Drop for ClickWorker {
    fn drop(&mut self) {
        self.state.shutdown.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn click_loop(state: Arc<ClickWorkerState>) {
    let mut fps_hz = 60.0;
    let mut fps_ticks = 0_u32;
    let mut maximum_loops = 0_u32;
    let mut pacer = PaceController::new();
    let mut humanizer = Humanizer::new();
    let mut last_mode: Option<ClickRate> = None;

    while !state.shutdown.load(Ordering::Relaxed) {
        if !state.running.load(Ordering::Relaxed) {
            pacer.reset();
            humanizer.reset_idle();
            thread::sleep(Duration::from_millis(10));
            continue;
        }

        let mode_raw = state.mode.load(Ordering::Relaxed);
        if mode_raw == ClickRate::Maximum.as_u8() {
            humanizer.reset_delay_window();
            run_maximum_burst(&state, &mut maximum_loops, &mut humanizer);
            last_mode = Some(ClickRate::Maximum);
            continue;
        }
        let mode = ClickRate::from_u8(mode_raw);
        if last_mode != Some(mode) {
            pacer.reset();
            humanizer.reset_delay_window();
            last_mode = Some(mode);
        }
        match mode {
            ClickRate::Fps => {
                if fps_ticks.is_multiple_of(300) {
                    fps_hz = current_monitor_refresh_hz()
                        .unwrap_or(60.0)
                        .clamp(1.0, 1_000.0);
                }
                fps_ticks = fps_ticks.wrapping_add(1);
                send_click_if_allowed(&state);
                pacer.wait_next(1.0 / fps_hz, &state, &mut humanizer, mode);
            }
            fixed => {
                if let Some(cps) = fixed.fixed_cps() {
                    let now = Instant::now();
                    let period = if state.humanize_random_delay.load(Ordering::Relaxed) {
                        humanizer.randomized_period(now, 1.0 / cps)
                    } else {
                        humanizer.reset_delay_window();
                        1.0 / cps
                    };
                    send_click_if_allowed(&state);
                    pacer.wait_next(period, &state, &mut humanizer, mode);
                }
            }
        }
    }
}

fn run_maximum_burst(
    state: &ClickWorkerState,
    maximum_loops: &mut u32,
    humanizer: &mut Humanizer,
) {
    let mut loops_since_poll = 0_u32;
    let mut hover_blocked = false;
    loop {
        let burst = state.maximum_burst.load(Ordering::Relaxed).clamp(1, 5);
        if hover_blocked {
            thread::sleep(Duration::from_millis(2));
        } else {
            let sent = send_left_click_burst(burst);
            if sent > 0 {
                state.click_count.fetch_add(u64::from(sent), Ordering::Relaxed);
            }
        }

        *maximum_loops = maximum_loops.wrapping_add(1);
        loops_since_poll = loops_since_poll.wrapping_add(1);

        if (*maximum_loops).is_multiple_of(MAXIMUM_YIELD_STRIDE) {
            thread::yield_now();
        }

        if loops_since_poll >= MAXIMUM_CONTROL_POLL_STRIDE {
            loops_since_poll = 0;
            hover_blocked = is_cursor_over_window(APP_WINDOW_TITLE);
            humanizer.tick_cursor_jitter(ClickRate::Maximum, state);
            if state.shutdown.load(Ordering::Relaxed)
                || !state.running.load(Ordering::Relaxed)
                || state.mode.load(Ordering::Relaxed) != ClickRate::Maximum.as_u8()
            {
                return;
            }
        }
    }
}

fn send_click_if_allowed(state: &ClickWorkerState) -> bool {
    if is_cursor_over_window(APP_WINDOW_TITLE) {
        return false;
    }
    send_left_click();
    state.click_count.fetch_add(1, Ordering::Relaxed);
    true
}

struct PaceController {
    next_deadline: Option<Instant>,
    ema_error_s: f64,
    last_period_s: f64,
}

impl PaceController {
    fn new() -> Self {
        Self {
            next_deadline: None,
            ema_error_s: 0.0,
            last_period_s: 0.0,
        }
    }

    fn reset(&mut self) {
        self.next_deadline = None;
        self.ema_error_s = 0.0;
        self.last_period_s = 0.0;
    }

    fn wait_next(
        &mut self,
        base_period_s: f64,
        state: &ClickWorkerState,
        humanizer: &mut Humanizer,
        mode: ClickRate,
    ) {
        if base_period_s <= 0.0 {
            return;
        }

        let now = Instant::now();
        let period_changed = (self.last_period_s - base_period_s).abs() > f64::EPSILON;
        if self.next_deadline.is_none() || period_changed {
            self.next_deadline = Some(now + Duration::from_secs_f64(base_period_s));
            self.ema_error_s = 0.0;
            self.last_period_s = base_period_s;
        }

        let deadline = self.next_deadline.unwrap_or(now);
        if !wait_until_interruptible(deadline, state, humanizer, mode) {
            return;
        }

        let woke = Instant::now();
        let error_s = if woke >= deadline {
            woke.duration_since(deadline).as_secs_f64()
        } else {
            -(deadline.duration_since(woke).as_secs_f64())
        };

        // Smooth timing error and compensate a fraction of it in the next period.
        self.ema_error_s = self.ema_error_s * 0.9 + error_s * 0.1;
        let correction_s = (self.ema_error_s * 0.5)
            .clamp(-base_period_s * 0.25, base_period_s * 0.45);
        let adjusted_period_s = (base_period_s - correction_s).max(0.000_5);

        let mut next = deadline + Duration::from_secs_f64(adjusted_period_s);
        if woke > next {
            let overdue_s = woke.duration_since(next).as_secs_f64();
            let skips = (overdue_s / base_period_s).floor() as u32 + 1;
            next += Duration::from_secs_f64(base_period_s * f64::from(skips));
        }
        self.next_deadline = Some(next);
    }
}

fn wait_until_interruptible(
    target: Instant,
    state: &ClickWorkerState,
    humanizer: &mut Humanizer,
    mode: ClickRate,
) -> bool {
    loop {
        if state.shutdown.load(Ordering::Relaxed) || !state.running.load(Ordering::Relaxed) {
            return false;
        }

        humanizer.tick_cursor_jitter(mode, state);

        let now = Instant::now();
        if now >= target {
            return true;
        }

        let remaining = target.saturating_duration_since(now);
        if remaining > Duration::from_millis(2) {
            thread::sleep(remaining - Duration::from_millis(1));
        } else if remaining > Duration::from_micros(200) {
            thread::yield_now();
        } else {
            hint::spin_loop();
        }
    }
}

#[derive(Clone, Copy)]
struct ScreenPoint {
    x: i32,
    y: i32,
}

struct Humanizer {
    rng: fastrand::Rng,
    delay_window_start: Instant,
    delay_balance_s: f64,
    jitter_anchor: Option<ScreenPoint>,
    jitter_expected: Option<ScreenPoint>,
    jitter_pause_until: Option<Instant>,
    next_jitter_at: Instant,
}

impl Humanizer {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            rng: fastrand::Rng::new(),
            delay_window_start: now,
            delay_balance_s: 0.0,
            jitter_anchor: None,
            jitter_expected: None,
            jitter_pause_until: None,
            next_jitter_at: now,
        }
    }

    fn reset_idle(&mut self) {
        self.reset_delay_window();
        self.jitter_anchor = None;
        self.jitter_expected = None;
        self.jitter_pause_until = None;
        self.next_jitter_at = Instant::now();
    }

    fn reset_delay_window(&mut self) {
        self.delay_window_start = Instant::now();
        self.delay_balance_s = 0.0;
    }

    fn randomized_period(&mut self, now: Instant, base_period_s: f64) -> f64 {
        if now.duration_since(self.delay_window_start) >= RANDOM_DELAY_WINDOW {
            self.delay_window_start = now;
            self.delay_balance_s = 0.0;
        }

        let min_offset = -base_period_s * 0.2;
        let max_offset = base_period_s * 0.5;
        let sample = self.rng.f64() * (max_offset - min_offset) + min_offset;
        let progress = now
            .duration_since(self.delay_window_start)
            .as_secs_f64()
            / RANDOM_DELAY_WINDOW.as_secs_f64();
        let correction_target = (-self.delay_balance_s).clamp(min_offset, max_offset);
        let correction_bias = (0.15 + progress * 0.7).clamp(0.15, 0.9);
        let offset = sample * (1.0 - correction_bias) + correction_target * correction_bias;

        self.delay_balance_s += offset;
        (base_period_s + offset).max(0.000_5)
    }

    fn tick_cursor_jitter(&mut self, mode: ClickRate, state: &ClickWorkerState) {
        if !state.humanize_cursor_jitter.load(Ordering::Relaxed) {
            self.jitter_anchor = None;
            self.jitter_expected = None;
            self.jitter_pause_until = None;
            self.next_jitter_at = Instant::now();
            return;
        }

        let now = Instant::now();
        if let Some(pause_until) = self.jitter_pause_until {
            if now < pause_until {
                return;
            }
            self.jitter_pause_until = None;
            self.jitter_anchor = None;
            self.jitter_expected = None;
        }

        if now < self.next_jitter_at {
            return;
        }

        let Some((cursor_x, cursor_y)) = cursor_position() else {
            self.next_jitter_at = now + Duration::from_millis(100);
            return;
        };
        let current = ScreenPoint {
            x: cursor_x,
            y: cursor_y,
        };

        let tolerance = self.manual_move_tolerance(mode);
        if let Some(expected) = self.jitter_expected {
            if Self::point_distance(current, expected) > tolerance {
                self.jitter_pause_until = Some(now + JITTER_PAUSE_ON_MANUAL_MOVE);
                self.jitter_anchor = Some(current);
                self.jitter_expected = None;
                self.next_jitter_at = now + JITTER_PAUSE_ON_MANUAL_MOVE;
                return;
            }
        }

        let anchor = self.jitter_anchor.unwrap_or(current);
        self.jitter_anchor = Some(anchor);

        let radius = self.jitter_radius(mode);
        let target = ScreenPoint {
            x: anchor.x + self.biased_offset(radius),
            y: anchor.y + self.biased_offset(radius),
        };

        if set_cursor_position(target.x, target.y) {
            self.jitter_expected = Some(target);
        } else {
            self.jitter_expected = None;
        }
        self.next_jitter_at = now + self.jitter_interval(mode);
    }

    fn jitter_interval(&self, mode: ClickRate) -> Duration {
        match mode {
            ClickRate::Slow => Duration::from_millis(180),
            ClickRate::Normal => Duration::from_millis(125),
            ClickRate::Fast => Duration::from_millis(95),
            ClickRate::Rapid => Duration::from_millis(78),
            ClickRate::Turbo => Duration::from_millis(64),
            ClickRate::Extreme => Duration::from_millis(50),
            ClickRate::Maximum => Duration::from_millis(42),
            ClickRate::Fps => Duration::from_millis(85),
        }
    }

    fn jitter_radius(&self, mode: ClickRate) -> i32 {
        match mode {
            ClickRate::Slow => 1,
            ClickRate::Normal => 2,
            ClickRate::Fast => 2,
            ClickRate::Rapid => 3,
            ClickRate::Turbo => 4,
            ClickRate::Extreme => 5,
            ClickRate::Maximum => 6,
            ClickRate::Fps => 3,
        }
    }

    fn manual_move_tolerance(&self, mode: ClickRate) -> i32 {
        self.jitter_radius(mode) + 2
    }

    fn biased_offset(&mut self, radius: i32) -> i32 {
        if radius <= 0 {
            return 0;
        }
        let a = self.rng.i32(-radius..=radius);
        let b = self.rng.i32(-radius..=radius);
        (a + b) / 2
    }

    fn point_distance(a: ScreenPoint, b: ScreenPoint) -> i32 {
        (a.x - b.x).abs().max((a.y - b.y).abs())
    }
}
