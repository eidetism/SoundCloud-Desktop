//! Playback primitives on the shared output. The player (`crate::player`)
//! drives these; the frontend never calls them directly.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::audio::decode::{create_player_from_bytes, resolve_normalization_gain};
use crate::audio::state::AudioState;
use crate::audio::types::{AudioThreadCmd, MediaCmd, STALL_SUPPRESS_MS};

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

/// Volume in percent (0–100) to rodio's linear factor.
fn volume_to_rodio(percent: f64) -> f32 {
    (percent / 100.0).clamp(0.0, 1.0) as f32
}

fn applied_gain(state: &AudioState) -> f32 {
    if state.normalization_enabled.load(Ordering::Relaxed) {
        *state.normalization_gain.lock().unwrap()
    } else {
        1.0
    }
}

/// A decoded track ready to replace the current one.
pub struct Prepared {
    player: rodio::Player,
    bytes: Vec<u8>,
    normalization_gain: f32,
}

/// Decode `bytes` into a paused player. Blocking (decoding, normalization
/// analysis): run off the async runtime. The current track keeps playing.
pub fn prepare(
    state: &AudioState,
    bytes: Vec<u8>,
    normalization_cache: Option<(PathBuf, String)>,
    duration: Option<Duration>,
) -> Result<Prepared, String> {
    let normalization_gain = if state.normalization_enabled.load(Ordering::Relaxed) {
        let (dir, key) = normalization_cache.unzip();
        resolve_normalization_gain(&bytes, dir.as_deref(), key.as_deref(), duration)?
    } else {
        1.0
    };
    let mixer = state.mixer.lock().unwrap().clone();
    let volume = *state.volume.lock().unwrap();
    let player = create_player_from_bytes(&bytes, &mixer, volume, normalization_gain, true)?;
    Ok(Prepared {
        player,
        bytes,
        normalization_gain,
    })
}

/// Swap in a prepared track, replacing whatever played before.
pub fn commit(state: &AudioState, prepared: Prepared, play: bool) {
    suppress_ended_temporarily(state);
    let Prepared {
        player: new_player,
        bytes,
        normalization_gain,
    } = prepared;
    new_player.set_volume(*state.volume.lock().unwrap());
    let mut player = state.player.lock().unwrap();
    if let Some(previous) = player.replace(new_player) {
        previous.stop();
    }
    *state.source_bytes.lock().unwrap() = Some(bytes);
    *state.normalization_gain.lock().unwrap() = normalization_gain;
    state.has_track.store(true, Ordering::Relaxed);
    state.ended_notified.store(false, Ordering::Relaxed);
    state.device_error.store(false, Ordering::Relaxed);
    if play {
        resume_device(state);
        if let Some(player) = player.as_ref() {
            player.play();
        }
    }
}

/// Recreate the player from the retained source bytes, seeked to `position` seconds.
fn rebuild_player(state: &AudioState, position: f64, paused: bool) -> Result<(), String> {
    let bytes = state.source_bytes.lock().unwrap().clone();
    let Some(bytes) = bytes else {
        return Err("No source to reload".into());
    };
    let mixer = state.mixer.lock().unwrap().clone();
    let volume = *state.volume.lock().unwrap();
    let new_player = create_player_from_bytes(&bytes, &mixer, volume, applied_gain(state), true)?;
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

/// If the device errored (sleep/wake, headphone unplug), reconnect right away
/// instead of waiting for stall detection.
fn resume_device(state: &AudioState) {
    if state.device_error.load(Ordering::Relaxed) {
        state.audio_tx.send(AudioThreadCmd::Reconnect).ok();
    }
}

pub fn play(state: &AudioState) {
    resume_device(state);
    if let Some(player) = state.player.lock().unwrap().as_ref() {
        player.play();
    }
}

pub fn pause(state: &AudioState) {
    if let Some(player) = state.player.lock().unwrap().as_ref() {
        player.pause();
    }
}

pub fn stop(state: &AudioState) {
    suppress_ended_temporarily(state);
    state.has_track.store(false, Ordering::Relaxed);
    if let Some(old) = state.player.lock().unwrap().take() {
        old.stop();
    }
    *state.source_bytes.lock().unwrap() = None;
}

/// Seek to `position` seconds, trying an in-place decoder seek first and recreating
/// the player when that fails: a bare `try_seek` silently no-ops on decoders that
/// can't seek in place.
pub fn seek(state: &AudioState, position: f64) -> Result<(), String> {
    suppress_ended_temporarily(state);
    let target = position.max(0.0);
    let paused = {
        let player = state.player.lock().unwrap();
        if let Some(player) = player.as_ref()
            && !player.empty()
            && player.try_seek(Duration::from_secs_f64(target)).is_ok()
        {
            state.ended_notified.store(false, Ordering::Relaxed);
            return Ok(());
        }
        player.as_ref().is_some_and(|player| player.is_paused())
    };
    rebuild_player(state, target, paused)
}

pub fn set_volume(state: &AudioState, percent: f64) {
    let volume = volume_to_rodio(percent);
    *state.volume.lock().unwrap() = volume;
    if let Some(player) = state.player.lock().unwrap().as_ref() {
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

pub fn set_normalization(state: &AudioState, enabled: bool) {
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

/// A loaded track that hasn't run out (paused or playing).
pub fn has_live_track(state: &AudioState) -> bool {
    state
        .player
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|player| !player.empty())
}

pub fn send_media(state: &AudioState, cmd: MediaCmd) {
    if let Some(tx) = state.media_tx.lock().unwrap().as_ref() {
        tx.send(cmd).ok();
    }
}
