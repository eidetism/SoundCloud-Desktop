use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::State;
use tokio::task;

use crate::audio::decode::{create_player_from_bytes, resolve_normalization_gain};
use crate::audio::state::AudioState;
use crate::audio::types::{AudioLoadResult, AudioThreadCmd, MediaCmd, STALL_SUPPRESS_MS};

const ENDED_SUPPRESS_MS: u64 = 1200;

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn suppress_ended_temporarily(state: &AudioState) {
    state
        .suppress_ended_until_ms
        .store(now_ms() + ENDED_SUPPRESS_MS, Ordering::Relaxed);
}

/// Mute stall-detection (tick.rs) for a short window. A device switch briefly
/// freezes the old player and the freshly opened output may lag before it starts
/// pulling samples; without this the tick thread reads the gap as a dead stream and
/// fires a redundant reconnect mid-switch.
pub fn suppress_stall_temporarily(state: &AudioState) {
    state
        .suppress_stall_until_ms
        .store(now_ms() + STALL_SUPPRESS_MS, Ordering::Relaxed);
}

fn volume_to_rodio(v: f64) -> f32 {
    (v / 100.0).clamp(0.0, 2.0) as f32
}

fn stop_current_player(state: &AudioState) {
    suppress_ended_temporarily(state);
    let mut player = state.player.lock().unwrap();
    if let Some(old) = player.take() {
        old.stop();
    }
}

fn applied_gain(state: &AudioState) -> f32 {
    if state.normalization_enabled.load(Ordering::Relaxed) {
        *state.normalization_gain.lock().unwrap()
    } else {
        1.0
    }
}

fn commit_loaded_track(
    state: &AudioState,
    bytes: Vec<u8>,
    new_player: rodio::Player,
    normalization_gain: f32,
) {
    let mut player = state.player.lock().unwrap();
    if let Some(previous) = player.replace(new_player) {
        previous.stop();
    }
    *state.source_bytes.lock().unwrap() = Some(bytes);
    *state.normalization_gain.lock().unwrap() = normalization_gain;
    state.has_track.store(true, Ordering::Relaxed);
    state.ended_notified.store(false, Ordering::Relaxed);
    state.device_error.store(false, Ordering::Relaxed);
}

/// Recreate the player from the retained source bytes, seeked to `position` seconds.
fn rebuild_player(state: &AudioState, position: f64, paused: bool) -> Result<(), String> {
    let bytes = state.source_bytes.lock().unwrap().clone();
    let Some(bytes) = bytes else {
        return Err("No source to reload".into());
    };
    let mixer = state.mixer.lock().unwrap().clone();
    let volume = *state.volume.lock().unwrap();
    let (new_player, _) =
        create_player_from_bytes(&bytes, &mixer, volume, applied_gain(state), true)?;
    if position > 0.0 {
        new_player
            .try_seek(Duration::from_secs_f64(position))
            .map_err(|e| format!("Seek to {position:.1}s failed: {e}"))?;
    }

    let mut player = state.player.lock().unwrap();
    if let Some(old) = player.take() {
        old.stop();
    }
    if !paused {
        new_player.play();
    }
    *player = Some(new_player);
    state.has_track.store(true, Ordering::Relaxed);
    state.ended_notified.store(false, Ordering::Relaxed);
    state.device_error.store(false, Ordering::Relaxed);
    Ok(())
}

pub fn reload_current_track(state: &AudioState) -> Result<(), String> {
    suppress_ended_temporarily(state);
    suppress_stall_temporarily(state);
    let (position, paused) = {
        let player = state.player.lock().unwrap();
        let Some(player) = player.as_ref() else {
            return Ok(());
        };
        (player.get_pos().as_secs_f64(), player.is_paused())
    };
    rebuild_player(state, position, paused)
}

