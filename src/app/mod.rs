mod config;
mod hotkey;
mod winutil;
mod worker;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;
use eframe::egui::{Align2, Color32, CornerRadius, FontId, Margin, RichText, Sense, Stroke};
use lucide_icons::{Icon, LUCIDE_FONT_BYTES};
use single_instance::SingleInstance;

use crate::app::config::AppConfig;
use crate::app::hotkey::{HotkeyBinding, HotkeyController, egui_key_to_code};
use crate::app::worker::{ClickRate, ClickWorker};

const APP_TITLE: &str = "Hermes Rapid Clicker";
const SINGLE_INSTANCE_ID: &str = "hermes_rapid_clicker_single_instance_v1";
const WINDOW_SIZE: [f32; 2] = [380.0, 240.0];
const LUCIDE_FONT_NAME: &str = "lucide-icons";
const VIEW_BODY_HEIGHT: f32 = 132.0;
const MAIN_CONTENT_HEIGHT: f32 = 120.0;
const MAIN_CONTENT_OFFSET_Y: f32 = 12.0;
const SETTING_CONTENT_HEIGHT: f32 = 164.0;
const SETTING_CONTENT_OFFSET_Y: f32 = -10.0;
const SETTING_VIEW_BOTTOM_MARGIN: i8 = 4;
const WINDOW_CHROME_HEIGHT: f32 = 79.0;

pub fn run() -> Result<(), String> {
    let instance = SingleInstance::new(SINGLE_INSTANCE_ID)
        .map_err(|err| format!("Single-instance initialization failed: {err}"))?;

    if !instance.is_single() {
        winutil::focus_existing_window(APP_TITLE);
        return Ok(());
    }

    let config_path = config_path_from_exe()?;
    let config = AppConfig::load(&config_path);
    config.save(&config_path)?;

    let mut viewport = egui::ViewportBuilder::default()
        .with_title(APP_TITLE)
        .with_inner_size(WINDOW_SIZE)
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false);
    if let Some(icon) = load_viewport_icon_data(include_bytes!("../assets/icon.png")) {
        viewport = viewport.with_icon(icon);
    }
    if let (Some(x), Some(y)) = (config.window_x, config.window_y) {
        viewport = viewport.with_position(egui::pos2(x as f32, y as f32));
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    let startup_config = config.clone();
    eframe::run_native(
        APP_TITLE,
        options,
        Box::new(move |cc| {
            install_icon_font(&cc.egui_ctx);
            Ok(Box::new(HermesApp::new(
                config_path.clone(),
                startup_config.clone(),
                &cc.egui_ctx,
            )))
        }),
    )
    .map_err(|err| format!("App runtime error: {err}"))?;

    Ok(())
}

fn config_path_from_exe() -> Result<PathBuf, String> {
    let exe =
        std::env::current_exe().map_err(|err| format!("Could not determine exe path: {err}"))?;
    Ok(exe.with_extension("ini"))
}

fn install_icon_font(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let lucide_family = egui::FontFamily::Name(LUCIDE_FONT_NAME.to_string().into());
    fonts.font_data.insert(
        LUCIDE_FONT_NAME.to_string(),
        egui::FontData::from_static(LUCIDE_FONT_BYTES).into(),
    );
    fonts
        .families
        .insert(lucide_family, vec![LUCIDE_FONT_NAME.to_string()]);
    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        family.push(LUCIDE_FONT_NAME.to_string());
    }
    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
        family.push(LUCIDE_FONT_NAME.to_string());
    }
    ctx.set_fonts(fonts);
}

fn load_title_icon_texture(
    ctx: &egui::Context,
    bytes: &[u8],
    texture_name: &str,
) -> Option<egui::TextureHandle> {
    let image = image::load_from_memory(bytes).ok()?.to_rgba8();
    // Pre-downsample with a high-quality filter so the small title icon stays crisp.
    let resized = image::imageops::resize(&image, 128, 128, image::imageops::FilterType::Lanczos3);
    let width = usize::try_from(resized.width()).ok()?;
    let height = usize::try_from(resized.height()).ok()?;
    let pixels = resized.as_flat_samples();
    let color_image = egui::ColorImage::from_rgba_unmultiplied([width, height], pixels.as_slice());
    Some(ctx.load_texture(
        texture_name,
        color_image,
        egui::TextureOptions::LINEAR.with_mipmap_mode(Some(egui::TextureFilter::Linear)),
    ))
}

fn load_viewport_icon_data(bytes: &[u8]) -> Option<Arc<egui::IconData>> {
    let image = image::load_from_memory(bytes).ok()?.to_rgba8();
    let width = image.width();
    let height = image.height();
    Some(Arc::new(egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    }))
}

