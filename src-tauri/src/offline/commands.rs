//! Offline commands for the frontend. Changes to what is saved or being
//! downloaded go out as `offline:changed`; a folder move reports progress as
//! `offline:relocate` (`[done, total]` bytes).

use std::path::PathBuf;

use tauri::{Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use super::{Offline, OfflineIds, SavedTrack, Summary, notify, relocate};
use crate::app::diagnostics;
use crate::player::PlayerState;
use crate::rt::AppHandle;
use crate::settings::SettingsStore;

const RELOCATE_EVENT: &str = "offline:relocate";

#[tauri::command]
pub fn offline_ids(offline: State<'_, Offline>) -> OfflineIds {
    offline.ids()
}

#[tauri::command]
pub fn offline_summary(offline: State<'_, Offline>) -> Summary {
    offline.summary()
}

/// Saved tracks, newest first; the player learns them so they play by id.
#[tauri::command]
pub fn offline_saved(app: AppHandle, offline: State<'_, Offline>) -> Vec<SavedTrack> {
    let saved = offline.saved_tracks();
    let tracks: Vec<_> = saved.iter().map(|s| s.track.clone()).collect();
    app.state::<PlayerState>().store.remember(&tracks);
    saved
}

/// Download (or promote from recents) a track for offline. Errors: `drm`,
/// `preview`, `unavailable`, or a network message.
#[tauri::command]
pub async fn offline_save(app: AppHandle, id: u64) -> Result<(), String> {
    let offline = app.state::<Offline>();
    if !offline.begin(id) {
        return Ok(());
    }
    notify(&app);
    let result = save(&app, id).await;
    offline.end(id);
    notify(&app);
    result.inspect_err(|error| diagnostics::warn(format!("[offline] save {id}: {error}")))
}

async fn save(app: &AppHandle, id: u64) -> Result<(), String> {
    let offline = app.state::<Offline>();
    let _shared = offline.lock.read().await;
    if offline.has(id) {
        let writer = app.clone();
        return tokio::task::spawn_blocking(move || {
            let offline = writer.state::<Offline>();
            let track = offline.kept_track(id).ok_or("gone")?;
            offline.put(&track, &[], true).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())?;
    }
    let (track, bytes) = crate::player::download(app, id).await?;
    let writer = app.clone();
    tokio::task::spawn_blocking(move || writer.state::<Offline>().put(&track, &bytes, true))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn offline_remove(app: AppHandle, id: u64) -> Result<(), String> {
    let offline = app.state::<Offline>();
    let _shared = offline.lock.read().await;
    let writer = app.clone();
    tokio::task::spawn_blocking(move || writer.state::<Offline>().unsave(id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    notify(&app);
    Ok(())
}

#[tauri::command]
pub async fn offline_clear_recent(app: AppHandle) -> Result<Summary, String> {
    let offline = app.state::<Offline>();
    let _shared = offline.lock.read().await;
    let writer = app.clone();
    tokio::task::spawn_blocking(move || writer.state::<Offline>().clear_recent())
        .await
        .map_err(|e| e.to_string())?;
    Ok(offline.summary())
}

/// Size limit of recently played tracks, MB (0 turns them off).
#[tauri::command]
pub async fn offline_set_recent_limit(
    app: AppHandle,
    store: State<'_, SettingsStore>,
    megabytes: u64,
) -> Result<Summary, String> {
    store.update(|settings| settings.recent_cache_mb = megabytes)?;
    let offline = app.state::<Offline>();
    let _shared = offline.lock.read().await;
    let writer = app.clone();
    tokio::task::spawn_blocking(move || writer.state::<Offline>().set_recent_limit(megabytes))
        .await
        .map_err(|e| e.to_string())?;
    Ok(offline.summary())
}

/// Ask for a folder; `None` if the dialog was cancelled.
#[tauri::command]
pub async fn offline_pick_folder(app: AppHandle) -> Result<Option<PathBuf>, String> {
    let picker = app.clone();
    let picked = tokio::task::spawn_blocking(move || picker.dialog().file().blocking_pick_folder())
        .await
        .map_err(|e| e.to_string())?;
    picked
        .map(|path| path.into_path().map_err(|e| e.to_string()))
        .transpose()
}

/// Move the cache into `<parent>/Pulse Offline` (`parent: null` — back to
/// the default place). Errors: `same`, `nested`, `not_writable`, or an I/O
/// message (then nothing changed).
#[tauri::command]
pub async fn offline_relocate(
    app: AppHandle,
    store: State<'_, SettingsStore>,
    parent: Option<PathBuf>,
) -> Result<Summary, String> {
    let offline = app.state::<Offline>();
    let _exclusive = offline.lock.write().await;
    let old_root = offline.root();
    let target_parent = parent
        .clone()
        .unwrap_or_else(|| offline.default_parent.clone());
    let worker = app.clone();
    let moved = tokio::task::spawn_blocking(move || {
        let new_root =
            relocate::target(&old_root, &target_parent).map_err(|r| r.code().to_string())?;
        let offline = worker.state::<Offline>();
        let index = offline.index.lock().unwrap().clone();
        let merged = relocate::move_all(&old_root, &new_root, &index, |done, total| {
            worker.emit(RELOCATE_EVENT, (done, total)).ok();
        })
        .map_err(|e| e.to_string())?;
        Ok::<_, String>((new_root, merged))
    })
    .await
    .map_err(|e| e.to_string())?
    .inspect_err(|error| diagnostics::warn(format!("[offline] relocate: {error}")))?;
    let (new_root, merged) = moved;
    let default = parent.as_ref().is_none_or(|p| *p == offline.default_parent);
    store.update(|settings| settings.offline_dir = if default { None } else { parent.clone() })?;
    *offline.root.lock().unwrap() = new_root;
    *offline.index.lock().unwrap() = merged;
    diagnostics::log("INFO", "[offline] folder moved");
    notify(&app);
    Ok(offline.summary())
}
