mod app;
mod audio;
mod discord;
mod rt;
mod shared;

use std::sync::Arc;

use tauri::Manager;

use discord::DiscordState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    app::diagnostics::init_log_file();
    app::diagnostics::install_panic_hook();
    app::webview_data::wipe_before_launch();
    app::webview2::exit_if_runtime_missing();

    tauri::Builder::<rt::Rt>::new()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if !app::autostart::is_login_launch(&args) {
                app::visibility::show_main(app);
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(app::navigation::plugin())
        .setup(|app| {
            app::webview2::exit_if_main_window_missing(app);
            app::autostart::reveal_main_window(app);
            app::autostart::refresh_entry(app);
            app::diagnostics::mark_session_started(app.handle());

            app.manage(Arc::new(DiscordState::default()));

            app.manage(audio::init(app.handle()));
            app::visibility::start_watch(app.handle());
            audio::start_tick_emitter(app.handle());
            audio::start_media_controls(app.handle());
            audio::start_default_output_monitor(app.handle());

            app::tray::setup_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                if window.label() == "main" && app::close_action::quits_on_close() {
                    app::tray::run_action(window.app_handle(), "quit");
                } else {
                    api.prevent_close();
                    let _ = window.hide();
                    app::visibility::set_window_page_visible(window, false);
                }
            }
            tauri::WindowEvent::Resized(size) if window.label() == "main" => {
                app::visibility::follow_minimize(window, size);
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            app::diagnostics::diagnostics_log,
            app::diagnostics::diagnostics_log_dir,
            app::diagnostics::diagnostics_open_log_dir,
            app::diagnostics::diagnostics_reveal_log,
            app::close_action::close_action_get,
            app::close_action::close_action_set,
            app::autostart::autostart_get,
            app::autostart::autostart_set_enabled,
            app::autostart::autostart_set_minimized,
            app::visibility::show_main_window,
            discord::discord_connect,
            discord::discord_disconnect,
            discord::discord_set_activity,
            discord::discord_clear_activity,
            audio::audio_load_file,
            audio::audio_play,
            audio::audio_pause,
            audio::audio_stop,
            audio::audio_seek,
            audio::audio_set_volume,
            audio::audio_get_position,
            audio::audio_set_normalization,
            audio::audio_is_playing,
            audio::audio_set_metadata,
            audio::audio_set_playback_state,
            audio::audio_set_media_position,
            audio::audio_list_devices,
            audio::audio_switch_device,
            audio::audio_set_follow_default_output,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
