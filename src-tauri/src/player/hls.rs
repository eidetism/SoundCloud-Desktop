//! HLS download: fetch a media playlist, then its init segment and every
//! media segment. A plain stream is concatenated into one fragmented MP4 that
//! the audio engine decodes and seeks like a file. A DRM stream (CENC, keys
//! in `EXT-X-KEY`) keeps its segments apart with their durations: it is
//! handed, still encrypted, to the WebView player (`player::drm`). Ported
//! from the donor's `track_cache/sc_anon/hls.rs`, on the shared client and
//! with every URL checked against the SoundCloud media hosts.

use std::time::Duration;

use reqwest::{StatusCode, Url};
use tokio::task::JoinHandle;

use crate::shared::hosts;

/// Segments fetched at once.
const PARALLEL_SEGMENTS: usize = 4;
const SEGMENT_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug)]
pub enum HlsError {
    /// 401/403/404/410: the signed URL expired or the stream was withdrawn.
    Gone,
    /// Not playable the way it was asked for (encrypted where plain was
    /// expected or the reverse, master playlist, foreign host…).
    Unsupported(&'static str),
    Network(String),
}

#[derive(Debug, PartialEq)]
pub struct MediaPlaylist {
    pub init: Option<Url>,
    pub segments: Vec<Url>,
    /// `EXTINF` of each segment, seconds.
    pub durations: Vec<f64>,
}

/// A downloaded stream with its segments apart (DRM playback seeks by them).
#[derive(Debug)]
pub struct Segmented {
    pub init: Vec<u8>,
    pub segments: Vec<Vec<u8>>,
    pub durations: Vec<f64>,
}

/// Parse a media playlist. Relative URIs resolve against `base`; every URI
/// must land on a SoundCloud media host. Keys (`EXT-X-KEY`) are refused
/// unless `encrypted`, and then required.
pub fn parse(text: &str, base: &Url, encrypted: bool) -> Result<MediaPlaylist, HlsError> {
    if !text.trim_start().starts_with("#EXTM3U") {
        return Err(HlsError::Unsupported("not an HLS playlist"));
    }
    let resolve = |uri: &str| {
        base.join(uri)
            .ok()
            .filter(hosts::is_soundcloud_media)
            .ok_or(HlsError::Unsupported(
                "segment outside SoundCloud media hosts",
            ))
    };
    let mut init = None;
    let mut segments = Vec::new();
    let mut durations = Vec::new();
    let mut duration = 0.0;
    let mut keyed = false;
    for line in text.lines().map(str::trim) {
        if let Some(attributes) = line.strip_prefix("#EXT-X-MAP:") {
            let uri = quoted_attribute(attributes, "URI")
                .ok_or(HlsError::Unsupported("EXT-X-MAP without URI"))?;
            init = Some(resolve(uri)?);
        } else if line.starts_with("#EXT-X-KEY:") && !line.contains("METHOD=NONE") {
            if !encrypted {
                return Err(HlsError::Unsupported("encrypted HLS"));
            }
            keyed = true;
        } else if let Some(info) = line.strip_prefix("#EXTINF:") {
            duration = info
                .split(',')
                .next()
                .and_then(|value| value.trim().parse().ok())
                .unwrap_or(0.0);
        } else if line.starts_with("#EXT-X-STREAM-INF") {
            return Err(HlsError::Unsupported("master playlist"));
        } else if !line.is_empty() && !line.starts_with('#') {
            segments.push(resolve(line)?);
            durations.push(duration);
            duration = 0.0;
        }
    }
    if segments.is_empty() {
        return Err(HlsError::Unsupported("playlist without segments"));
    }
    if encrypted && (!keyed || init.is_none()) {
        return Err(HlsError::Unsupported("DRM playlist without keys or init"));
    }
    Ok(MediaPlaylist {
        init,
        segments,
        durations,
    })
}

fn quoted_attribute<'a>(attributes: &'a str, name: &str) -> Option<&'a str> {
    let start = attributes.find(&format!("{name}=\""))? + name.len() + 2;
    let rest = &attributes[start..];
    rest.find('"').map(|end| &rest[..end])
}

/// Download a plain stream behind `playlist_url` as one buffer.
pub async fn download(http: &reqwest::Client, playlist_url: &str) -> Result<Vec<u8>, HlsError> {
    let stream = download_segments(http, playlist_url, false).await?;
    let mut audio = stream.init;
    for segment in stream.segments {
        audio.extend_from_slice(&segment);
    }
    Ok(audio)
}