struct HermesApp {
    config_path: PathBuf,
    config: AppConfig,
    worker: ClickWorker,
    title_icon_idle: Option<egui::TextureHandle>,
    title_icon_active: Option<egui::TextureHandle>,
    viewport_icon_idle: Option<Arc<egui::IconData>>,
    viewport_icon_active: Option<Arc<egui::IconData>>,
    hotkeys: Option<HotkeyController>,
    waiting_for_hotkey: bool,
    status_line: String,
    pin_dirty: bool,
    last_sample_time: Instant,
    last_sample_count: u64,
    smoothed_actual_rate: f64,
    recent_rates: VecDeque<f64>,
    theme_applied: bool,
    suppress_click_rate_until_mouse_up: bool,
    last_saved_window_pos: Option<(i32, i32)>,
    active_view: AppView,
    size_enforced: bool,
    last_running_state: bool,
    applied_opacity: u8,
    last_applied_window_size: Option<egui::Vec2>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AppView {
    Main,
    Settings,
}

impl HermesApp {
    fn new(config_path: PathBuf, config: AppConfig, egui_ctx: &egui::Context) -> Self {
        let mut status_line = String::new();
        let hotkeys = match HotkeyController::new(config.hotkey) {
            Ok(h) => Some(h),
            Err(err) => {
                status_line = err;
                None
            }
        };
        let worker = ClickWorker::new(
            config.click_rate,
            config.maximum_burst,
            config.humanize_random_delay,
            config.humanize_cursor_jitter,
        );
        let title_icon_idle = load_title_icon_texture(
            egui_ctx,
            include_bytes!("../assets/icon.png"),
            "app-title-icon-idle",
        );
        let title_icon_active = load_title_icon_texture(
            egui_ctx,
            include_bytes!("../assets/icon-active.png"),
            "app-title-icon-active",
        );
        let viewport_icon_idle = load_viewport_icon_data(include_bytes!("../assets/icon.png"));
        let viewport_icon_active =
            load_viewport_icon_data(include_bytes!("../assets/icon-active.png"));
        let last_running_state = worker.is_running();
        Self {
            config_path,
            config,
            worker,
            title_icon_idle,
            title_icon_active,
            viewport_icon_idle,
            viewport_icon_active,
            hotkeys,
            waiting_for_hotkey: false,
            status_line,
            pin_dirty: true,
            last_sample_time: Instant::now(),
            last_sample_count: 0,
            smoothed_actual_rate: 0.0,
            recent_rates: VecDeque::new(),
            theme_applied: false,
            suppress_click_rate_until_mouse_up: false,
            last_saved_window_pos: None,
            active_view: AppView::Main,
            size_enforced: false,
            last_running_state,
            applied_opacity: 0,
            last_applied_window_size: None,
        }
    }

    fn persist_config(&mut self) {
        if let Err(err) = self.config.save(&self.config_path) {
            self.status_line = err;
        }
    }

    fn process_hotkey_events(&mut self) {
        if self.waiting_for_hotkey {
            if let Some(hotkeys) = &self.hotkeys {
                // If the currently-bound hotkey is pressed while listening,
                // treat it as a no-op capture and exit listening state.
                if hotkeys.poll_toggle_event() {
                    self.waiting_for_hotkey = false;
                    self.status_line.clear();
                }
                while hotkeys.poll_toggle_event() {}
            }
            return;
        }
        if let Some(hotkeys) = &self.hotkeys {
            if hotkeys.poll_toggle_event() {
                self.worker.toggle_running();
            }
        }
    }

    fn update_actual_rate(&mut self) {
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.last_sample_time);
        if elapsed < Duration::from_millis(200) {
            return;
        }

        let count = self.worker.click_count();
        let delta = count.saturating_sub(self.last_sample_count);
        let rate = delta as f64 / elapsed.as_secs_f64();
        self.recent_rates.push_back(rate);
        while self.recent_rates.len() > 6 {
            self.recent_rates.pop_front();
        }
        self.smoothed_actual_rate =
            self.recent_rates.iter().copied().sum::<f64>() / self.recent_rates.len() as f64;
        self.last_sample_time = now;
        self.last_sample_count = count;
    }

    fn apply_pin(&mut self, ctx: &egui::Context) {
        if !self.pin_dirty {
            return;
        }
        let level = if self.config.pinned {
            egui::WindowLevel::AlwaysOnTop
        } else {
            egui::WindowLevel::Normal
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
        self.pin_dirty = false;
    }

    fn apply_theme(&mut self, ctx: &egui::Context) {
        if self.theme_applied && self.applied_opacity == self.config.background_opacity {
            return;
        }

        let mut style = (*ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.spacing.item_spacing = egui::vec2(10.0, 10.0);
        style.spacing.button_padding = egui::vec2(10.0, 6.0);
        style.spacing.window_margin = Margin::same(0);

        let visuals = &mut style.visuals;
        visuals.panel_fill = Color32::TRANSPARENT;
        visuals.window_fill = self.window_bg_color();
        visuals.extreme_bg_color = Color32::from_rgb(10, 10, 10);
        visuals.faint_bg_color = Color32::from_rgba_unmultiplied(255, 255, 255, 8);
        visuals.window_corner_radius = CornerRadius::same(14);
        visuals.menu_corner_radius = CornerRadius::same(10);
        visuals.window_stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(90, 90, 90, 64));
        visuals.widgets.noninteractive.corner_radius = CornerRadius::same(8);
        visuals.widgets.inactive.corner_radius = CornerRadius::same(8);
        visuals.widgets.hovered.corner_radius = CornerRadius::same(8);
        visuals.widgets.active.corner_radius = CornerRadius::same(8);
        visuals.widgets.open.corner_radius = CornerRadius::same(8);
        visuals.widgets.inactive.bg_fill = Color32::from_rgb(33, 33, 33);
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(42, 42, 42);
        visuals.widgets.active.bg_fill = Color32::from_rgb(52, 52, 52);
        visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(28, 28, 28);
        visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(36, 36, 36);
        visuals.widgets.active.weak_bg_fill = Color32::from_rgb(46, 46, 46);
        visuals.widgets.inactive.fg_stroke.color = Color32::from_rgb(210, 214, 220);
        visuals.widgets.hovered.fg_stroke.color = Color32::from_rgb(235, 238, 242);
        visuals.selection.bg_fill = Color32::from_rgb(48, 130, 76);
        visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(126, 210, 150));

        ctx.set_style(style);
        self.theme_applied = true;
        self.applied_opacity = self.config.background_opacity;
    }

    fn window_bg_color(&self) -> Color32 {
        let alpha = ((self.config.background_opacity as f32 / 100.0) * 255.0)
            .round()
            .clamp(0.0, 255.0) as u8;
        Color32::from_rgba_unmultiplied(18, 18, 18, alpha)
    }

