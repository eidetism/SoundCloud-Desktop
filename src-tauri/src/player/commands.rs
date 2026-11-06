//! Player commands for the frontend. Each one queues a command for the
//! player actor; results arrive as `player:state` events.

use tauri::State;

use crate::player::{self, Command, PlayerState, Repeat, Snapshot};
use crate::rt::AppHandle;

#[tauri::command]
pub fn player_state(player: State<'_, PlayerState>) -> Snapshot {
    player.snapshot()
}

/// Replace the queue with `ids` and play `ids[start]`.
#[tauri::command]
pub fn player_play(app: AppHandle, ids: Vec<u64>, start: usize) {
    player::send(&app, Command::PlayTracks { ids, start });
}

#[tauri::command]
pub fn player_toggle(app: AppHandle) {
    player::send(&app, Command::Toggle);
}

#[tauri::command]
pub fn player_next(app: AppHandle) {
    player::send(&app, Command::Next);
}

#[tauri::command]
pub fn player_previous(app: AppHandle) {
    player::send(&app, Command::Previous);
}

#[tauri::command]
pub fn player_seek(app: AppHandle, seconds: f64) {
    player::send(&app, Command::Seek(seconds));
}

/// `percent`: 0–100.
#[tauri::command]
pub fn player_set_volume(app: AppHandle, percent: f64) {
    player::send(&app, Command::SetVolume(percent));
}

#[tauri::command]
pub fn player_set_shuffle(app: AppHandle, shuffle: bool) {
    player::send(&app, Command::SetShuffle(shuffle));
}

#[tauri::command]
pub fn player_set_repeat(app: AppHandle, repeat: Repeat) {
    player::send(&app, Command::SetRepeat(repeat));
}

#[tauri::command]
pub fn player_play_next(app: AppHandle, ids: Vec<u64>) {
    player::send(&app, Command::PlayNext(ids));
}

#[tauri::command]
pub fn player_append(app: AppHandle, ids: Vec<u64>) {
    player::send(&app, Command::Append(ids));
}

/// Jump `steps` tracks ahead in play order (1 = the next one).
#[tauri::command]
pub fn player_skip_ahead(app: AppHandle, steps: usize) {
    player::send(&app, Command::SkipAhead(steps));
}