pub async fn load_file(
    path: String,
    normalization_cache_dir: Option<PathBuf>,
    normalization_cache_key: Option<String>,
    start_paused: bool,
    state: State<'_, AudioState>,
) -> Result<AudioLoadResult, String> {
    let bytes = task::spawn_blocking({
        let path = path.clone();
        move || std::fs::read(&path).map_err(|e| format!("Failed to read {path}: {e}"))
    })
    .await
    .map_err(|e| format!("audio file read task failed: {e}"))??;

    stop_current_player(&state);

    let mixer = state.mixer.lock().unwrap().clone();
    let volume = *state.volume.lock().unwrap();
    let normalization_enabled = state.normalization_enabled.load(Ordering::Relaxed);
    let (bytes, new_player, duration_secs, normalization_gain) = task::spawn_blocking(move || {
        let gain = if normalization_enabled {
            resolve_normalization_gain(
                &bytes,
                normalization_cache_dir.as_deref(),
                normalization_cache_key.as_deref(),
            )?
        } else {
            1.0
        };
        let (player, duration) =
            create_player_from_bytes(&bytes, &mixer, volume, gain, start_paused)?;
        Ok::<_, String>((bytes, player, duration, gain))
    })
    .await
    .map_err(|e| format!("audio decode task failed: {e}"))??;

    commit_loaded_track(&state, bytes, new_player, normalization_gain);
    Ok(AudioLoadResult { duration_secs })
}

pub fn play(state: State<'_, AudioState>) {
    // If the device errored (sleep/wake, headphone unplug), reconnect immediately
    // instead of waiting for stall detection.
    if state.device_error.load(Ordering::Relaxed) {
        state.audio_tx.send(AudioThreadCmd::Reconnect).ok();
    }
    if let Ok(player) = state.player.try_lock()
        && let Some(ref player) = *player
    {
        player.play();
    }
}

pub fn pause(state: State<'_, AudioState>) {
    if let Ok(player) = state.player.try_lock()
        && let Some(ref player) = *player
    {
        player.pause();
    }
}

pub fn stop(state: State<'_, AudioState>) {
    state.has_track.store(false, Ordering::Relaxed);
    state.load_gen.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut player) = state.player.try_lock()
        && let Some(old) = player.take()
    {
        old.stop();
    }
    if let Ok(mut bytes) = state.source_bytes.try_lock() {
        *bytes = None;
    }
}

/// Seek to `position` seconds, trying an in-place decoder seek first and recreating
/// the player when that fails: a bare `try_seek` silently no-ops on decoders that
/// can't seek in place.
pub fn seek(position: f64, state: State<'_, AudioState>) -> Result<(), String> {
    suppress_ended_temporarily(&state);
    let target = position.max(0.0);
    let paused = {
        let player = state.player.lock().unwrap();
        if let Some(ref player) = *player
            && !player.empty()
            && player.try_seek(Duration::from_secs_f64(target)).is_ok()
        {
            state.ended_notified.store(false, Ordering::Relaxed);
            return Ok(());
        }
        player.as_ref().is_some_and(|player| player.is_paused())
    };
    rebuild_player(&state, target, paused)
}

pub fn set_volume(volume: f64, state: State<'_, AudioState>) {
    let volume = volume_to_rodio(volume);
    *state.volume.lock().unwrap() = volume;
    if let Some(ref player) = *state.player.lock().unwrap() {
        player.set_volume(volume);
    }
}

pub fn position(state: &AudioState) -> f64 {
    state
        .player
        .lock()
        .unwrap()
        .as_ref()
        .map(|player| player.get_pos().as_secs_f64())
        .unwrap_or(0.0)
}

pub fn set_normalization(enabled: bool, state: State<'_, AudioState>) {
    state
        .normalization_enabled
        .store(enabled, Ordering::Relaxed);
}

pub fn is_playing(state: &AudioState) -> bool {
    state
        .player
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|player| !player.is_paused() && !player.empty())
}

fn send_media(state: &AudioState, cmd: MediaCmd) {
    if let Some(tx) = state.media_tx.lock().unwrap().as_ref() {
        tx.send(cmd).ok();
    }
}

pub fn set_metadata(
    title: String,
    artist: String,
    cover_url: Option<String>,
    duration_secs: f64,
    state: State<'_, AudioState>,
) {
    send_media(
        &state,
        MediaCmd::SetMetadata {
            title,
            artist,
            cover_url,
            duration_secs,
        },
    );
}

pub fn set_playback_state(playing: bool, state: State<'_, AudioState>) {
    send_media(&state, MediaCmd::SetPlaying(playing));
}

pub fn set_media_position(position: f64, state: State<'_, AudioState>) {
    send_media(&state, MediaCmd::SetPosition(position));
}
