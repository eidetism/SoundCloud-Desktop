//! The screen the main window shows (route and back history), as the
//! frontend last reported it. The window is destroyed in the tray; when it
//! is created again it opens where the old one was. Opaque to Rust.

use std::sync::Mutex;

use serde_json::Value;
use tauri::State;

/// A route with its history is a few KB; anything far bigger is a bug.
const MAX_BYTES: usize = 512 * 1024;

#[derive(Default)]
pub struct ScreenState(Mutex<Option<Value>>);

#[tauri::command]
pub fn screen_get(state: State<'_, ScreenState>) -> Option<Value> {
    state.0.lock().unwrap().clone()
}

#[tauri::command]
pub fn screen_set(state: State<'_, ScreenState>, screen: Value) -> Result<(), String> {
    if screen.to_string().len() > MAX_BYTES {
        return Err("screen state too large".into());
    }
    *state.0.lock().unwrap() = Some(screen);
    Ok(())
}