    fn sync_running_visuals(&mut self, ctx: &egui::Context) {
        let running = self.worker.is_running();
        if running == self.last_running_state {
            return;
        }

        let next_icon = if running {
            self.viewport_icon_active
                .clone()
                .or_else(|| self.viewport_icon_idle.clone())
        } else {
            self.viewport_icon_idle
                .clone()
                .or_else(|| self.viewport_icon_active.clone())
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Icon(next_icon));
        self.last_running_state = running;
    }

    fn draw_title_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let row_height = 30.0;
        let available_width = ui.available_width();
        ui.allocate_ui_with_layout(
            egui::vec2(available_width, row_height),
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                let old_spacing = ui.spacing().item_spacing;
                ui.spacing_mut().item_spacing.x = 2.0;
                ui.spacing_mut().item_spacing.y = 0.0;

                let button_size = egui::vec2(15.4, 30.8);
                let spacing = 8.0;
                let controls_width = button_size.x * 4.0 + spacing * 3.0;
                let drag_width = (ui.available_width() - controls_width).max(0.0);

                let (drag_rect, drag_response) = ui.allocate_exact_size(
                    egui::vec2(drag_width, row_height),
                    Sense::click_and_drag(),
                );
                if drag_response.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }

                let icon_size = 42.0;
                let center_y = drag_rect.center().y;
                let title_block_left = drag_rect.left();
                let title_icon = if self.worker.is_running() {
                    self.title_icon_active
                        .as_ref()
                        .or(self.title_icon_idle.as_ref())
                } else {
                    self.title_icon_idle
                        .as_ref()
                        .or(self.title_icon_active.as_ref())
                };
                let title_left = if let Some(icon) = title_icon {
                    let icon_rect = egui::Rect::from_center_size(
                        egui::pos2(title_block_left + icon_size * 0.5, center_y + 2.0),
                        egui::vec2(icon_size, icon_size),
                    );
                    ui.painter().image(
                        icon.id(),
                        icon_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                    title_block_left + icon_size + 6.0
                } else {
                    title_block_left
                };
                let title_pos = egui::pos2(title_left - 1.0, center_y - 7.0);
                ui.painter().text(
                    title_pos,
                    Align2::LEFT_CENTER,
                    "Hermes",
                    FontId::proportional(19.1),
                    Color32::from_rgb(232, 232, 232),
                );
                // Faux-bold pass for painter text.
                ui.painter().text(
                    title_pos + egui::vec2(0.6, 0.0),
                    Align2::LEFT_CENTER,
                    "Hermes",
                    FontId::proportional(19.1),
                    Color32::from_rgb(232, 232, 232),
                );
                let running = self.worker.is_running();
                let (state_text, state_fg, state_bg, state_dot) = if running {
                    (
                        "ON",
                        Color32::from_rgb(140, 235, 164),
                        Color32::from_rgba_unmultiplied(28, 78, 44, 220),
                        Color32::from_rgb(126, 224, 154),
                    )
                } else {
                    (
                        "OFF",
                        Color32::from_rgb(186, 190, 196),
                        Color32::from_rgba_unmultiplied(46, 46, 46, 220),
                        Color32::from_rgb(134, 138, 144),
                    )
                };
                let badge_size = egui::vec2(54.0, 18.0);
                let badge_bottom_y = title_pos.y + 10.0;
                let badge_rect = egui::Rect::from_min_size(
                    egui::pos2(title_pos.x + 98.0 - (badge_size.x * 0.5), badge_bottom_y - badge_size.y),
                    badge_size,
                );
                ui.painter()
                    .rect_filled(badge_rect, CornerRadius::same(5), state_bg);
                ui.painter().rect_stroke(
                    badge_rect,
                    CornerRadius::same(5),
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 20)),
                    egui::StrokeKind::Inside,
                );
                ui.painter().circle_filled(
                    egui::pos2(badge_rect.left() + 10.0, badge_rect.center().y),
                    3.0,
                    state_dot,
                );
                ui.painter().text(
                    egui::pos2(badge_rect.center().x + 4.0, badge_rect.center().y),
                    Align2::CENTER_CENTER,
                    state_text,
                    FontId::proportional(11.8),
                    state_fg,
                );
                ui.painter().text(
                    egui::pos2(title_left - 1.0, center_y + 13.0),
                    Align2::LEFT_CENTER,
                    "Rapid Clicker",
                    FontId::proportional(14.0),
                    Color32::from_rgb(168, 168, 168),
                );

                let hover_bg = Color32::from_rgba_unmultiplied(38, 38, 38, 190);
                let active_bg = Color32::from_rgba_unmultiplied(48, 48, 48, 210);
                let (controls_rect, _) =
                    ui.allocate_exact_size(egui::vec2(controls_width, row_height), Sense::hover());
                let controls_rect = controls_rect.translate(egui::vec2(0.0, -8.0));
                let mut controls_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(controls_rect)
                        .layout(egui::Layout::left_to_right(egui::Align::Min)),
                );
                controls_ui.spacing_mut().item_spacing.x = spacing;
                controls_ui.spacing_mut().item_spacing.y = 0.0;

                let min_resp = title_icon_button(
                    &mut controls_ui,
                    button_size,
                    &icon_char(Icon::SquareMinus),
                    Color32::from_rgb(210, 214, 220),
                    hover_bg,
                    active_bg,
                    23.8,
                )
                .on_hover_text("Minimize");
                if min_resp.clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }

                let pin_resp = title_icon_button(
                    &mut controls_ui,
                    button_size,
                    &icon_char(Icon::SquareDot),
                    if self.config.pinned {
                        Color32::from_rgb(120, 214, 148)
                    } else {
                        Color32::from_gray(170)
                    },
                    hover_bg,
                    active_bg,
                    23.8,
                )
                .on_hover_text(if self.config.pinned {
                    "Unpin window"
                } else {
                    "Pin window on top"
                });
                if pin_resp.clicked() {
                    self.config.pinned = !self.config.pinned;
                    self.pin_dirty = true;
                    self.persist_config();
                }

                let settings_resp = title_icon_button(
                    &mut controls_ui,
                    button_size,
                    &icon_char(Icon::SquareChartGantt),
                    if self.active_view == AppView::Settings {
                        Color32::from_rgb(124, 176, 236)
                    } else {
                        Color32::from_rgb(200, 204, 210)
                    },
                    hover_bg,
                    active_bg,
                    23.8,
                )
                .on_hover_text("Settings");
                if settings_resp.clicked() {
                    self.active_view = if self.active_view == AppView::Settings {
                        AppView::Main
                    } else {
                        AppView::Settings
                    };
                }

                let close_resp = title_icon_button(
                    &mut controls_ui,
                    button_size,
                    &icon_char(Icon::SquareX),
                    Color32::from_rgb(230, 135, 135),
                    hover_bg,
                    active_bg,
                    23.8,
                )
                .on_hover_text("Close");
                if close_resp.clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }

                if self.active_view == AppView::Settings {
                    let back_size = egui::vec2(56.0, 16.0);
                    let back_rect = egui::Rect::from_min_size(
                        egui::pos2(
                            controls_rect.right() - back_size.x,
                            controls_rect.bottom() + 2.0,
                        ),
                        back_size,
                    );
                    let back_resp = ui.interact(back_rect, ui.id().with("settings_back_link"), Sense::click());
                    let back_color = if back_resp.hovered() {
                        Color32::from_rgb(210, 214, 220)
                    } else {
                        Color32::from_rgb(162, 166, 172)
                    };
                    ui.painter().text(
                        back_rect.right_center(),
                        Align2::RIGHT_CENTER,
                        "< Back",
                        FontId::proportional(12.0),
                        back_color,
                    );
                    if back_resp.clicked() {
                        self.active_view = AppView::Main;
                    }
                }

                ui.spacing_mut().item_spacing = old_spacing;
            },
        );
    }

    fn begin_hotkey_capture(&mut self) {
        self.waiting_for_hotkey = true;
        self.status_line.clear();
    }

    fn poll_hotkey_capture(&mut self, ctx: &egui::Context) {
        if !self.waiting_for_hotkey {
            return;
        }

        let events = ctx.input(|input| input.events.clone());
        for event in events {
            if let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = event
            {
                if key == egui::Key::Escape {
                    self.waiting_for_hotkey = false;
                    self.status_line.clear();
                    return;
                }

                let Some(code) = egui_key_to_code(key) else {
                    continue;
                };

                let mut mod_keys = global_hotkey::hotkey::Modifiers::empty();
                if modifiers.ctrl || modifiers.command {
                    mod_keys |= global_hotkey::hotkey::Modifiers::CONTROL;
                }
                if modifiers.alt {
                    mod_keys |= global_hotkey::hotkey::Modifiers::ALT;
                }
                if modifiers.shift {
                    mod_keys |= global_hotkey::hotkey::Modifiers::SHIFT;
                }

                let binding = HotkeyBinding {
                    modifiers: mod_keys,
                    code,
                };
                self.waiting_for_hotkey = false;
                self.apply_hotkey_binding(binding);
                return;
            }
        }
    }

    fn apply_hotkey_binding(&mut self, binding: HotkeyBinding) {
        if let Some(hotkeys) = self.hotkeys.as_mut() {
            match hotkeys.update_binding(binding) {
                Ok(()) => {
                    self.config.hotkey = binding;
                    self.persist_config();
                    self.status_line.clear();
                }
                Err(err) => self.status_line = err,
            }
            return;
        }

        match HotkeyController::new(binding) {
            Ok(h) => {
                self.hotkeys = Some(h);
                self.config.hotkey = binding;
                self.persist_config();
                self.status_line.clear();
            }
            Err(err) => self.status_line = err,
        }
    }

    fn persist_window_position_if_changed(&mut self, ctx: &egui::Context) {
        let Some(outer_rect) = ctx.input(|input| input.viewport().outer_rect) else {
            return;
        };
        let x = outer_rect.min.x.round() as i32;
        let y = outer_rect.min.y.round() as i32;
        let new_pos = (x, y);
        if self.last_saved_window_pos == Some(new_pos) {
            return;
        }
        self.last_saved_window_pos = Some(new_pos);
        self.config.window_x = Some(x);
        self.config.window_y = Some(y);
        self.persist_config();
    }

    fn enforce_window_size_once(&mut self, ctx: &egui::Context) {
        if self.size_enforced {
            return;
        }
        let size = self.desired_window_size();
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
        self.last_applied_window_size = Some(size);
        self.size_enforced = true;
    }

    fn desired_window_size(&self) -> egui::Vec2 {
        match self.active_view {
            AppView::Main => egui::vec2(WINDOW_SIZE[0], WINDOW_SIZE[1]),
            AppView::Settings => {
                let h = (WINDOW_CHROME_HEIGHT + SETTING_CONTENT_OFFSET_Y + SETTING_CONTENT_HEIGHT)
                    .max(WINDOW_SIZE[1]);
                egui::vec2(WINDOW_SIZE[0], h)
            }
        }
    }

    fn sync_window_size_for_view(&mut self, ctx: &egui::Context) {
        let desired = self.desired_window_size();
        if self.last_applied_window_size == Some(desired) {
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(desired));
        self.last_applied_window_size = Some(desired);
    }
}

