use std::os::windows::ffi::OsStrExt;

use windows::Win32::Graphics::Gdi::{
    DEVMODEW, ENUM_CURRENT_SETTINGS, EnumDisplaySettingsW, GetMonitorInfoW,
    MONITOR_DEFAULTTONEAREST, MONITORINFO, MONITORINFOEXW, MonitorFromPoint,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT, SendInput,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetCursorPos, IsChild, SW_RESTORE, SetCursorPos, SetForegroundWindow, ShowWindow,
    WindowFromPoint,
};
use windows::core::PCWSTR;

pub fn focus_existing_window(title: &str) {
    let title = to_wide(title);
    // SAFETY: We pass null class name and a valid title pointer.
    unsafe {
        if let Ok(hwnd) = FindWindowW(PCWSTR::null(), PCWSTR(title.as_ptr())) {
            if !hwnd.0.is_null() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
                let _ = SetForegroundWindow(hwnd);
            }
        }
    }
}

pub fn is_cursor_over_window(title: &str) -> bool {
    let title = to_wide(title);
    // SAFETY: We pass null class name and a valid title pointer.
    let hwnd = unsafe { FindWindowW(PCWSTR::null(), PCWSTR(title.as_ptr())) };
    let Ok(hwnd) = hwnd else {
        return false;
    };
    if hwnd.0.is_null() {
        return false;
    }

    let mut point = windows::Win32::Foundation::POINT::default();
    // SAFETY: `point` is writable.
    if unsafe { GetCursorPos(&mut point) }.is_err() {
        return false;
    }

    // SAFETY: point is initialized.
    let hover_hwnd = unsafe { WindowFromPoint(point) };
    if hover_hwnd.0.is_null() {
        return false;
    }

    if hover_hwnd == hwnd {
        return true;
    }

    // SAFETY: both HWNDs are valid handles for the query.
    unsafe { IsChild(hwnd, hover_hwnd).as_bool() }
}

pub fn send_left_click() {
    let _ = send_left_click_burst(1);
}

pub fn send_left_click_burst(clicks: u8) -> u32 {
    let clicks = clicks.max(1);
    let mut inputs = Vec::with_capacity(usize::from(clicks) * 2);

    let down = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: 0,
                dwFlags: MOUSEEVENTF_LEFTDOWN,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let up = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: 0,
                dwFlags: MOUSEEVENTF_LEFTUP,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    for _ in 0..clicks {
        inputs.push(down);
        inputs.push(up);
    }

    // SAFETY: We pass a valid slice with correct INPUT struct size.
    let sent_inputs = unsafe {
        SendInput(
            &inputs,
            i32::try_from(std::mem::size_of::<INPUT>()).unwrap_or(0),
        )
    };
    sent_inputs / 2
}

pub fn current_monitor_refresh_hz() -> Option<f64> {
    let mut point = windows::Win32::Foundation::POINT::default();
    // SAFETY: `point` is writable.
    if unsafe { GetCursorPos(&mut point) }.is_err() {
        return None;
    }

    // SAFETY: `point` is initialized.
    let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
    if monitor.0.is_null() {
        return None;
    }

    let mut monitor_info = MONITORINFOEXW::default();
    monitor_info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    // SAFETY: monitor handle is valid and monitor_info points to writable memory.
    let info_ok = unsafe {
        GetMonitorInfoW(
            monitor,
            &mut monitor_info as *mut MONITORINFOEXW as *mut MONITORINFO,
        )
    };
    if !info_ok.as_bool() {
        return None;
    }

    let mut dev_mode = DEVMODEW::default();
    dev_mode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
    // SAFETY: device name pointer is valid and dev_mode points to writable memory.
    let mode_ok = unsafe {
        EnumDisplaySettingsW(
            PCWSTR(monitor_info.szDevice.as_ptr()),
            ENUM_CURRENT_SETTINGS,
            &mut dev_mode,
        )
    };
    if !mode_ok.as_bool() {
        return None;
    }

    let hz = dev_mode.dmDisplayFrequency;
    if hz <= 1 { None } else { Some(hz as f64) }
}

pub fn cursor_position() -> Option<(i32, i32)> {
    let mut point = windows::Win32::Foundation::POINT::default();
    // SAFETY: `point` is writable.
    if unsafe { GetCursorPos(&mut point) }.is_err() {
        return None;
    }
    Some((point.x, point.y))
}

pub fn set_cursor_position(x: i32, y: i32) -> bool {
    // SAFETY: coordinates are plain screen-space integers for the system call.
    unsafe { SetCursorPos(x, y) }.is_ok()
}

fn to_wide(value: &str) -> Vec<u16> {
    std::ffi::OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}
