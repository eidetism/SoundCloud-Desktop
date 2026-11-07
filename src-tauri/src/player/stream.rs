//! From a track to playable bytes: pick a transcoding, exchange it for a
//! signed playlist URL, download the HLS stream.
//!
//! Per the HAR captures api-v2 serves only HLS now: fMP4 AAC presets
//! (`aac_160k`, `aac_256k`, `aac_96k`), `abr_*` master playlists, and for
//! some label tracks only DRM (`*-encrypted-hls`). There is no progressive
//! MP3 and no Opus any more. A track with only DRM streams gets its CENC
//! (`ctr-encrypted-hls`) stream downloaded still encrypted, with the licence
//! token, for the WebView player (`player::drm`); nothing is decrypted here.
//! A Go+ track without a subscription plays its 30-second preview
//! (`snipped`).

use crate::api::models::{Media, Track, Transcoding};
use crate::api::{Api, ApiError};
use crate::app::diagnostics;
use crate::player::hls::{self, HlsError};

/// Preference among plain fMP4 presets: the Go+ high-quality stream first
/// (refused for accounts without the subscription), then what the web
/// player uses, then the low bitrate.
const PRESETS: [&str; 3] = ["aac_256k", "aac_160k", "aac_96k"];

#[derive(Debug)]
pub enum StreamError {
    /// Nothing we may play: DRM only, no transcodings, or withdrawn.
    Unavailable,
    /// The track's signatures went stale; refetch the track and retry.
    Expired,
    Unauthorized,
    Network(String),
}

impl std::fmt::Display for StreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamError::Unavailable => write!(f, "unavailable"),
            StreamError::Expired => write!(f, "stream link expired"),
            StreamError::Unauthorized => write!(f, "unauthorized"),
            StreamError::Network(message) => write!(f, "{message}"),
        }
    }
}

impl From<ApiError> for StreamError {
    fn from(error: ApiError) -> Self {
        match error {
            ApiError::Unauthorized => StreamError::Unauthorized,
            ApiError::Http(401 | 403 | 404 | 410) => StreamError::Expired,
            ApiError::BadUrl => StreamError::Unavailable,
            other => StreamError::Network(other.to_string()),
        }
    }
}

impl From<HlsError> for StreamError {
    fn from(error: HlsError) -> Self {
        match error {
            HlsError::Gone => StreamError::Expired,
            HlsError::Unsupported(reason) => {
                diagnostics::log("INFO", format!("[player] stream refused: {reason}"));
                StreamError::Unavailable
            }
            HlsError::Network(message) => StreamError::Network(message),
        }
    }
}

pub struct Audio {
    pub payload: Payload,
    /// Only the 30-second preview was available.
    pub preview: bool,
    /// Read from the offline folder, not downloaded.
    pub from_cache: bool,
}

pub enum Payload {
    /// Plain fMP4 for the Rust audio engine.
    Plain(Vec<u8>),
    /// Encrypted CENC segments for the WebView player.
    Drm(DrmMedia),
}

/// A DRM stream as the WebView player needs it: segments apart (it seeks by
/// them) and the token the licence server wants.
pub struct DrmMedia {
    pub init: Vec<u8>,
    pub segments: Vec<Vec<u8>>,
    pub durations: Vec<f64>,
    pub license_token: String,
}

/// Transcodings worth trying, best first: full streams before previews;
/// among those, the preferred plain-fMP4 preset. DRM and `abr_*` master
/// playlists are left out.
pub fn ranked(media: &Media) -> Vec<&Transcoding> {
    let playable = |t: &&Transcoding| {
        !t.is_encrypted()
            && t.format.protocol == "hls"
            && t.format.mime_type.starts_with("audio/mp4")
    };
    let rank = |t: &Transcoding| {
        let preset = PRESETS
            .iter()
            .position(|p| *p == t.preset)
            .unwrap_or(PRESETS.len());
        (t.snipped, preset)
    };
    let mut candidates: Vec<&Transcoding> = media.transcodings.iter().filter(playable).collect();
    candidates.sort_by_key(|t| rank(t));
    candidates
}

/// CENC streams in the same preference, for tracks with nothing plain.
pub fn ranked_drm(media: &Media) -> Vec<&Transcoding> {
    let preset = |t: &Transcoding| {
        PRESETS
            .iter()
            .position(|p| *p == t.preset)
            .unwrap_or(PRESETS.len())
    };
    let mut candidates: Vec<&Transcoding> = media
        .transcodings
        .iter()
        .filter(|t| t.is_cenc() && !t.snipped)
        .collect();
    candidates.sort_by_key(|t| preset(t));
    candidates
}