fn icon_char(icon: Icon) -> String {
    char::from(icon).to_string()
}

fn title_icon_button(
    ui: &mut egui::Ui,
    size: egui::Vec2,
    icon: &str,
    icon_color: Color32,
    hover_bg: Color32,
    active_bg: Color32,
    icon_size: f32,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    if response.is_pointer_button_down_on() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), active_bg);
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), hover_bg);
    }

    let icon_draw_color = if response.hovered() || response.is_pointer_button_down_on() {
        icon_color
    } else {
        Color32::from_rgba_unmultiplied(
            icon_color.r(),
            icon_color.g(),
            icon_color.b(),
            ((icon_color.a() as f32) * 0.1).round().clamp(0.0, 255.0) as u8,
        )
    };

    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        icon,
        FontId::new(
            icon_size,
            egui::FontFamily::Name(LUCIDE_FONT_NAME.to_string().into()),
        ),
        icon_draw_color,
    );

    response
}

fn fieldset_card(
    ui: &mut egui::Ui,
    title: &str,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    fieldset_card_with_top_spacing(ui, title, None, add_contents);
}

fn fieldset_card_with_top_spacing(
    ui: &mut egui::Ui,
    title: &str,
    top_spacing: Option<f32>,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let fill = Color32::from_rgba_unmultiplied(0, 0, 0, 44);
    let stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(110, 110, 110, 46));
    let title_color = Color32::from_rgb(176, 180, 186);
    let title_font = FontId::proportional(11.5);

    let legend_galley = ui
        .painter()
        .layout_no_wrap(title.to_owned(), title_font.clone(), title_color);
    let legend_w = legend_galley.size().x;
    let legend_h = legend_galley.size().y;
    let legend_pad_x = 6.0;
    let legend_pad_y = 1.0;
    let legend_left_margin = 10.0;
    let legend_overlap = 0.5;

    let legend_top_spacing = top_spacing.unwrap_or_else(|| (legend_h * 0.55).max(6.0));
    if legend_top_spacing > 0.0 {
        ui.add_space(legend_top_spacing);
    }
    let frame = egui::Frame::new()
        .fill(fill)
        .stroke(stroke)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            add_contents(ui);
        });

    let rect = frame.response.rect;
    let legend_bg = ui.visuals().window_fill;
    let legend_rect = egui::Rect::from_min_size(
        egui::pos2(
            rect.left() + legend_left_margin,
            rect.top() - (legend_h * legend_overlap) - legend_pad_y,
        ),
        egui::vec2(legend_w + legend_pad_x * 2.0, legend_h + legend_pad_y * 2.0),
    );
    ui.painter()
        .rect_filled(legend_rect, CornerRadius::same(3), legend_bg);
    let legend_pos = egui::pos2(
        (legend_rect.left() + legend_pad_x).round(),
        (legend_rect.top() + legend_pad_y).round(),
    );
    ui.painter()
        .galley(legend_pos, legend_galley.clone(), title_color);
    // Light faux-bold pass shared by main-view and settings-view cards.
    ui.painter().galley(
        legend_pos + egui::vec2(1.0, 0.0),
        legend_galley.clone(),
        title_color,
    );
    ui.painter().galley(
        legend_pos + egui::vec2(0.0, 1.0),
        legend_galley,
        title_color,
    );
}

