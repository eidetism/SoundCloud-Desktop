//! Offline audio: tracks saved with the "Offline" button (`saved/`, kept
//! until removed) and recently played ones (`recent/`, trimmed to a size
//! limit, least recently played first). A file is the fMP4 the player
//! already downloaded for a plain stream; previews and DRM are never kept.
//! The player looks here before the network, and a kept track plays without
//! one from the metadata in `index.json`.
//!
//! The folder is `Pulse Offline` inside a parent the user picks (by default
//! the app's local data dir); `relocate` moves it.

pub mod commands;
mod index;
mod relocate;

use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{Emitter, Manager};

use crate::api::models::Track;
use crate::app::diagnostics;
use crate::rt::AppHandle;
use index::{Entry, Index};

pub const ROOT: &str = "Pulse Offline";
const SAVED: &str = "saved";
const RECENT: &str = "recent";
const PART: &str = "part";
const EVENT: &str = "offline:changed";
const MB: u64 = 1024 * 1024;

pub struct Offline {
    default_parent: PathBuf,
    root: Mutex<PathBuf>,
    index: Mutex<Index>,
    /// Cache writes share it; moving the folder takes it alone.
    lock: tokio::sync::RwLock<()>,
    downloading: Mutex<BTreeSet<u64>>,
    recent_limit: AtomicU64,
}

