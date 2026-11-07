use std::backtrace::Backtrace;
use std::fs;
use std::path::PathBuf;

use crate::app::{APP_IDENTIFIER, log_sink};
use crate::rt::AppHandle;
use tauri_plugin_opener::OpenerExt;

fn log_dir() -> Result<PathBuf, String> {
    dirs::data_local_dir()
        .map(|data| data.join(APP_IDENTIFIER).join("logs"))
        .ok_or_else(|| "failed to resolve app log dir".to_string())
}

pub fn init_log_file() {
    if let Err(err) = log_dir().and_then(|dir| log_sink::init(&dir)) {
        eprintln!("[Diagnostics] {err}");
    }
}

pub fn log_native(_app: &AppHandle, level: &str, message: impl AsRef<str>) {
    let _ = log_sink::append(level, message.as_ref());
}

pub fn log(level: &str, message: impl AsRef<str>) {
    let message = message.as_ref();
    match level {
        "ERROR" | "WARN" | "PANIC" => eprintln!("{message}"),
        _ => println!("{message}"),
    }
    let _ = log_sink::append(level, message);
}

pub fn warn(message: impl AsRef<str>) {
    log("WARN", message);
}

pub fn error(message: impl AsRef<str>) {
    log("ERROR", message);
}

pub fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let thread = thread.name().unwrap_or("unnamed");
        let backtrace = Backtrace::force_capture();
        let message = format!("thread '{thread}' {info}");
        let _ = log_sink::append("PANIC", &format!("{message}\n{backtrace}"));
        default_hook(info);
    }));
}

pub fn mark_session_started(app: &AppHandle) {
    let _ = log_sink::append("INFO", "------------ SESSION STARTED -----------------");
    let _ = log_sink::append(
        "INFO",
        &format!(
            "Pulse {} on {} {}",
            app.package_info().version,
            std::env::consts::OS,
            std::env::consts::ARCH
        ),
    );
    if let Some(path) = log_sink::path() {
        let _ = log_sink::append("INFO", &format!("Log file: {}", path.display()));
    }
}

#[tauri::command]
pub fn diagnostics_log(level: String, message: String) -> Result<(), String> {
    log_sink::append(&level, &message)
}

#[tauri::command]
pub fn diagnostics_log_dir() -> Result<String, String> {
    log_dir().map(|dir| dir.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn diagnostics_open_log_dir(app: AppHandle) -> Result<(), String> {
    let dir = log_dir()?;
    fs::create_dir_all(&dir).map_err(|e| format!("failed to create app log dir: {e}"))?;
    let target = log_sink::path()
        .filter(|path| path.exists())
        .map(|path| path.to_path_buf());
    match target {
        Some(file) => app.opener().reveal_item_in_dir(file),
        None => app
            .opener()
            .open_path(dir.to_string_lossy().into_owned(), None::<&str>),
    }
    .map_err(|e| format!("failed to open log dir: {e}"))
}

#[tauri::command]
pub async fn diagnostics_reveal_log() -> Result<(), String> {
    let path = match log_sink::path() {
        Some(path) => path.to_path_buf(),
        None => log_sink::init(&log_dir()?)?,
    };
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("failed to open log file: {e}"))?;
    tauri_plugin_opener::reveal_item_in_dir(&path)
        .map_err(|e| format!("failed to reveal log file: {e}"))
}