/// Download the best transcoding the account may play. A refused one
/// (e.g. `aac_256k` without Go+) falls through to the next; DRM only when
/// there is no plain stream at all.
pub async fn fetch(
    api: &Api,
    http: &reqwest::Client,
    token: &str,
    track: &Track,
) -> Result<Audio, StreamError> {
    let plain = ranked(&track.media);
    let drm = plain.is_empty();
    let candidates = if drm { ranked_drm(&track.media) } else { plain };
    let mut last = StreamError::Unavailable;
    for transcoding in candidates {
        match fetch_one(api, http, token, track, transcoding, drm).await {
            Ok(payload) => {
                return Ok(Audio {
                    payload,
                    preview: transcoding.snipped,
                    from_cache: false,
                });
            }
            Err(error @ (StreamError::Expired | StreamError::Unavailable)) => {
                diagnostics::log(
                    "INFO",
                    format!(
                        "[player] track {} {}: {error}",
                        track.id, transcoding.preset
                    ),
                );
                if !matches!(last, StreamError::Expired) {
                    last = error;
                }
            }
            Err(error) => return Err(error),
        }
    }
    Err(last)
}

async fn fetch_one(
    api: &Api,
    http: &reqwest::Client,
    token: &str,
    track: &Track,
    transcoding: &Transcoding,
    drm: bool,
) -> Result<Payload, StreamError> {
    let stream = api
        .stream_url(
            http,
            token,
            &transcoding.url,
            track.track_authorization.as_deref(),
        )
        .await?;
    match (drm, stream.license_auth_token) {
        (false, None) => Ok(Payload::Plain(hls::download(http, &stream.url).await?)),
        (true, Some(license_token)) => {
            let stream = hls::download_segments(http, &stream.url, true).await?;
            Ok(Payload::Drm(DrmMedia {
                init: stream.init,
                segments: stream.segments,
                durations: stream.durations,
                license_token,
            }))
        }
        // A plain stream with a licence token, or DRM without one.
        _ => Err(StreamError::Unavailable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(json: &str) -> Track {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn captured_tracks_try_the_best_aac_preset_first() {
        let tracks: Vec<Track> =
            serde_json::from_str(include_str!("../api/fixtures/tracks.json")).unwrap();
        for track in &tracks {
            let offered: Vec<&str> = track
                .media
                .transcodings
                .iter()
                .map(|t| t.preset.as_str())
                .collect();
            let expected: Vec<&str> = PRESETS
                .iter()
                .copied()
                .filter(|p| offered.contains(p))
                .collect();
            let order: Vec<&str> = ranked(&track.media)
                .iter()
                .map(|t| t.preset.as_str())
                .collect();
            assert_eq!(order, expected, "track {}", track.id);
            assert!(
                ranked(&track.media)
                    .iter()
                    .all(|t| t.url.ends_with("/stream/hls"))
            );
        }
    }

    #[test]
    fn drm_only_track_falls_back_to_cenc_best_first() {
        let drm = track(include_str!("../api/fixtures/track_drm.json"));
        assert!(ranked(&drm.media).is_empty());
        let order: Vec<&str> = ranked_drm(&drm.media)
            .iter()
            .map(|t| t.preset.as_str())
            .collect();
        assert_eq!(order, ["aac_256k", "aac_160k", "aac_96k"]);
        assert!(
            ranked_drm(&drm.media)
                .iter()
                .all(|t| t.url.ends_with("/stream/ctr-encrypted-hls"))
        );
    }

    fn transcoding(preset: &str, protocol: &str, mime: &str, snipped: bool) -> serde_json::Value {
        serde_json::json!({
            "url": format!("https://api-v2.soundcloud.com/media/x/{preset}/stream/hls"),
            "preset": preset,
            "snipped": snipped,
            "format": { "protocol": protocol, "mime_type": mime },
        })
    }

    fn media(transcodings: Vec<serde_json::Value>) -> Media {
        serde_json::from_value(serde_json::json!({ "transcodings": transcodings })).unwrap()
    }

    #[test]
    fn full_streams_beat_previews_and_masters_are_skipped() {
        let mp4 = "audio/mp4; codecs=\"mp4a.40.2\"";
        let m = media(vec![
            transcoding("abr_sq", "hls", "audio/mpegurl", false),
            transcoding("aac_160k", "hls", mp4, true),
            transcoding("aac_96k", "hls", mp4, false),
            transcoding("aac_160k", "ctr-encrypted-hls", mp4, false),
        ]);
        let picked = ranked(&m)[0];
        assert_eq!((picked.preset.as_str(), picked.snipped), ("aac_96k", false));

        let previews = media(vec![transcoding("aac_160k", "hls", mp4, true)]);
        assert!(ranked(&previews)[0].snipped);

        let masters_only = media(vec![transcoding("abr_hq", "hls", "audio/mpegurl", false)]);
        assert!(ranked(&masters_only).is_empty());
    }

    #[test]
    fn stale_signatures_map_to_expired() {
        assert!(matches!(
            StreamError::from(ApiError::Http(403)),
            StreamError::Expired
        ));
        assert!(matches!(
            StreamError::from(HlsError::Gone),
            StreamError::Expired
        ));
        assert!(matches!(
            StreamError::from(ApiError::Http(500)),
            StreamError::Network(_)
        ));
        assert!(matches!(
            StreamError::from(ApiError::Unauthorized),
            StreamError::Unauthorized
        ));
    }
}
