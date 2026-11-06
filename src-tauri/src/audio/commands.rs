//! Output-device commands. Playback itself is driven by
//! `crate::player`.

use tauri::{Manager, State};

use crate::audio::device;
use crate::audio::state::AudioState;
use crate::audio::types::AudioSink;
use crate::rt::AppHandle;
use crate::shared::blocking::run_blocking;

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
