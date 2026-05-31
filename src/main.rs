#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("This app only runs on Windows.");
}

#[cfg(target_os = "windows")]
mod app;

#[cfg(target_os = "windows")]
fn main() {
    if let Err(err) = app::run() {
        eprintln!("{err}");
    }
}