/// Download a stream keeping its segments apart. `encrypted` must match the
/// stream: CENC sample entries (`enca`) in the init segment mean DRM.
pub async fn download_segments(
    http: &reqwest::Client,
    playlist_url: &str,
    encrypted: bool,
) -> Result<Segmented, HlsError> {
    let base =
        hosts::media_url(playlist_url).map_err(|_| HlsError::Unsupported("playlist host"))?;
    let text = fetch(http, base.clone()).await?;
    let playlist = parse(&String::from_utf8_lossy(&text), &base, encrypted)?;

    let init = match playlist.init {
        Some(init) => fetch(http, init).await?,
        None => Vec::new(),
    };
    if init.windows(4).any(|w| w == b"enca") != encrypted {
        return Err(HlsError::Unsupported(if encrypted {
            "DRM stream with a plain init segment"
        } else {
            "encrypted init segment"
        }));
    }

    let mut segments = Vec::with_capacity(playlist.segments.len());
    let mut pending = playlist.segments.into_iter();
    let mut in_flight: Vec<JoinHandle<Result<Vec<u8>, HlsError>>> = Vec::new();
    let spawn = |url: Url| {
        let http = http.clone();
        tokio::spawn(async move { fetch(&http, url).await })
    };
    in_flight.extend(pending.by_ref().take(PARALLEL_SEGMENTS).map(spawn));
    while !in_flight.is_empty() {
        let segment = in_flight.remove(0);
        let bytes = match segment.await {
            Ok(result) => result,
            Err(join) => Err(HlsError::Network(join.to_string())),
        };
        match bytes {
            Ok(bytes) => segments.push(bytes),
            Err(error) => {
                in_flight.iter().for_each(JoinHandle::abort);
                return Err(error);
            }
        }
        in_flight.extend(pending.next().map(spawn));
    }
    Ok(Segmented {
        init,
        segments,
        durations: playlist.durations,
    })
}

async fn fetch(http: &reqwest::Client, url: Url) -> Result<Vec<u8>, HlsError> {
    let response = http
        .get(url)
        .timeout(SEGMENT_TIMEOUT)
        .send()
        .await
        .map_err(|e| HlsError::Network(e.without_url().to_string()))?;
    match response.status() {
        status if status.is_success() => response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|e| HlsError::Network(e.without_url().to_string())),
        StatusCode::UNAUTHORIZED
        | StatusCode::FORBIDDEN
        | StatusCode::NOT_FOUND
        | StatusCode::GONE => Err(HlsError::Gone),
        status => Err(HlsError::Network(format!("HTTP {status}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://playback.media-streaming.soundcloud.cloud/u0/aac_160k/abc/playlist.m3u8?expires=1").unwrap()
    }

    #[test]
    fn captured_playlist_parses_into_init_and_segments() {
        let playlist = parse(
            include_str!("../api/fixtures/playlist.m3u8"),
            &base(),
            false,
        )
        .unwrap();
        let init = playlist.init.unwrap();
        assert!(init.path().ends_with("/init.mp4"));
        assert_eq!(playlist.segments.len(), 2);
        assert!(playlist.segments[0].path().ends_with("/data000.m4s"));
        assert!(playlist.segments[1].path().ends_with("/data001.m4s"));
    }

    #[test]
    fn relative_uris_resolve_against_the_playlist() {
        let text = "#EXTM3U\n#EXT-X-MAP:URI=\"init.mp4\"\n#EXTINF:10,\ndata000.m4s?sig=1\n#EXT-X-ENDLIST\n";
        let playlist = parse(text, &base(), false).unwrap();
        assert_eq!(
            playlist.segments[0].as_str(),
            "https://playback.media-streaming.soundcloud.cloud/u0/aac_160k/abc/data000.m4s?sig=1"
        );
        assert!(playlist.init.unwrap().as_str().ends_with("/abc/init.mp4"));
    }

    #[test]
    fn encrypted_master_and_foreign_playlists_are_refused() {
        let encrypted =
            "#EXTM3U\n#EXT-X-KEY:METHOD=SAMPLE-AES,URI=\"skd://x\"\n#EXTINF:10,\ndata000.m4s\n";
        assert!(matches!(
            parse(encrypted, &base(), false),
            Err(HlsError::Unsupported(_))
        ));
        let master = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1\nlow.m3u8\n";
        assert!(matches!(
            parse(master, &base(), false),
            Err(HlsError::Unsupported(_))
        ));
        let foreign = "#EXTM3U\n#EXTINF:10,\nhttps://example.com/data000.m4s\n";
        assert!(matches!(
            parse(foreign, &base(), false),
            Err(HlsError::Unsupported(_))
        ));
        assert!(matches!(
            parse("<html>", &base(), false),
            Err(HlsError::Unsupported(_))
        ));
        assert!(matches!(
            parse("#EXTM3U\n#EXT-X-ENDLIST\n", &base(), false),
            Err(HlsError::Unsupported(_))
        ));
    }

    #[test]
    fn captured_drm_playlist_parses_with_durations() {
        let text = include_str!("../api/fixtures/playlist_drm.m3u8");
        assert!(matches!(
            parse(text, &base(), false),
            Err(HlsError::Unsupported(_))
        ));
        let playlist = parse(text, &base(), true).unwrap();
        assert!(playlist.init.unwrap().path().ends_with("/init.mp4"));
        assert_eq!(playlist.segments.len(), 3);
        assert_eq!(playlist.durations.len(), 3);
        assert!((playlist.durations[0] - 10.008).abs() < 1e-9);
    }

    #[test]
    fn drm_mode_requires_keys() {
        let plain = "#EXTM3U\n#EXT-X-MAP:URI=\"init.mp4\"\n#EXTINF:10,\ndata000.m4s\n";
        assert!(matches!(
            parse(plain, &base(), true),
            Err(HlsError::Unsupported(_))
        ));
    }
}
