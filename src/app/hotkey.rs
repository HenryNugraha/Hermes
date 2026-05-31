use eframe::egui;
use global_hotkey::GlobalHotKeyManager;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, HotKeyState};

#[derive(Clone, Copy, Debug)]
pub struct HotkeyBinding {
    pub modifiers: Modifiers,
    pub code: Code,
}

impl Default for HotkeyBinding {
    fn default() -> Self {
        Self {
            modifiers: Modifiers::empty(),
            code: Code::F8,
        }
    }
}

impl HotkeyBinding {
    pub fn parse(value: &str) -> Option<Self> {
        let mut modifiers = Modifiers::empty();
        let mut code: Option<Code> = None;
        for token in value.split('+') {
            let token = token.trim();
            if token.is_empty() {
                continue;
            }
            let upper = token.to_ascii_uppercase();
            match upper.as_str() {
                "CTRL" | "CONTROL" => modifiers |= Modifiers::CONTROL,
                "ALT" => modifiers |= Modifiers::ALT,
                "SHIFT" => modifiers |= Modifiers::SHIFT,
                "WIN" | "SUPER" | "META" => modifiers |= Modifiers::SUPER,
                _ => code = parse_code_token(&upper),
            }
        }
        Some(Self {
            modifiers,
            code: code?,
        })
    }

    pub fn as_hotkey(self) -> HotKey {
        if self.modifiers.is_empty() {
            HotKey::new(None, self.code)
        } else {
            HotKey::new(Some(self.modifiers), self.code)
        }
    }

    pub fn display_text(self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if self.modifiers.contains(Modifiers::CONTROL) {
            parts.push("Ctrl");
        }
        if self.modifiers.contains(Modifiers::ALT) {
            parts.push("Alt");
        }
        if self.modifiers.contains(Modifiers::SHIFT) {
            parts.push("Shift");
        }
        if self.modifiers.contains(Modifiers::SUPER) {
            parts.push("Win");
        }
        parts.push(code_to_name(self.code).unwrap_or("Unknown"));
        parts.join("+")
    }
}

pub struct HotkeyController {
    manager: GlobalHotKeyManager,
    current_hotkey: HotKey,
}

impl HotkeyController {
    pub fn new(binding: HotkeyBinding) -> Result<Self, String> {
        let manager =
            GlobalHotKeyManager::new().map_err(|err| format!("Hotkey manager error: {err}"))?;
        let hotkey = binding.as_hotkey();
        manager.register(hotkey).map_err(|err| {
            format!(
                "Failed to register hotkey {}: {err}",
                binding.display_text()
            )
        })?;
        Ok(Self {
            manager,
            current_hotkey: hotkey,
        })
    }

    pub fn update_binding(&mut self, binding: HotkeyBinding) -> Result<(), String> {
        let next = binding.as_hotkey();
        self.manager
            .unregister(self.current_hotkey)
            .map_err(|err| format!("Failed to unregister previous hotkey: {err}"))?;
        if let Err(err) = self.manager.register(next) {
            let rollback = self.manager.register(self.current_hotkey);
            if let Err(rollback_err) = rollback {
                return Err(format!(
                    "Failed to bind new hotkey ({err}) and rollback failed ({rollback_err})"
                ));
            }
            return Err(format!(
                "Failed to register hotkey {}: {err}",
                binding.display_text()
            ));
        }
        self.current_hotkey = next;
        Ok(())
    }

    pub fn poll_toggle_event(&self) -> bool {
        let mut toggled = false;
        let receiver = GlobalHotKeyEvent::receiver();
        while let Ok(event) = receiver.try_recv() {
            if event.id == self.current_hotkey.id() && event.state == HotKeyState::Pressed {
                toggled = !toggled;
            }
        }
        toggled
    }
}

impl Drop for HotkeyController {
    fn drop(&mut self) {
        let _ = self.manager.unregister(self.current_hotkey);
    }
}

