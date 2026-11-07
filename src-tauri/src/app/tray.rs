//! System tray: native Tauri `tray-icon` (Win32 Shell_NotifyIcon).
//!
//! Tray callbacks may arrive off the main thread, while Tauri window operations
//! must run on it, so every action goes through `run_on_main_thread`.

use tauri::image::Image;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

use crate::app::{locale, visibility};
use crate::player::{self, Command};
use crate::rt::{App, AppHandle};

const TOOLTIP: &str = "Pulse";
const TRAY_ID: &str = "main";

pub fn setup_tray(app: &App) -> tauri::Result<()> {
    let item = |id: &str, label: &str| MenuItemBuilder::with_id(id, label).build(app);
    let menu = MenuBuilder::new(app)
        .items(&[
            &item("show", locale::pick("Показать", "Show"))?,
            &item(
                "play_pause",
                locale::pick("Пауза / Воспроизвести", "Play / Pause"),
            )?,
            &item("prev", locale::pick("Предыдущий", "Previous"))?,
            &item("next", locale::pick("Следующий", "Next"))?,
            &item("quit", locale::pick("Выход", "Quit"))?,
        ])
        .build()?;

    let icon = tray_icon(app);

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip(TOOLTIP)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| run_action(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                run_action(tray.app_handle(), "show");
            }
        })
        .build(app)?;
    Ok(())
}

/// Pixel-aligned Pulse tray icon. The 16px one keeps only the 5 central bars:
/// all 9 merge into a blob at that size. Windows asks for 16px at 100-125%
/// scaling and 24-32px from 150%, so the bigger one is picked from there.
fn tray_icon(app: &App) -> Image<'static> {
    let scale = app
        .primary_monitor()
        .ok()
        .flatten()
        .map_or(1.0, |m| m.scale_factor());
    if scale >= 1.5 {
        tauri::include_image!("icons/tray-32.png")
    } else {
        tauri::include_image!("icons/tray-16.png")
    }
}

/// Run a tray/menu action on the main thread.
pub fn run_action(app: &AppHandle, id: &str) {
    let handle = app.clone();
    let id = id.to_string();
    let _ = app.run_on_main_thread(move || match id.as_str() {
        "show" => visibility::show_main(&handle),
        "quit" => handle.exit(0),
        "play_pause" => player::send(&handle, Command::Toggle),
        "next" => player::send(&handle, Command::Next),
        "prev" => player::send(&handle, Command::Previous),
        _ => {}
    });
}

/// Show the current track in the tray tooltip (`None`: just the app name).
pub fn set_now_playing(app: &AppHandle, track: Option<&str>) {
    let tooltip = match track {
        Some(track) => format!(
            "{track}
{TOOLTIP}"
        ),
        None => TOOLTIP.to_string(),
    };
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(tray) = handle.tray_by_id(TRAY_ID) {
            let _ = tray.set_tooltip(Some(tooltip));
        }
    });
}
