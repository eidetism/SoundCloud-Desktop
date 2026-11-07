use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{Manager, PhysicalSize};

use crate::rt::{AppHandle, Rt, WebviewWindow};

static PAGES_HIDDEN: AtomicBool = AtomicBool::new(false);
static MAIN_MINIMIZED: AtomicBool = AtomicBool::new(false);
const WATCH_INTERVAL: Duration = Duration::from_secs(1);
pub const MAIN: &str = "main";

/// The main window as `tauri.conf.json` describes it (hidden until shown).
fn create_main(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == MAIN)
        .cloned()
        .ok_or_else(|| tauri::Error::WindowNotFound)?;
    tauri::WebviewWindowBuilder::from_config(app, &config)?.build()
}

pub fn pages_hidden() -> bool {
    PAGES_HIDDEN.load(Ordering::Relaxed)
}

pub fn start_watch(app: &AppHandle) {
    let app = app.clone();
    std::thread::Builder::new()
        .name("page-visibility".into())
        .spawn(move || {
            loop {
                std::thread::sleep(WATCH_INTERVAL);
                let shown = app.webview_windows().values().any(|window| {
                    window.is_visible().unwrap_or(true) && !window.is_minimized().unwrap_or(false)
                });
                PAGES_HIDDEN.store(!shown, Ordering::Relaxed);
            }
        })
        .expect("failed to spawn page-visibility thread");
}

pub fn set_page_visible(window: &WebviewWindow, visible: bool) {
    if visible {
        PAGES_HIDDEN.store(false, Ordering::Relaxed);
    }
    if !cfg!(windows) {
        return;
    }
    let webview: &tauri::Webview<Rt> = window.as_ref();
    let _ = if visible {
        webview.show()
    } else {
        webview.hide()
    };
}

pub fn set_window_page_visible(window: &tauri::Window<Rt>, visible: bool) {
    if let Some(page) = window.get_webview_window(window.label()) {
        set_page_visible(&page, visible);
    }
}

pub fn follow_minimize(window: &tauri::Window<Rt>, size: &PhysicalSize<u32>) {
    let minimized = size.width == 0 && size.height == 0;
    if MAIN_MINIMIZED.swap(minimized, Ordering::Relaxed) != minimized {
        set_window_page_visible(window, !minimized);
    }
}

/// Show the main window, creating it again if it was closed to the tray.
/// Call on the main thread.
pub fn show_main(app: &AppHandle) {
    let window = match app.get_webview_window(MAIN) {
        Some(window) => window,
        None => match create_main(app) {
            Ok(window) => window,
            Err(error) => {
                crate::app::diagnostics::warn(format!("[window] recreate: {error}"));
                return;
            }
        },
    };
    set_page_visible(&window, true);
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

#[tauri::command]
pub async fn show_main_window(app: AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || show_main(&handle));
}