fn settings_centered_column(
    ui: &mut egui::Ui,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let total_width = ui.available_width();
    let column_width = (total_width * 0.9).min(320.0).max(220.0).min(total_width);
    let side_space = (total_width - column_width).max(0.0) * 0.5;

    ui.horizontal(|ui| {
        ui.add_space(side_space);
        ui.allocate_ui_with_layout(
            egui::vec2(column_width, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            add_contents,
        );
    });
}

fn settings_centered_width(
    ui: &mut egui::Ui,
    width: f32,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let total_width = ui.available_width();
    let content_width = width.max(180.0).min(total_width);
    let side_space = (total_width - content_width).max(0.0) * 0.5;

    ui.horizontal(|ui| {
        ui.add_space(side_space);
        ui.allocate_ui_with_layout(
            egui::vec2(content_width, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            add_contents,
        );
    });
}

fn settings_footer(ui: &mut egui::Ui, version: &str, author: &str) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let total_width = ui.available_width();
        let line_width = (total_width * 0.5).max(30.0).min(total_width);
        let side_space = (total_width - line_width).max(0.0) * 0.5;
        ui.add_space(side_space);
        ui.add_sized(
            egui::vec2(line_width, 6.0),
            egui::Separator::default().spacing(0.0),
        );
    });
    ui.add_space(1.0);
    ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        ui.label(
            RichText::new(version)
                .size(11.5)
                .color(Color32::from_rgb(150, 154, 160)),
        );
        ui.label(
            RichText::new(author)
                .size(11.5)
                .color(Color32::from_rgb(150, 154, 160)),
        );
    });
}

