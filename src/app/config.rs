use std::fs;
use std::path::Path;

use crate::app::hotkey::HotkeyBinding;
use crate::app::worker::ClickRate;

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub click_rate: ClickRate,
    pub hotkey: HotkeyBinding,
    pub pinned: bool,
    pub background_opacity: u8,
    pub humanize_random_delay: bool,
    pub humanize_cursor_jitter: bool,
    pub maximum_burst: u8,
    pub window_x: Option<i32>,
    pub window_y: Option<i32>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            click_rate: ClickRate::Normal,
            hotkey: HotkeyBinding::default(),
            pinned: false,
            background_opacity: 95,
            humanize_random_delay: false,
            humanize_cursor_jitter: false,
            maximum_burst: 1,
            window_x: None,
            window_y: None,
        }
    }
}

impl AppConfig {
    pub fn load(path: &Path) -> Self {
        let mut cfg = Self::default();
        if let Ok(contents) = fs::read_to_string(path) {
            for raw in contents.lines() {
                let line = raw.trim();
                if line.is_empty()
                    || line.starts_with(';')
                    || line.starts_with('#')
                    || line.starts_with('[')
                {
                    continue;
                }
                let Some((key, value)) = line.split_once('=') else {
                    continue;
                };
                let key = key.trim().to_ascii_lowercase();
                let value = value.trim();
                match key.as_str() {
                    "click_rate" => {
                        if let Some(rate) = ClickRate::from_config_value(value) {
                            cfg.click_rate = rate;
                        }
                    }
                    "hotkey" => {
                        if let Some(hotkey) = HotkeyBinding::parse(value) {
                            cfg.hotkey = hotkey;
                        }
                    }
                    "pinned" => {
                        cfg.pinned = matches!(
                            value.to_ascii_lowercase().as_str(),
                            "1" | "true" | "yes" | "on"
                        );
                    }
                    "background_opacity" | "bg_opacity_percent" => {
                        if let Ok(percent) = value.parse::<u8>() {
                            cfg.background_opacity = percent.clamp(15, 100);
                        }
                    }
                    "humanize_random_delay" => {
                        cfg.humanize_random_delay = matches!(
                            value.to_ascii_lowercase().as_str(),
                            "1" | "true" | "yes" | "on"
                        );
                    }
                    "humanize_cursor_jitter" => {
                        cfg.humanize_cursor_jitter = matches!(
                            value.to_ascii_lowercase().as_str(),
                            "1" | "true" | "yes" | "on"
                        );
                    }
                    "maximum_burst" => {
                        if let Ok(burst) = value.parse::<u8>() {
                            cfg.maximum_burst = burst.clamp(1, 5);
                        }
                    }
                    "window_x" => {
                        if let Ok(x) = value.parse::<i32>() {
                            cfg.window_x = Some(x);
                        }
                    }
                    "window_y" => {
                        if let Ok(y) = value.parse::<i32>() {
                            cfg.window_y = Some(y);
                        }
                    }
                    _ => {}
                }
            }
        }
        cfg
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let mut body = String::new();
        body.push_str("[hermes]\n");
        body.push_str(&format!(
            "click_rate={}\n",
            self.click_rate.as_config_value()
        ));
        body.push_str(&format!("hotkey={}\n", self.hotkey.display_text()));
        body.push_str(&format!(
            "pinned={}\n",
            if self.pinned { "true" } else { "false" }
        ));
        body.push_str(&format!("background_opacity={}\n", self.background_opacity));
        body.push_str(&format!(
            "humanize_random_delay={}\n",
            if self.humanize_random_delay {
                "true"
            } else {
                "false"
            }
        ));
        body.push_str(&format!(
            "humanize_cursor_jitter={}\n",
            if self.humanize_cursor_jitter {
                "true"
            } else {
                "false"
            }
        ));
        body.push_str(&format!("maximum_burst={}\n", self.maximum_burst));
        if let Some(x) = self.window_x {
            body.push_str(&format!("window_x={x}\n"));
        }
        if let Some(y) = self.window_y {
            body.push_str(&format!("window_y={y}\n"));
        }
        fs::write(path, body)
            .map_err(|err| format!("Failed to write config file {}: {err}", path.display()))
    }
}