fn code_to_name(code: Code) -> Option<&'static str> {
    let name = match code {
        Code::F1 => "F1",
        Code::F2 => "F2",
        Code::F3 => "F3",
        Code::F4 => "F4",
        Code::F5 => "F5",
        Code::F6 => "F6",
        Code::F7 => "F7",
        Code::F8 => "F8",
        Code::F9 => "F9",
        Code::F10 => "F10",
        Code::F11 => "F11",
        Code::F12 => "F12",
        Code::F13 => "F13",
        Code::F14 => "F14",
        Code::F15 => "F15",
        Code::F16 => "F16",
        Code::F17 => "F17",
        Code::F18 => "F18",
        Code::F19 => "F19",
        Code::F20 => "F20",
        Code::F21 => "F21",
        Code::F22 => "F22",
        Code::F23 => "F23",
        Code::F24 => "F24",
        Code::KeyA => "A",
        Code::KeyB => "B",
        Code::KeyC => "C",
        Code::KeyD => "D",
        Code::KeyE => "E",
        Code::KeyF => "F",
        Code::KeyG => "G",
        Code::KeyH => "H",
        Code::KeyI => "I",
        Code::KeyJ => "J",
        Code::KeyK => "K",
        Code::KeyL => "L",
        Code::KeyM => "M",
        Code::KeyN => "N",
        Code::KeyO => "O",
        Code::KeyP => "P",
        Code::KeyQ => "Q",
        Code::KeyR => "R",
        Code::KeyS => "S",
        Code::KeyT => "T",
        Code::KeyU => "U",
        Code::KeyV => "V",
        Code::KeyW => "W",
        Code::KeyX => "X",
        Code::KeyY => "Y",
        Code::KeyZ => "Z",
        Code::Digit0 => "0",
        Code::Digit1 => "1",
        Code::Digit2 => "2",
        Code::Digit3 => "3",
        Code::Digit4 => "4",
        Code::Digit5 => "5",
        Code::Digit6 => "6",
        Code::Digit7 => "7",
        Code::Digit8 => "8",
        Code::Digit9 => "9",
        Code::Escape => "Esc",
        Code::Space => "Space",
        Code::Enter => "Enter",
        Code::Tab => "Tab",
        Code::Backspace => "Backspace",
        Code::ArrowUp => "Up",
        Code::ArrowDown => "Down",
        Code::ArrowLeft => "Left",
        Code::ArrowRight => "Right",
        Code::Insert => "Insert",
        Code::Delete => "Delete",
        Code::Home => "Home",
        Code::End => "End",
        Code::PageUp => "PageUp",
        Code::PageDown => "PageDown",
        _ => return None,
    };
    Some(name)
}

fn parse_code_token(token_upper: &str) -> Option<Code> {
    let code = match token_upper {
        "F1" => Code::F1,
        "F2" => Code::F2,
        "F3" => Code::F3,
        "F4" => Code::F4,
        "F5" => Code::F5,
        "F6" => Code::F6,
        "F7" => Code::F7,
        "F8" => Code::F8,
        "F9" => Code::F9,
        "F10" => Code::F10,
        "F11" => Code::F11,
        "F12" => Code::F12,
        "F13" => Code::F13,
        "F14" => Code::F14,
        "F15" => Code::F15,
        "F16" => Code::F16,
        "F17" => Code::F17,
        "F18" => Code::F18,
        "F19" => Code::F19,
        "F20" => Code::F20,
        "F21" => Code::F21,
        "F22" => Code::F22,
        "F23" => Code::F23,
        "F24" => Code::F24,
        "A" => Code::KeyA,
        "B" => Code::KeyB,
        "C" => Code::KeyC,
        "D" => Code::KeyD,
        "E" => Code::KeyE,
        "F" => Code::KeyF,
        "G" => Code::KeyG,
        "H" => Code::KeyH,
        "I" => Code::KeyI,
        "J" => Code::KeyJ,
        "K" => Code::KeyK,
        "L" => Code::KeyL,
        "M" => Code::KeyM,
        "N" => Code::KeyN,
        "O" => Code::KeyO,
        "P" => Code::KeyP,
        "Q" => Code::KeyQ,
        "R" => Code::KeyR,
        "S" => Code::KeyS,
        "T" => Code::KeyT,
        "U" => Code::KeyU,
        "V" => Code::KeyV,
        "W" => Code::KeyW,
        "X" => Code::KeyX,
        "Y" => Code::KeyY,
        "Z" => Code::KeyZ,
        "0" => Code::Digit0,
        "1" => Code::Digit1,
        "2" => Code::Digit2,
        "3" => Code::Digit3,
        "4" => Code::Digit4,
        "5" => Code::Digit5,
        "6" => Code::Digit6,
        "7" => Code::Digit7,
        "8" => Code::Digit8,
        "9" => Code::Digit9,
        "ESC" | "ESCAPE" => Code::Escape,
        "SPACE" => Code::Space,
        "ENTER" => Code::Enter,
        "TAB" => Code::Tab,
        "BACKSPACE" => Code::Backspace,
        "UP" => Code::ArrowUp,
        "DOWN" => Code::ArrowDown,
        "LEFT" => Code::ArrowLeft,
        "RIGHT" => Code::ArrowRight,
        "INSERT" => Code::Insert,
        "DELETE" => Code::Delete,
        "HOME" => Code::Home,
        "END" => Code::End,
        "PAGEUP" => Code::PageUp,
        "PAGEDOWN" => Code::PageDown,
        _ => return None,
    };
    Some(code)
}

