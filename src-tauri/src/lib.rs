mod api;
mod app;
mod audio;
mod auth;
mod discord;
mod images;
mod net;
mod offline;
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
            app.manage(api::inbox::Inbox::default());
            api::inbox::start(app.handle());
            app.manage(DiscordState::start());

            app.manage(images::ImageCache::open(&app.path().app_cache_dir()?));
            app.manage(offline::Offline::open(
                app.path().app_local_data_dir()?,
                settings_snapshot.offline_dir.clone(),
                settings_snapshot.recent_cache_mb,
            ));
            app.manage(player::waveform::WaveformCache::default());
            let audio = audio::init(app.handle());
            audio::engine::set_normalization(&audio, settings_snapshot.normalize_volume);
            app.manage(audio);
            app.manage(PlayerState::new());
            app.manage(player::drm::DrmState::default());
            player::start(app.handle());
            app::visibility::start_watch(app.handle());
            audio::start_tick_emitter(app.handle());
            app.manage(app::screen::ScreenState::default());
            let media_owner = app::media_window::create().map(|hwnd| hwnd as usize);
            audio::start_media_controls(app.handle(), media_owner);
            audio::start_default_output_monitor(app.handle());

            app::tray::setup_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                if window.label() == app::visibility::MAIN {
                    // Quit, or let it close: the main window and its WebView
                    // go away in the tray and are created again from there.
                    if app::close_action::quits_on_close() {
                        app::tray::run_action(window.app_handle(), "quit");
                    }
                } else {
                    api.prevent_close();
                    let _ = window.hide();
                    app::visibility::set_window_page_visible(window, false);
                }
            }
            tauri::WindowEvent::Resized(size) if window.label() == app::visibility::MAIN => {
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
            app::screen::screen_get,
            app::screen::screen_set,
            net::net_settings_get,
            net::net_settings_set,
            net::net_check,
            auth::auth_status,
            auth::auth_login_start,
            auth::auth_logout,
            app::links::app_open_link,
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
            api::commands::api_library,
            api::commands::api_user_top_tracks,
            api::commands::api_user_playlists,
            api::commands::api_comment_create,
            api::commands::api_comment_delete,
            api::commands::api_comment_like,
            api::commands::api_set_repost,
            api::commands::api_reposted_ids,
            api::commands::api_following_ids,
            api::commands::api_related,
            api::commands::api_track_sets,
            api::commands::api_likers,
            api::commands::api_own_playlists,
            api::commands::api_playlist_set_track,
            api::inbox::messages_unread,
            api::inbox::api_conversations,
            api::inbox::api_conversation,
            api::inbox::api_messages,
            api::inbox::api_conversation_read,
            api::inbox::api_message_send,
            api::inbox::api_resolve_track,
            offline::commands::offline_ids,
            offline::commands::offline_summary,
            offline::commands::offline_saved,
            offline::commands::offline_save,
            offline::commands::offline_remove,
            offline::commands::offline_clear_recent,
            offline::commands::offline_set_recent_limit,
            offline::commands::offline_pick_folder,
            offline::commands::offline_relocate,
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
            player::drm::drm_boot,
            player::drm::drm_segment,
            player::drm::drm_license,
            player::drm::drm_report,
            player::drm::drm_diag,
            player::waveform::player_waveform,
            settings::settings_ui_get,
            settings::settings_ui_set,
            audio::audio_list_devices,
            audio::audio_switch_device,
            audio::audio_set_follow_default_output,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| {
            // The last window closing (to the tray) doesn't end the app;
            // an explicit exit (tray "Quit") carries a code and does.
            if let tauri::RunEvent::ExitRequested {
                code: None, api, ..
            } = event
            {
                api.prevent_exit();
            }
        });
}
