use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use tauri::{Emitter, Manager};

use crate::app::{diagnostics, visibility};
use crate::audio::engine;
use crate::audio::state::AudioState;
use crate::audio::types::{
    AudioThreadCmd, HIDDEN_TICK_INTERVAL, STALL_COOLDOWN_MS, STALL_THRESHOLD_MS, TICK_INTERVAL_MS,
};
use crate::rt::AppHandle;

/// Emits `audio:tick` (position, seconds), tells the player when a track ends,
/// and reconnects the
/// output device when playback stalls.
pub fn start_tick_emitter(app: &AppHandle) {
    let handle = app.clone();
    std::thread::Builder::new()
        .name("audio-tick".into())
        .spawn(move || {
            let mut last_pos_ms = 0u64;
            let mut last_progress_at = Instant::now();
            let mut stall_cooldown_until = Instant::now();
            let mut last_tick_emit = Instant::now();

            loop {
                std::thread::sleep(Duration::from_millis(TICK_INTERVAL_MS));
                let state = handle.state::<AudioState>();

                if state.device_reconnected.swap(false, Ordering::Acquire) {
                    let _ = engine::reload_current_track(&state);
                    diagnostics::log_native(
                        &handle,
                        "INFO",
                        "[Audio] Device reconnected and reloaded",
                    );
                    handle.emit("audio:device-reconnected", ()).ok();
                }

                if !state.has_track.load(Ordering::Relaxed) {
                    last_pos_ms = 0;
                    last_progress_at = Instant::now();
                    continue;
                }

                let player_guard = state.player.lock().unwrap();
                let Some(ref player) = *player_guard else {
                    continue;
                };

                if player.empty() {
                    let suppress_ended =
                        engine::now_ms() < state.suppress_ended_until_ms.load(Ordering::Relaxed);
                    if !state.device_error.load(Ordering::Relaxed)
                        && !suppress_ended
                        && !state.ended_notified.swap(true, Ordering::Relaxed)
                    {
                        crate::player::send(&handle, crate::player::Command::TrackEnded);
                    }
                    continue;
                }

                let pos = player.get_pos().as_secs_f64();
                if !visibility::pages_hidden() || last_tick_emit.elapsed() >= HIDDEN_TICK_INTERVAL {
                    handle.emit("audio:tick", pos).ok();
                    last_tick_emit = Instant::now();
                }

                let pos_ms = (pos * 1000.0) as u64;
                let now = Instant::now();

                // Paused, progressing, or seeked backwards: nothing stalled.
                if player.is_paused()
                    || pos_ms > last_pos_ms
                    || pos_ms < last_pos_ms.saturating_sub(500)
                {
                    last_pos_ms = pos_ms;
                    last_progress_at = now;
                    continue;
                }

                // Don't mistake a settling device switch/reconnect for a stall.
                if engine::now_ms() < state.suppress_stall_until_ms.load(Ordering::Relaxed) {
                    last_progress_at = now;
                    continue;
                }

                if now < stall_cooldown_until {
                    continue;
                }

                if now.duration_since(last_progress_at).as_millis() as u64 > STALL_THRESHOLD_MS {
                    drop(player_guard);
                    diagnostics::log_native(
                        &handle,
                        "WARN",
                        "[Audio] Stall detected, reconnecting audio device",
                    );
                    // A stall usually means the output stream died silently (sleep/wake,
                    // headphone unplug); reloading the track on a dead mixer won't help.
                    state.audio_tx.send(AudioThreadCmd::Reconnect).ok();
                    stall_cooldown_until =
                        Instant::now() + Duration::from_millis(STALL_COOLDOWN_MS);
                    last_progress_at = Instant::now();
                }
            }
        })
        .expect("failed to spawn tick thread");
}
