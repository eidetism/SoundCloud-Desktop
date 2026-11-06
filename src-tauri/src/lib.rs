mod api;
mod app;
mod audio;
mod auth;
mod discord;
mod images;
mod net;
mod player;
mod rt;
mod secrets;
mod settings;
mod shared;

use tauri::Manager;

use api::Api;
use auth::AuthState;
use discord::DiscordState;
use net::NetState;
use player::PlayerState;
use settings::SettingsStore;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    app::diagnostics::init_log_file();
    app::diagnostics::install_panic_hook();
    let _ = rustls::crypto::ring::default_provider().install_default();
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
        .register_asynchronous_uri_scheme_protocol(images::SCHEME, |ctx, request, responder| {
            images::handle(ctx.app_handle(), request, responder)
        })
        .setup(|app| {
            app::webview2::exit_if_main_window_missing(app);
            app::autostart::reveal_main_window(app);
            app::autostart::refresh_entry(app);
            app::diagnostics::mark_session_started(app.handle());

            let data_dir = app.path().app_data_dir()?;
            let settings = SettingsStore::open(&data_dir);
            let settings_snapshot = settings.get();
            app.manage(NetState::init(&settings.get()));
            app.manage(settings);
            app.manage(Api::default());
            app.manage(AuthState::init(&data_dir));
            auth::verify(app.handle());
            app.manage(DiscordState::start());

            app.manage(images::ImageCache::open(&app.path().app_cache_dir()?));
            app.manage(player::waveform::WaveformCache::default());
            let audio = audio::init(app.handle());
            audio::engine::set_normalization(&audio, settings_snapshot.normalize_volume);
            app.manage(audio);
            app.manage(PlayerState::new());
            player::start(app.handle());
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
            net::net_settings_get,
            net::net_settings_set,
            net::net_check,
            auth::auth_status,
            auth::auth_login_start,
            auth::auth_logout,
            api::commands::api_feed,
            api::commands::api_likes,
            api::commands::api_history,
            api::commands::api_discover,
            api::commands::api_search,
            api::commands::api_playlist,
            api::commands::api_tracks,
            api::commands::api_comments,
            api::commands::api_artist_shortcuts,
            api::commands::api_liked_ids,
            api::commands::api_set_like,
            api::commands::api_user,
            api::commands::api_user_tracks,
            player::commands::player_state,
            player::commands::player_play,
            player::commands::player_toggle,
            player::commands::player_next,
            player::commands::player_previous,
            player::commands::player_seek,
            player::commands::player_set_volume,
            player::commands::player_set_shuffle,
            player::commands::player_set_repeat,
            player::commands::player_play_next,
            player::commands::player_append,
            player::commands::player_skip_ahead,
            player::waveform::player_waveform,
            settings::settings_ui_get,
            settings::settings_ui_set,
            audio::audio_list_devices,
            audio::audio_switch_device,
            audio::audio_set_follow_default_output,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