pub fn egui_key_to_code(key: egui::Key) -> Option<Code> {
    let code = match key {
        egui::Key::A => Code::KeyA,
        egui::Key::B => Code::KeyB,
        egui::Key::C => Code::KeyC,
        egui::Key::D => Code::KeyD,
        egui::Key::E => Code::KeyE,
        egui::Key::F => Code::KeyF,
        egui::Key::G => Code::KeyG,
        egui::Key::H => Code::KeyH,
        egui::Key::I => Code::KeyI,
        egui::Key::J => Code::KeyJ,
        egui::Key::K => Code::KeyK,
        egui::Key::L => Code::KeyL,
        egui::Key::M => Code::KeyM,
        egui::Key::N => Code::KeyN,
        egui::Key::O => Code::KeyO,
        egui::Key::P => Code::KeyP,
        egui::Key::Q => Code::KeyQ,
        egui::Key::R => Code::KeyR,
        egui::Key::S => Code::KeyS,
        egui::Key::T => Code::KeyT,
        egui::Key::U => Code::KeyU,
        egui::Key::V => Code::KeyV,
        egui::Key::W => Code::KeyW,
        egui::Key::X => Code::KeyX,
        egui::Key::Y => Code::KeyY,
        egui::Key::Z => Code::KeyZ,
        egui::Key::Num0 => Code::Digit0,
        egui::Key::Num1 => Code::Digit1,
        egui::Key::Num2 => Code::Digit2,
        egui::Key::Num3 => Code::Digit3,
        egui::Key::Num4 => Code::Digit4,
        egui::Key::Num5 => Code::Digit5,
        egui::Key::Num6 => Code::Digit6,
        egui::Key::Num7 => Code::Digit7,
        egui::Key::Num8 => Code::Digit8,
        egui::Key::Num9 => Code::Digit9,
        egui::Key::F1 => Code::F1,
        egui::Key::F2 => Code::F2,
        egui::Key::F3 => Code::F3,
        egui::Key::F4 => Code::F4,
        egui::Key::F5 => Code::F5,
        egui::Key::F6 => Code::F6,
        egui::Key::F7 => Code::F7,
        egui::Key::F8 => Code::F8,
        egui::Key::F9 => Code::F9,
        egui::Key::F10 => Code::F10,
        egui::Key::F11 => Code::F11,
        egui::Key::F12 => Code::F12,
        egui::Key::F13 => Code::F13,
        egui::Key::F14 => Code::F14,
        egui::Key::F15 => Code::F15,
        egui::Key::F16 => Code::F16,
        egui::Key::F17 => Code::F17,
        egui::Key::F18 => Code::F18,
        egui::Key::F19 => Code::F19,
        egui::Key::F20 => Code::F20,
        egui::Key::Escape => Code::Escape,
        egui::Key::Space => Code::Space,
        egui::Key::Enter => Code::Enter,
        egui::Key::Tab => Code::Tab,
        egui::Key::Backspace => Code::Backspace,
        egui::Key::ArrowUp => Code::ArrowUp,
        egui::Key::ArrowDown => Code::ArrowDown,
        egui::Key::ArrowLeft => Code::ArrowLeft,
        egui::Key::ArrowRight => Code::ArrowRight,
        egui::Key::Insert => Code::Insert,
        egui::Key::Delete => Code::Delete,
        egui::Key::Home => Code::Home,
        egui::Key::End => Code::End,
        egui::Key::PageUp => Code::PageUp,
        egui::Key::PageDown => Code::PageDown,
        _ => return None,
    };
    Some(code)
}