fn settings_toggle_switch(ui: &mut egui::Ui, value: &mut bool) -> egui::Response {
    let desired_size = egui::vec2(36.0, 20.0);
    let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::click());
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }

    let radius = rect.height() * 0.5;
    let knob_radius = radius - 2.5;
    let is_hovered = response.hovered();
    let is_pressed = response.is_pointer_button_down_on();
    let track_fill = if *value {
        if is_pressed {
            Color32::from_rgb(46, 118, 68)
        } else if is_hovered {
            Color32::from_rgb(58, 140, 82)
        } else {
            Color32::from_rgb(52, 130, 76)
        }
    } else {
        if is_pressed {
            Color32::from_rgb(66, 66, 66)
        } else if is_hovered {
            Color32::from_rgb(74, 74, 74)
        } else {
            Color32::from_rgb(56, 56, 56)
        }
    };
    let track_stroke = if *value {
        if is_hovered {
            Color32::from_rgba_unmultiplied(132, 204, 154, 190)
        } else {
            Color32::from_rgba_unmultiplied(110, 184, 132, 170)
        }
    } else {
        if is_hovered {
            Color32::from_rgba_unmultiplied(255, 255, 255, 42)
        } else {
            Color32::from_rgba_unmultiplied(255, 255, 255, 24)
        }
    };
    let knob_fill = if *value {
        Color32::from_rgb(228, 235, 232)
    } else {
        Color32::from_rgb(182, 186, 192)
    };
    let knob_x = if *value {
        rect.right() - radius
    } else {
        rect.left() + radius
    };

    ui.painter().rect_filled(rect, CornerRadius::same(radius as u8), track_fill);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(radius as u8),
        Stroke::new(1.0, track_stroke),
        egui::StrokeKind::Inside,
    );
    ui.painter().circle_filled(egui::pos2(knob_x, rect.center().y), knob_radius, knob_fill);

    response
}