/// What the UI shows of the cache.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub folder: PathBuf,
    pub default_folder: bool,
    pub saved_bytes: u64,
    pub saved_count: usize,
    pub recent_bytes: u64,
    pub recent_count: usize,
    pub recent_limit_mb: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflineIds {
    pub saved: Vec<u64>,
    pub downloading: Vec<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedTrack {
    pub track: Track,
    pub bytes: u64,
    pub added_at: u64,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn root_in(parent: &Path) -> PathBuf {
    parent.join(ROOT)
}

impl Offline {
    /// Open the folder under `parent` (or the default), dropping index
    /// entries whose file is gone.
    pub fn open(default_parent: PathBuf, parent: Option<PathBuf>, recent_limit_mb: u64) -> Self {
        let root = root_in(parent.as_deref().unwrap_or(&default_parent));
        let mut index = Index::load(&root.join(index::FILE));
        let before = index.tracks.len();
        index.tracks.retain(|_, e| entry_path(&root, e).is_file());
        let offline = Self {
            default_parent,
            root: Mutex::new(root),
            index: Mutex::new(index),
            lock: tokio::sync::RwLock::new(()),
            downloading: Mutex::default(),
            recent_limit: AtomicU64::new(recent_limit_mb * MB),
        };
        if offline.index.lock().unwrap().tracks.len() != before {
            offline.persist(&offline.index.lock().unwrap());
        }
        offline
    }

    fn root(&self) -> PathBuf {
        self.root.lock().unwrap().clone()
    }

    fn persist(&self, index: &Index) {
        let root = self.root();
        let result =
            std::fs::create_dir_all(&root).and_then(|()| index.save(&root.join(index::FILE)));
        if let Err(error) = result {
            diagnostics::warn(format!("[offline] index: {error}"));
        }
    }

    /// Shared access for a reader; `None` while the folder is being moved.
    pub fn lock_for_read(&self) -> Option<tokio::sync::RwLockReadGuard<'_, ()>> {
        self.lock.try_read().ok()
    }

    pub fn kept_track(&self, id: u64) -> Option<Track> {
        self.index.lock().unwrap().tracks.get(&id).map(Entry::track)
    }

    pub fn has(&self, id: u64) -> bool {
        self.index.lock().unwrap().tracks.contains_key(&id)
    }

    /// A kept track's audio and metadata; marks it played.
    pub fn read(&self, id: u64) -> Option<(Track, Vec<u8>)> {
        let root = self.root();
        let entry = self.index.lock().unwrap().tracks.get(&id)?.clone();
        let bytes = match std::fs::read(entry_path(&root, &entry)) {
            Ok(bytes) => bytes,
            Err(error) => {
                diagnostics::warn(format!("[offline] read {id}: {error}"));
                let mut index = self.index.lock().unwrap();
                index.tracks.remove(&id);
                self.persist(&index);
                return None;
            }
        };
        let mut index = self.index.lock().unwrap();
        if let Some(kept) = index.tracks.get_mut(&id) {
            kept.played_at = now();
        }
        self.persist(&index);
        Some((entry.track(), bytes))
    }

    /// Keep `bytes` for `track`: as saved, or as recent (if recents are on).
    /// A recent track being saved is moved, not rewritten.
    pub fn put(&self, track: &Track, bytes: &[u8], saved: bool) -> io::Result<()> {
        let limit = self.recent_limit.load(Ordering::Relaxed);
        let root = self.root();
        let mut index = self.index.lock().unwrap();
        if let Some(entry) = index.tracks.get(&track.id).cloned() {
            if saved && !entry.saved {
                move_file(
                    &entry_path(&root, &entry),
                    &root.join(SAVED).join(entry.file_name()),
                )?;
                let kept = index.tracks.get_mut(&track.id).expect("present");
                kept.saved = true;
                kept.added_at = now();
                self.persist(&index);
            }
            return Ok(());
        }
        if !saved && limit == 0 {
            return Ok(());
        }
        let dir = root.join(if saved { SAVED } else { RECENT });
        std::fs::create_dir_all(&dir)?;
        let target = dir.join(index::file_name(track.id));
        let part = target.with_extension(PART);
        std::fs::write(&part, bytes)?;
        std::fs::rename(&part, &target)?;
        index.tracks.insert(
            track.id,
            Entry::new(track, bytes.len() as u64, saved, now()),
        );
        self.trim(&root, &mut index, limit);
        self.persist(&index);
        Ok(())
    }

    /// Un-save: the file becomes a recent one (or goes, with recents off).
    pub fn unsave(&self, id: u64) -> io::Result<()> {
        let limit = self.recent_limit.load(Ordering::Relaxed);
        let root = self.root();
        let mut index = self.index.lock().unwrap();
        let Some(entry) = index.tracks.get(&id).cloned().filter(|e| e.saved) else {
            return Ok(());
        };
        let from = entry_path(&root, &entry);
        if limit == 0 {
            remove_file(&from);
            index.tracks.remove(&id);
        } else {
            move_file(&from, &root.join(RECENT).join(entry.file_name()))?;
            let kept = index.tracks.get_mut(&id).expect("present");
            kept.saved = false;
            kept.played_at = now();
            self.trim(&root, &mut index, limit);
        }
        self.persist(&index);
        Ok(())
    }

    pub fn clear_recent(&self) {
        let root = self.root();
        let mut index = self.index.lock().unwrap();
        self.trim(&root, &mut index, 0);
        self.persist(&index);
    }

    pub fn set_recent_limit(&self, megabytes: u64) {
        let limit = megabytes * MB;
        self.recent_limit.store(limit, Ordering::Relaxed);
        let root = self.root();
        let mut index = self.index.lock().unwrap();
        self.trim(&root, &mut index, limit);
        self.persist(&index);
    }

    fn trim(&self, root: &Path, index: &mut Index, limit: u64) {
        for id in index.over_limit(limit) {
            if let Some(entry) = index.tracks.remove(&id) {
                remove_file(&entry_path(root, &entry));
            }
        }
    }

    pub fn summary(&self) -> Summary {
        let index = self.index.lock().unwrap();
        let root = self.root();
        Summary {
            default_folder: root == root_in(&self.default_parent),
            folder: root,
            saved_bytes: index.bytes(true),
            saved_count: index.tracks.values().filter(|e| e.saved).count(),
            recent_bytes: index.bytes(false),
            recent_count: index.tracks.values().filter(|e| !e.saved).count(),
            recent_limit_mb: self.recent_limit.load(Ordering::Relaxed) / MB,
        }
    }

    /// Saved tracks, most recently saved first.
    pub fn saved_tracks(&self) -> Vec<SavedTrack> {
        let index = self.index.lock().unwrap();
        let mut saved: Vec<SavedTrack> = index
            .tracks
            .values()
            .filter(|e| e.saved)
            .map(|e| SavedTrack {
                track: e.track(),
                bytes: e.bytes,
                added_at: e.added_at,
            })
            .collect();
        saved.sort_by_key(|s| std::cmp::Reverse(s.added_at));
        saved
    }

    pub fn ids(&self) -> OfflineIds {
        let index = self.index.lock().unwrap();
        OfflineIds {
            saved: index
                .tracks
                .values()
                .filter(|e| e.saved)
                .map(|e| e.id)
                .collect(),
            downloading: self.downloading.lock().unwrap().iter().copied().collect(),
        }
    }

    /// Mark a download started (`false` if it already runs).
    fn begin(&self, id: u64) -> bool {
        self.downloading.lock().unwrap().insert(id)
    }

    fn end(&self, id: u64) {
        self.downloading.lock().unwrap().remove(&id);
    }
}

fn entry_path(root: &Path, entry: &Entry) -> PathBuf {
    root.join(if entry.saved { SAVED } else { RECENT })
        .join(entry.file_name())
}

fn move_file(from: &Path, to: &Path) -> io::Result<()> {
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::rename(from, to)
}

fn remove_file(path: &Path) {
    if let Err(error) = std::fs::remove_file(path)
        && error.kind() != io::ErrorKind::NotFound
    {
        diagnostics::warn(format!("[offline] remove: {error}"));
    }
}

pub fn notify(app: &AppHandle) {
    app.emit(EVENT, app.state::<Offline>().ids()).ok();
}

/// Keep a track the player just downloaded as a recent one, in the
/// background. Skipped when recents are off or it is already kept.
pub fn remember_played(app: &AppHandle, track: std::sync::Arc<Track>, bytes: &[u8]) {
    let offline = app.state::<Offline>();
    if offline.recent_limit.load(Ordering::Relaxed) == 0 || offline.has(track.id) {
        return;
    }
    let bytes = bytes.to_vec();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let offline = app.state::<Offline>();
        let _shared = offline.lock.read().await;
        let writer = app.clone();
        let result = tokio::task::spawn_blocking(move || {
            writer.state::<Offline>().put(&track, &bytes, false)
        })
        .await;
        if let Ok(Err(error)) = result {
            diagnostics::warn(format!("[offline] keep recent: {error}"));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::Track;

    fn track(id: u64) -> Track {
        serde_json::from_value(serde_json::json!({
            "id": id, "title": format!("t{id}"), "duration": 1000,
            "user": { "id": 7, "username": "a", "permalink": "a" }
        }))
        .unwrap()
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pulse-offline-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn save_promote_unsave_and_trim() {
        let parent = temp("cycle");
        let offline = Offline::open(parent.clone(), None, 1);
        let root = parent.join(ROOT);
        offline.put(&track(1), &[0; 10], false).unwrap();
        assert!(root.join("recent/1.mp4").is_file());
        // Saving a recent track moves its file.
        offline.put(&track(1), &[], true).unwrap();
        assert!(root.join("saved/1.mp4").is_file() && !root.join("recent/1.mp4").exists());
        assert_eq!(offline.ids().saved, vec![1]);
        // Un-saving keeps it as recent; recents over the limit go.
        offline.unsave(1).unwrap();
        assert!(root.join("recent/1.mp4").is_file());
        offline
            .put(&track(2), &vec![0; MB as usize], false)
            .unwrap();
        assert!(!offline.has(1) && offline.has(2));
        // The index survives a reopen; a missing file drops its entry.
        std::fs::remove_file(root.join("recent/2.mp4")).unwrap();
        offline.put(&track(3), &[1, 2, 3], true).unwrap();
        let reopened = Offline::open(parent.clone(), None, 1);
        assert!(reopened.has(3) && !reopened.has(2));
        let (played, bytes) = reopened.read(3).unwrap();
        assert_eq!((played.id, bytes), (3, vec![1, 2, 3]));
        std::fs::remove_dir_all(&parent).unwrap();
    }

    #[test]
    fn recents_off_keeps_nothing_automatically() {
        let parent = temp("off");
        let offline = Offline::open(parent.clone(), None, 0);
        offline.put(&track(1), &[0; 4], false).unwrap();
        assert!(!offline.has(1));
        offline.put(&track(2), &[0; 4], true).unwrap();
        offline.unsave(2).unwrap();
        assert!(!offline.has(2));
        let _ = std::fs::remove_dir_all(&parent);
    }
}
