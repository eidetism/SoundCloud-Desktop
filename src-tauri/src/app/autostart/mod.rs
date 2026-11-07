mod windows;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::app::diagnostics::log_native;
use crate::app::APP_IDENTIFIER;
use crate::rt::App;

use windows as platform;

pub const LOGIN_ARG: &str = "--autostart";
const FLAG_FILE: &str = "autostart.json";

#[derive(Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Flag {
    start_minimized: bool,
}

impl Default for Flag {
    fn default() -> Self {
        Self {
            start_minimized: true,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutostartState {
    enabled: bool,
    start_minimized: bool,
}

fn flag_path() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join(APP_IDENTIFIER).join(FLAG_FILE))
}

fn load_flag() -> Flag {
    flag_path()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn update_flag(change: impl FnOnce(&mut Flag)) -> Result<(), String> {
    let mut flag = load_flag();
    change(&mut flag);
    let path = flag_path().ok_or("app data dir is unavailable")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let bytes = serde_json::to_vec(&flag).map_err(|e| e.to_string())?;
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

fn current() -> AutostartState {
    AutostartState {
        enabled: platform::is_enabled(),
        start_minimized: load_flag().start_minimized,
    }
}

pub fn is_login_launch<S: AsRef<str>>(args: &[S]) -> bool {
    args.iter().any(|arg| arg.as_ref() == LOGIN_ARG)
}

fn starts_hidden() -> bool {
    std::env::args_os().any(|arg| arg == LOGIN_ARG) && load_flag().start_minimized
}

pub fn reveal_main_window(app: &App) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if starts_hidden() {
        // In the tray the window doesn't exist; the tray recreates it.
        let _ = window.destroy();
    } else {
        let _ = window.show();
    }
}

pub fn refresh_entry(app: &App) {
    if cfg!(debug_assertions) {
        return;
    }
    if let Err(err) = platform::refresh() {
        log_native(
            app.handle(),
            "WARN",
            format!("[autostart] failed to refresh the login entry: {err}"),
        );
    }
}

#[tauri::command]
pub fn autostart_get() -> AutostartState {
    current()
}

#[tauri::command]
pub async fn autostart_set_enabled(
    enabled: bool,
    reason: String,
) -> Result<AutostartState, String> {
    platform::set_enabled(enabled, &reason).await?;
    Ok(current())
}

#[tauri::command]
pub fn autostart_set_minimized(start_minimized: bool) -> Result<AutostartState, String> {
    update_flag(|flag| flag.start_minimized = start_minimized)?;
    Ok(current())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_flag_starts_minimized() {
        let flag: Flag = serde_json::from_slice(b"{}").unwrap();
        assert!(flag.start_minimized);
    }

    #[test]
    fn flag_round_trips_in_camel_case() {
        let bytes = serde_json::to_vec(&Flag {
            start_minimized: false,
        })
        .unwrap();
        assert_eq!(bytes, br#"{"startMinimized":false}"#);
    }

    #[test]
    fn login_launch_needs_the_autostart_arg() {
        assert!(is_login_launch(&["scdesktop", LOGIN_ARG]));
        assert!(!is_login_launch(&["scdesktop"]));
    }
}
