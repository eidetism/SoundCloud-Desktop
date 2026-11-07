//! `index.json`: what the offline folder holds, with enough of each track to
//! show and play it without the network. Kept next to the audio so moving
//! the folder carries it along. Plain snake_case both ways (the API models
//! read snake_case but send camelCase, so they can't round-trip).

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::api::models::{Media, Track, User};

pub const FILE: &str = "index.json";
const VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: u64,
    #[serde(default)]
    pub urn: String,
    pub title: String,
    #[serde(default)]
    pub user_id: Option<u64>,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub user_permalink: String,
    #[serde(default)]
    pub user_avatar_url: Option<String>,
    #[serde(default)]
    pub artwork_url: Option<String>,
    pub duration_ms: u64,
    #[serde(default)]
    pub permalink_url: String,
    #[serde(default)]
    pub waveform_url: Option<String>,
    pub bytes: u64,
    /// Saved with the "Offline" button (`saved/`), else kept as recent.
    pub saved: bool,
    /// Unix seconds.
    pub added_at: u64,
    /// Unix seconds; recent entries go oldest first.
    pub played_at: u64,
}

impl Entry {
    pub fn new(track: &Track, bytes: u64, saved: bool, now: u64) -> Self {
        let user = track.user.as_ref();
        Self {
            id: track.id,
            urn: track.urn.clone(),
            title: track.title.clone(),
            user_id: user.map(|u| u.id),
            username: user.map(|u| u.username.clone()).unwrap_or_default(),
            user_permalink: user.map(|u| u.permalink.clone()).unwrap_or_default(),
            user_avatar_url: user.and_then(|u| u.avatar_url.clone()),
            artwork_url: track.artwork_url.clone(),
            duration_ms: track.duration,
            permalink_url: track.permalink_url.clone(),
            waveform_url: track.waveform_url.clone(),
            bytes,
            saved,
            added_at: now,
            played_at: now,
        }
    }

    /// The track as the player and the frontend know it. It has no
    /// transcodings: it plays from the file.
    pub fn track(&self) -> Track {
        let user = self.user_id.map(|id| User {
            id,
            username: self.username.clone(),
            permalink: self.user_permalink.clone(),
            permalink_url: if self.user_permalink.is_empty() {
                String::new()
            } else {
                format!("https://soundcloud.com/{}", self.user_permalink)
            },
            avatar_url: self.user_avatar_url.clone(),
            verified: false,
            followers_count: None,
            track_count: None,
        });
        Track {
            id: self.id,
            urn: self.urn.clone(),
            title: self.title.clone(),
            user,
            artwork_url: self.artwork_url.clone(),
            duration: self.duration_ms,
            permalink_url: self.permalink_url.clone(),
            waveform_url: self.waveform_url.clone(),
            genre: None,
            display_date: None,
            likes_count: None,
            reposts_count: None,
            playback_count: None,
            comment_count: None,
            description: None,
            tags: Vec::new(),
            media: Media::default(),
            track_authorization: None,
        }
    }

    pub fn file_name(&self) -> String {
        file_name(self.id)
    }
}

pub fn file_name(id: u64) -> String {
    format!("{id}.mp4")
}

#[derive(Clone, Default, Debug, PartialEq)]
pub struct Index {
    pub tracks: BTreeMap<u64, Entry>,
}

#[derive(Serialize, Deserialize)]
struct Stored {
    version: u32,
    tracks: Vec<Entry>,
}

impl Index {
    pub fn load(path: &Path) -> Self {
        let Some(stored) = std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Stored>(&bytes).ok())
        else {
            return Self::default();
        };
        Self {
            tracks: stored.tracks.into_iter().map(|e| (e.id, e)).collect(),
        }
    }

    /// Atomic: written next to the target, then renamed over it.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let stored = Stored {
            version: VERSION,
            tracks: self.tracks.values().cloned().collect(),
        };
        let bytes = serde_json::to_vec_pretty(&stored).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, path)
    }

    pub fn bytes(&self, saved: bool) -> u64 {
        self.tracks
            .values()
            .filter(|e| e.saved == saved)
            .map(|e| e.bytes)
            .sum()
    }

    /// Recent entries to drop, least recently played first, until recent
    /// files fit in `limit` bytes.
    pub fn over_limit(&self, limit: u64) -> Vec<u64> {
        let mut recent: Vec<&Entry> = self.tracks.values().filter(|e| !e.saved).collect();
        recent.sort_by_key(|e| (e.played_at, e.id));
        let mut total: u64 = recent.iter().map(|e| e.bytes).sum();
        let mut drop = Vec::new();
        for entry in recent {
            if total <= limit {
                break;
            }
            total -= entry.bytes;
            drop.push(entry.id);
        }
        drop
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: u64, bytes: u64, saved: bool, played_at: u64) -> Entry {
        Entry {
            id,
            urn: format!("soundcloud:tracks:{id}"),
            title: format!("t{id}"),
            user_id: Some(7),
            username: "artist".into(),
            user_permalink: "artist".into(),
            user_avatar_url: None,
            artwork_url: None,
            duration_ms: 1000,
            permalink_url: String::new(),
            waveform_url: None,
            bytes,
            saved,
            added_at: 0,
            played_at,
        }
    }

    #[test]
    fn recent_overflow_drops_least_recently_played_and_keeps_saved() {
        let mut index = Index::default();
        for e in [
            entry(1, 40, false, 30),
            entry(2, 40, false, 10),
            entry(3, 40, false, 20),
            entry(4, 500, true, 0),
        ] {
            index.tracks.insert(e.id, e);
        }
        assert_eq!(index.over_limit(120), Vec::<u64>::new());
        assert_eq!(index.over_limit(100), vec![2]);
        assert_eq!(index.over_limit(0), vec![2, 3, 1]);
        assert_eq!((index.bytes(true), index.bytes(false)), (500, 120));
    }

    #[test]
    fn index_round_trips_and_tolerates_garbage() {
        let dir = std::env::temp_dir().join(format!("pulse-index-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE);
        let mut index = Index::default();
        index.tracks.insert(5, entry(5, 10, true, 1));
        index.save(&path).unwrap();
        assert_eq!(Index::load(&path), index);
        std::fs::write(&path, b"not json").unwrap();
        assert_eq!(Index::load(&path), Index::default());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn entries_rebuild_a_playable_track() {
        let track = entry(9, 1, true, 0).track();
        assert_eq!((track.id, track.duration), (9, 1000));
        let user = track.user.unwrap();
        assert_eq!(user.permalink_url, "https://soundcloud.com/artist");
        assert!(track.media.transcodings.is_empty());
    }
}
