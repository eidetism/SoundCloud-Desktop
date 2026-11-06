//! In-memory store of full tracks seen in API answers, so the player can
//! start a track the frontend knows only by id. Entries go stale (their
//! `track_authorization` and transcoding links expire) and are refetched.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::api::models::{CarriesTracks, Track};
use crate::api::{Api, ApiError, TRACKS_PER_LOOKUP};

/// Older entries are refetched before playback.
const FRESH_FOR: Duration = Duration::from_secs(20 * 60);
/// Above this many entries the oldest half is dropped.
const CAPACITY: usize = 4000;

#[derive(Default)]
pub struct TrackStore {
    tracks: Mutex<HashMap<u64, (Arc<Track>, Instant)>>,
}

impl TrackStore {
    pub fn remember(&self, carrier: &impl CarriesTracks) {
        let now = Instant::now();
        let mut tracks = self.tracks.lock().unwrap();
        carrier.for_each_track(&mut |track| {
            tracks.insert(track.id, (Arc::new(track.clone()), now));
        });
        if tracks.len() > CAPACITY {
            evict_oldest_half(&mut tracks);
        }
    }

    pub fn get(&self, id: u64) -> Option<Arc<Track>> {
        self.tracks.lock().unwrap().get(&id).map(|(t, _)| t.clone())
    }

    fn fresh(&self, id: u64) -> Option<Arc<Track>> {
        let tracks = self.tracks.lock().unwrap();
        let (track, at) = tracks.get(&id)?;
        (at.elapsed() < FRESH_FOR).then(|| track.clone())
    }

    /// One track, fetched if missing, stale or `force`d.
    pub async fn load(
        &self,
        api: &Api,
        http: &reqwest::Client,
        token: &str,
        id: u64,
        force: bool,
    ) -> Result<Option<Arc<Track>>, ApiError> {
        if !force && let Some(track) = self.fresh(id) {
            return Ok(Some(track));
        }
        let fetched = api.tracks(http, token, &[id]).await?;
        self.remember(&fetched);
        Ok(self.get(id))
    }

    /// Tracks for `ids` in the given order, fetching the unknown ones in
    /// batches. Ids SoundCloud no longer knows are left out.
    pub async fn resolve(
        &self,
        api: &Api,
        http: &reqwest::Client,
        token: &str,
        ids: &[u64],
    ) -> Result<Vec<Arc<Track>>, ApiError> {
        let missing: Vec<u64> = ids
            .iter()
            .copied()
            .filter(|&id| self.get(id).is_none())
            .collect();
        for batch in missing.chunks(TRACKS_PER_LOOKUP) {
            let fetched = api.tracks(http, token, batch).await?;
            self.remember(&fetched);
        }
        Ok(ids.iter().filter_map(|&id| self.get(id)).collect())
    }
}

fn evict_oldest_half(tracks: &mut HashMap<u64, (Arc<Track>, Instant)>) {
    let mut ages: Vec<Instant> = tracks.values().map(|(_, at)| *at).collect();
    ages.sort_unstable();
    let cutoff = ages[ages.len() / 2];
    tracks.retain(|_, (_, at)| *at >= cutoff);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_full_tracks_from_any_answer() {
        let store = TrackStore::default();
        let playlist: crate::api::models::Playlist =
            serde_json::from_str(include_str!("../api/fixtures/playlist.json")).unwrap();
        store.remember(&playlist);
        let full: Vec<u64> = playlist
            .tracks
            .iter()
            .filter_map(|t| match t {
                crate::api::models::TrackRef::Full(t) => Some(t.id),
                crate::api::models::TrackRef::Stub { .. } => None,
            })
            .collect();
        assert_eq!(full.len(), 5);
        assert!(full.iter().all(|&id| store.get(id).is_some()));
        let stub = playlist.tracks.last().unwrap().id();
        assert!(store.get(stub).is_none());
        assert!(store.fresh(full[0]).is_some());
    }

    #[test]
    fn eviction_keeps_the_newer_half() {
        let track: Track =
            serde_json::from_str(include_str!("../api/fixtures/track_drm.json")).unwrap();
        let start = Instant::now();
        let mut tracks: HashMap<u64, (Arc<Track>, Instant)> = (0..10u64)
            .map(|i| (i, (Arc::new(track.clone()), start + Duration::from_secs(i))))
            .collect();
        evict_oldest_half(&mut tracks);
        let mut kept: Vec<u64> = tracks.keys().copied().collect();
        kept.sort_unstable();
        assert_eq!(kept, [5, 6, 7, 8, 9]);
    }
}