fn settings_toggle_row(ui: &mut egui::Ui, label: &str, value: &mut bool) {
    let switch_width = 36.0;
    let gap_width = 10.0;
    let row_height = 24.0;
    let width = ui.available_width();
    let (row_rect, row_response) = ui.allocate_exact_size(egui::vec2(width, row_height), Sense::click());
    if row_response.clicked() {
        *value = !*value;
    }

    let is_hovered = row_response.hovered();
    let is_pressed = row_response.is_pointer_button_down_on();
    if is_hovered {
        let hover_fill = if is_pressed {
            Color32::from_rgba_unmultiplied(255, 255, 255, 8)
        } else {
            Color32::from_rgba_unmultiplied(255, 255, 255, 4)
        };
        ui.painter()
            .rect_filled(row_rect, CornerRadius::same(6), hover_fill);
    }

    let switch_rect = egui::Rect::from_center_size(
        egui::pos2(
            row_rect.right() - switch_width * 0.5,
            row_rect.center().y,
        ),
        egui::vec2(switch_width, 20.0),
    );
    let label_rect = egui::Rect::from_min_max(
        row_rect.min,
        egui::pos2(switch_rect.left() - gap_width, row_rect.max.y),
    );

    let label_color = if is_hovered {
        Color32::from_rgb(220, 224, 230)
    } else {
        Color32::from_rgb(208, 212, 218)
    };
    ui.painter().text(
        egui::pos2(label_rect.left() + 4.0, row_rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(12.0),
        label_color,
    );

    let mut switch_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(switch_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let _ = settings_toggle_switch(&mut switch_ui, value);
}

impl Drop for HermesApp {
    fn drop(&mut self) {
        self.worker.set_running(false);
        let _ = self.config.save(&self.config_path);
    }
}

impl eframe::App for HermesApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.suppress_click_rate_until_mouse_up && !ctx.input(|i| i.pointer.primary_down()) {
            self.suppress_click_rate_until_mouse_up = false;
        }
        self.apply_theme(ctx);
        self.process_hotkey_events();
        self.poll_hotkey_capture(ctx);
        self.apply_pin(ctx);
        self.sync_running_visuals(ctx);
        self.enforce_window_size_once(ctx);
        self.sync_window_size_for_view(ctx);
        self.update_actual_rate();
        self.persist_window_position_if_changed(ctx);

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(Color32::TRANSPARENT)
                    .inner_margin(6),
            )
            .show(ctx, |ui| {
                let full_area = ui.available_size();
                let frame_inner_margin = 10.0;
                let frame_margin = if self.active_view == AppView::Settings {
                    Margin {
                        left: frame_inner_margin as i8,
                        right: frame_inner_margin as i8,
                        top: frame_inner_margin as i8,
                        bottom: SETTING_VIEW_BOTTOM_MARGIN,
                    }
                } else {
                    Margin::same(frame_inner_margin as i8)
                };
                egui::Frame::new()
                    .fill(self.window_bg_color())
                    .stroke(ui.visuals().window_stroke)
                    .corner_radius(CornerRadius::same(14))
                    .inner_margin(frame_margin)
                    .show(ui, |ui| {
                        // Keep the visible themed frame stretched to the whole native window.
                        ui.set_min_size(egui::vec2(
                            (full_area.x - frame_inner_margin * 2.0).max(0.0),
                            (full_area.y - frame_inner_margin * 2.0).max(0.0),
                        ));
                        self.draw_title_bar(ui, ctx);
                        ui.add_space(2.0);
                        ui.separator();
                        ui.add_space(2.0);

                        let body_height = if self.active_view == AppView::Settings {
                            (SETTING_CONTENT_OFFSET_Y + SETTING_CONTENT_HEIGHT).max(VIEW_BODY_HEIGHT)
                        } else {
                            VIEW_BODY_HEIGHT
                        };
                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), body_height),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| match self.active_view {
                            AppView::Main => {
                                ui.horizontal(|ui| {
                                    let total_width = ui.available_width();
                                    let left_width = (total_width * 0.48).max(80.0);
                                    let divider_width = 10.0;
                                    let horizontal_gaps = ui.spacing().item_spacing.x * 2.0;
                                    let right_width =
                                        (total_width - left_width - divider_width - horizontal_gaps)
                                            .max(120.0);
                                    let pane_offset_y = MAIN_CONTENT_OFFSET_Y;

                                    let (left_rect, _) = ui.allocate_exact_size(
                                        egui::vec2(left_width, MAIN_CONTENT_HEIGHT),
                                        Sense::hover(),
                                    );
                                    let left_rect = left_rect.translate(egui::vec2(0.0, pane_offset_y));
                                    let mut left_ui = ui.new_child(
                                        egui::UiBuilder::new()
                                            .max_rect(left_rect)
                                            .layout(egui::Layout::bottom_up(egui::Align::Max)),
                                    );
                                    {
                                        let actual = self.smoothed_actual_rate.max(0.0);
                                        let number_text = format!("{:.0}", actual);
                                        let digits = number_text.len();
                                        let dynamic_size =
                                            (76.0 - 16.0 * (digits.saturating_sub(3) as f32)).max(28.0);
                                        let split_two_rows = dynamic_size <= 28.0 && digits >= 10;

                                        left_ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Max),
                                            |ui| {
                                                ui.label(
                                                    RichText::new("Current clicks per second")
                                                        .size(13.0)
                                                        .color(Color32::from_rgb(160, 164, 170)),
                                                );
                                            },
                                        );

                                        left_ui.add_space(2.0);
                                        left_ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Max),
                                            |ui| {
                                                if split_two_rows {
                                                    let split_at = (digits + 1) / 2;
                                                    let (top, bottom) = number_text.split_at(split_at);
                                                    ui.vertical(|ui| {
                                                        ui.spacing_mut().item_spacing.y = -16.0;
                                                        ui.label(
                                                            RichText::new(top)
                                                                .size(28.0)
                                                                .color(Color32::from_rgb(225, 228, 232))
                                                                .strong(),
                                                        );
                                                        ui.label(
                                                            RichText::new(bottom)
                                                                .size(28.0)
                                                                .color(Color32::from_rgb(225, 228, 232))
                                                                .strong(),
                                                        );
                                                    });
                                                } else {
                                                    ui.label(
                                                        RichText::new(number_text)
                                                            .size(dynamic_size)
                                                            .color(Color32::from_rgb(225, 228, 232))
                                                            .strong(),
                                                    );
                                                }
                                            },
                                        );
                                    }

                                    let (sep_rect, _) = ui.allocate_exact_size(
                                        egui::vec2(divider_width, MAIN_CONTENT_HEIGHT),
                                        Sense::hover(),
                                    );
                                    let sep_rect = sep_rect.translate(egui::vec2(0.0, pane_offset_y));
                                    let mut sep_ui = ui.new_child(
                                        egui::UiBuilder::new()
                                            .max_rect(sep_rect)
                                            .layout(egui::Layout::top_down(egui::Align::Center)),
                                    );
                                    sep_ui.add_sized(
                                        egui::vec2(divider_width, MAIN_CONTENT_HEIGHT),
                                        egui::Separator::default().vertical(),
                                    );

                                    ui.allocate_ui_with_layout(
                                        egui::vec2(right_width, MAIN_CONTENT_HEIGHT),
                                        egui::Layout::top_down(egui::Align::Min),
                                        |ui| {
                                            let mut next_rate = self.config.click_rate;
                                            fieldset_card(ui, "Click Rate", |ui| {
                                                ui.spacing_mut().item_spacing.y = 4.0;
                                                let dropdown_width = (ui.available_width() * 0.8).max(77.0);
                                                if !self.suppress_click_rate_until_mouse_up {
                                                    let mut selection_clicked = false;
                                                    egui::ComboBox::from_id_salt("click_rate")
                                                        .width(dropdown_width)
                                                        .selected_text(
                                                            self.config.click_rate.compact_label(),
                                                        )
                                                        .show_ui(ui, |ui| {
                                                            for rate in ClickRate::ALL {
                                                                let response = ui.selectable_value(
                                                                    &mut next_rate,
                                                                    rate,
                                                                    rate.label(),
                                                                );
                                                                if response.clicked() {
                                                                    selection_clicked = true;
                                                                }
                                                            }
                                                        });
                                                    if selection_clicked {
                                                        ui.memory_mut(|mem| mem.close_popup());
                                                        self.suppress_click_rate_until_mouse_up = true;
                                                    }
                                                    if next_rate != self.config.click_rate {
                                                        self.config.click_rate = next_rate;
                                                        self.worker.set_mode(next_rate);
                                                        self.persist_config();
                                                    }
                                                } else {
                                                    ui.add_enabled_ui(false, |ui| {
                                                        ui.add_sized(
                                                            egui::vec2(
                                                                dropdown_width,
                                                                ui.spacing().interact_size.y,
                                                            ),
                                                            egui::Button::new(
                                                                self.config.click_rate.compact_label(),
                                                            ),
                                                        );
                                                    });
                                                }
                                            });

                                            ui.add_space(2.0);
                                            fieldset_card(ui, "Hotkey", |ui| {
                                                ui.spacing_mut().item_spacing.y = 6.0;
                                                egui::Frame::new()
                                                    .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 14))
                                                    .corner_radius(CornerRadius::same(99))
                                                    .inner_margin(Margin::symmetric(8, 3))
                                                    .show(ui, |ui| {
                                                        ui.label(
                                                            RichText::new(self.config.hotkey.display_text())
                                                                .size(12.0)
                                                                .color(Color32::from_rgb(214, 218, 224)),
                                                        );
                                                    });

                                                ui.horizontal(|ui| {
                                                    ui.spacing_mut().item_spacing.x = 6.0;
                                                    let hotkey_button = egui::Button::new(
                                                        if self.waiting_for_hotkey {
                                                            "ESC to cancel..."
                                                        } else {
                                                            "Change"
                                                        },
                                                    );
                                                    let hotkey_button = if self.waiting_for_hotkey {
                                                        hotkey_button.fill(Color32::from_rgb(122, 64, 74))
                                                    } else {
                                                        hotkey_button
                                                    };
                                                    if ui
                                                        .add_sized(
                                                            egui::vec2(82.0, 24.0),
                                                            hotkey_button,
                                                        )
                                                        .clicked()
                                                    {
                                                        self.begin_hotkey_capture();
                                                    }
                                                    if self.waiting_for_hotkey {
                                                        ui.label(
                                                            RichText::new("")
                                                                .size(12.0)
                                                                .color(Color32::from_rgb(132, 136, 142)),
                                                        );
                                                    }
                                                });
                                            });
                                        },
                                    );
                                });
                            }
                            AppView::Settings => {
                                ui.add_space(SETTING_CONTENT_OFFSET_Y);
                                let effective_setting_height = SETTING_CONTENT_HEIGHT.max(24.0);
                                ui.allocate_ui_with_layout(
                                    egui::vec2(ui.available_width(), effective_setting_height),
                                    egui::Layout::top_down(egui::Align::Min),
                                    |ui| {
                                        egui::Frame::new()
                                            .fill(Color32::TRANSPARENT)
                                            .stroke(Stroke::NONE)
                                            .inner_margin(Margin::same(0))
                                            .show(ui, |ui| {
                                        egui::ScrollArea::vertical()
                                            .id_salt("settings_scroll")
                                            .auto_shrink([false; 2])
                                            .show(ui, |ui| {
                                                settings_centered_column(ui, |ui| {
                                                    let humanize_width =
                                                        (ui.available_width() - 26.0).max(240.0);
                                                    settings_centered_width(ui, humanize_width, |ui| {
                                                        fieldset_card_with_top_spacing(
                                                            ui,
                                                            "Humanize",
                                                            Some(0.0),
                                                            |ui| {
                                                            ui.spacing_mut().item_spacing.y = 4.0;
                                                            ui.add(
                                                                egui::Label::new(
                                                                    RichText::new(
                                                                        "Add human-like randomness in actions to avoid simple behavioural detection.",
                                                                    )
                                                                    .size(11.5)
                                                                    .color(Color32::from_rgb(138, 142, 148)),
                                                                )
                                                                .wrap(),
                                                            );
                                                            ui.add_space(2.0);

                                                            let mut humanize_random_delay =
                                                                self.config.humanize_random_delay;
                                                            let mut humanize_cursor_jitter =
                                                                self.config.humanize_cursor_jitter;
                                                            settings_toggle_row(
                                                                ui,
                                                                "Randomize delay between clicks",
                                                                &mut humanize_random_delay,
                                                            );
                                                            ui.add_space(2.0);
                                                            settings_toggle_row(
                                                                ui,
                                                                "Jitter cursor position",
                                                                &mut humanize_cursor_jitter,
                                                            );
                                                            ui.add_space(2.0);

                                                            if humanize_random_delay
                                                                != self.config.humanize_random_delay
                                                                || humanize_cursor_jitter
                                                                    != self.config.humanize_cursor_jitter
                                                            {
                                                                self.config.humanize_random_delay =
                                                                    humanize_random_delay;
                                                                self.config.humanize_cursor_jitter =
                                                                    humanize_cursor_jitter;
                                                                self.worker.set_humanize_random_delay(
                                                                    humanize_random_delay,
                                                                );
                                                                self.worker.set_humanize_cursor_jitter(
                                                                    humanize_cursor_jitter,
                                                                );
                                                                self.persist_config();
                                                            }
                                                        },
                                                        );
                                                    });

                                                    settings_centered_width(ui, humanize_width, |ui| {
                                                        fieldset_card_with_top_spacing(
                                                            ui,
                                                            "Maximum Mode",
                                                            Some(2.0),
                                                            |ui| {
                                                            ui.spacing_mut().item_spacing.y = 4.0;
                                                            ui.add(
                                                                egui::Label::new(
                                                                    RichText::new(
                                                                        "Boosts speed by multiplying click inputs per cycle.\nMay not work on weaker machine.",
                                                                    )
                                                                    .size(11.5)
                                                                    .color(Color32::from_rgb(138, 142, 148)),
                                                                )
                                                                .wrap(),
                                                            );

                                                            let mut burst =
                                                                self.config.maximum_burst.clamp(1, 5);
                                                            ui.horizontal(|ui| {
                                                                ui.spacing_mut().item_spacing.x = 8.0;
                                                                for choice in
                                                                    [1_u8, 2_u8, 3_u8, 4_u8, 5_u8]
                                                                {
                                                                    if ui
                                                                        .selectable_label(
                                                                            burst == choice,
                                                                            format!("{choice}x"),
                                                                        )
                                                                        .clicked()
                                                                    {
                                                                        burst = choice;
                                                                    }
                                                                }
                                                            });
                                                            if burst != self.config.maximum_burst {
                                                                self.config.maximum_burst = burst;
                                                                self.worker.set_maximum_burst(burst);
                                                                self.persist_config();
                                                            }
                                                        },
                                                        );
                                                    });

                                                    let authors = if env!("CARGO_PKG_AUTHORS").is_empty() {
                                                        "Henry Nugraha"
                                                    } else {
                                                        env!("CARGO_PKG_AUTHORS")
                                                    };
                                                    settings_footer(
                                                        ui,
                                                        &format!("v{}", env!("CARGO_PKG_VERSION")),
                                                        authors,
                                                    );
                                                });
                                            });
                                            });
                                    },
                                );
                            }
                            },
                        );

                        if !self.status_line.is_empty() {
                            ui.colored_label(Color32::from_rgb(217, 150, 80), &self.status_line);
                        }
                    });
            });

        ctx.request_repaint_after(Duration::from_millis(16));
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        Color32::TRANSPARENT.to_normalized_gamma_f32()
    }
}
