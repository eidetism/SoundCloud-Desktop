use std::time::Duration;

use crate::app::diagnostics;
use crate::rt::AppHandle;
use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata as SmtcMetadata, MediaPlayback, MediaPosition,
    PlatformConfig,
};
use tauri::Manager;

use crate::player::{self, Command};

use crate::audio::engine;
use crate::audio::state::AudioState;
use crate::audio::types::MediaCmd;

/// Start SMTC on `owner` (the invisible media window, as a raw HWND).
pub fn start_media_controls(app: &AppHandle, owner: Option<usize>) {
    let handle = app.clone();
    let (tx, rx) = std::sync::mpsc::channel::<MediaCmd>();

    let state = app.state::<AudioState>();
    *state.media_tx.lock().unwrap() = Some(tx);

    std::thread::Builder::new()
        .name("media-controls".into())
        .spawn(move || {
            let hwnd = owner.map(|hwnd| hwnd as *mut std::ffi::c_void);

            let config = PlatformConfig {
                display_name: "Pulse",
                dbus_name: "scdesktop",
                hwnd,
            };

            let mut controls = match MediaControls::new(config) {
                Ok(controls) => controls,
                Err(error) => {
                    diagnostics::warn(format!("[MediaControls] Failed to create: {:?}", error));
                    return;
                }
            };

            let event_handle = handle.clone();
            controls
                .attach(move |event: MediaControlEvent| {
                    let command = match event {
                        MediaControlEvent::Play => Command::Play,
                        MediaControlEvent::Pause => Command::Pause,
                        MediaControlEvent::Toggle => Command::Toggle,
                        MediaControlEvent::Next => Command::Next,
                        MediaControlEvent::Previous => Command::Previous,
                        MediaControlEvent::SetPosition(MediaPosition(pos)) => {
                            Command::Seek(pos.as_secs_f64())
                        }
                        MediaControlEvent::Seek(souvlaki::SeekDirection::Forward) => {
                            Command::SeekBy(10.0)
                        }
                        MediaControlEvent::Seek(souvlaki::SeekDirection::Backward) => {
                            Command::SeekBy(-10.0)
                        }
                        MediaControlEvent::Raise => {
                            crate::app::tray::run_action(&event_handle, "show");
                            return;
                        }
                        MediaControlEvent::Quit => {
                            crate::app::tray::run_action(&event_handle, "quit");
                            return;
                        }
                        _ => return,
                    };
                    player::send(&event_handle, command);
                })
                .ok();

            loop {
                match rx.recv() {
                    Ok(MediaCmd::SetMetadata {
                        title,
                        artist,
                        cover_url,
                        duration_secs,
                    }) => {
                        controls
                            .set_metadata(SmtcMetadata {
                                title: Some(&title),
                                artist: Some(&artist),
                                cover_url: cover_url.as_deref(),
                                duration: if duration_secs > 0.0 {
                                    Some(Duration::from_secs_f64(duration_secs))
                                } else {
                                    None
                                },
                                ..Default::default()
                            })
                            .ok();
                    }
                    Ok(MediaCmd::SetPlaying(playing)) => {
                        let state = handle.state::<AudioState>();
                        let pos = Duration::from_secs_f64(engine::position(&state));
                        let progress = Some(MediaPosition(pos));
                        let playback = if playing {
                            MediaPlayback::Playing { progress }
                        } else {
                            MediaPlayback::Paused { progress }
                        };
                        controls.set_playback(playback).ok();
                    }
                    Ok(MediaCmd::SetPosition(secs)) => {
                        let state = handle.state::<AudioState>();
                        let is_playing = engine::is_playing(&state);
                        let progress = Some(MediaPosition(Duration::from_secs_f64(secs)));
                        let playback = if is_playing {
                            MediaPlayback::Playing { progress }
                        } else {
                            MediaPlayback::Paused { progress }
                        };
                        controls.set_playback(playback).ok();
                    }
                    Err(_) => break,
                }
            }
        })
        .expect("failed to spawn media-controls thread");
}
