use tauri::{Manager, State};

use crate::audio::device;
use crate::audio::engine;
use crate::audio::state::AudioState;
use crate::audio::types::{AudioLoadResult, AudioSink};
use crate::rt::AppHandle;
use crate::shared::blocking::run_blocking;

#[tauri::command]
pub async fn audio_load_file(
    path: String,
    cache_key: Option<String>,
    start_paused: bool,
    app: AppHandle,
    state: State<'_, AudioState>,
) -> Result<AudioLoadResult, String> {
    let normalization_cache_dir = app
        .path()
        .app_cache_dir()
        .ok()
        .map(|dir| dir.join("audio-normalization"));
    engine::load_file(
        path,
        normalization_cache_dir,
        cache_key,
        start_paused,
        state,
    )
    .await
}

#[tauri::command]
pub fn audio_play(state: State<'_, AudioState>) {
    engine::play(state);
}

#[tauri::command]
pub fn audio_pause(state: State<'_, AudioState>) {
    engine::pause(state);
}

#[tauri::command]
pub fn audio_stop(state: State<'_, AudioState>) {
    engine::stop(state);
}

#[tauri::command]
pub fn audio_seek(position: f64, state: State<'_, AudioState>) -> Result<(), String> {
    engine::seek(position, state)
}

#[tauri::command]
pub fn audio_set_volume(volume: f64, state: State<'_, AudioState>) {
    engine::set_volume(volume, state);
}

#[tauri::command]
pub fn audio_get_position(state: State<'_, AudioState>) -> f64 {
    engine::position(&state)
}

#[tauri::command]
pub fn audio_set_normalization(enabled: bool, state: State<'_, AudioState>) {
    engine::set_normalization(enabled, state);
}

#[tauri::command]
pub fn audio_is_playing(state: State<'_, AudioState>) -> bool {
    engine::is_playing(&state)
}

#[tauri::command]
pub fn audio_set_metadata(
    title: String,
    artist: String,
    cover_url: Option<String>,
    duration_secs: f64,
    state: State<'_, AudioState>,
) {
    engine::set_metadata(title, artist, cover_url, duration_secs, state);
}

#[tauri::command]
pub fn audio_set_playback_state(playing: bool, state: State<'_, AudioState>) {
    engine::set_playback_state(playing, state);
}

#[tauri::command]
pub fn audio_set_media_position(position: f64, state: State<'_, AudioState>) {
    engine::set_media_position(position, state);
}

#[tauri::command]
pub async fn audio_list_devices(app: AppHandle) -> Result<Vec<AudioSink>, String> {
    run_blocking(move || device::list_devices(&app.state::<AudioState>())).await
}

#[tauri::command]
pub async fn audio_switch_device(
    device_name: Option<String>,
    app: AppHandle,
) -> Result<(), String> {
    run_blocking(move || device::switch_device(app.state(), device_name)).await?
}

#[tauri::command]
pub fn audio_set_follow_default_output(follow: bool, state: State<'_, AudioState>) {
    device::set_follow_default_output(state, follow);
}
