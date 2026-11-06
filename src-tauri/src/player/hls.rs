//! HLS download: fetch a media playlist, then its init segment and every
//! media segment, and concatenate them into one fragmented MP4 that the audio
//! engine decodes and seeks like a file. Ported from the donor's
//! `track_cache/sc_anon/hls.rs`, on the shared client and with every URL
//! checked against the SoundCloud media hosts.

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
    /// Encrypted (`EXT-X-KEY`) or otherwise not playable as plain fMP4.
    Unsupported(&'static str),
    Network(String),
}

#[derive(Debug, PartialEq)]
pub struct MediaPlaylist {
    pub init: Option<Url>,
    pub segments: Vec<Url>,
}

/// Parse a media playlist. Relative URIs resolve against `base`; every URI
/// must land on a SoundCloud media host.
pub fn parse(text: &str, base: &Url) -> Result<MediaPlaylist, HlsError> {
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
    for line in text.lines().map(str::trim) {
        if let Some(attributes) = line.strip_prefix("#EXT-X-MAP:") {
            let uri = quoted_attribute(attributes, "URI")
                .ok_or(HlsError::Unsupported("EXT-X-MAP without URI"))?;
            init = Some(resolve(uri)?);
        } else if line.starts_with("#EXT-X-KEY:") && !line.contains("METHOD=NONE") {
            return Err(HlsError::Unsupported("encrypted HLS"));
        } else if line.starts_with("#EXT-X-STREAM-INF") {
            return Err(HlsError::Unsupported("master playlist"));
        } else if !line.is_empty() && !line.starts_with('#') {
            segments.push(resolve(line)?);
        }
    }
    if segments.is_empty() {
        return Err(HlsError::Unsupported("playlist without segments"));
    }
    Ok(MediaPlaylist { init, segments })
}

fn quoted_attribute<'a>(attributes: &'a str, name: &str) -> Option<&'a str> {
    let start = attributes.find(&format!("{name}=\""))? + name.len() + 2;
    let rest = &attributes[start..];
    rest.find('"').map(|end| &rest[..end])
}

/// Download the whole stream behind `playlist_url` as one buffer.
pub async fn download(http: &reqwest::Client, playlist_url: &str) -> Result<Vec<u8>, HlsError> {
    let base =
        hosts::media_url(playlist_url).map_err(|_| HlsError::Unsupported("playlist host"))?;
    let text = fetch(http, base.clone()).await?;
    let playlist = parse(&String::from_utf8_lossy(&text), &base)?;

    let mut audio = match playlist.init {
        Some(init) => {
            let init = fetch(http, init).await?;
            // `enca`/`encv` sample entries mean CENC: DRM, not ours to play.
            if init.windows(4).any(|w| w == b"enca") {
                return Err(HlsError::Unsupported("encrypted init segment"));
            }
            init
        }
        None => Vec::new(),
    };

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
            Ok(bytes) => audio.extend_from_slice(&bytes),
            Err(error) => {
                in_flight.iter().for_each(JoinHandle::abort);
                return Err(error);
            }
        }
        in_flight.extend(pending.next().map(spawn));
    }
    Ok(audio)
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
        let playlist = parse(include_str!("../api/fixtures/playlist.m3u8"), &base()).unwrap();
        let init = playlist.init.unwrap();
        assert!(init.path().ends_with("/init.mp4"));
        assert_eq!(playlist.segments.len(), 2);
        assert!(playlist.segments[0].path().ends_with("/data000.m4s"));
        assert!(playlist.segments[1].path().ends_with("/data001.m4s"));
    }

    #[test]
    fn relative_uris_resolve_against_the_playlist() {
        let text = "#EXTM3U\n#EXT-X-MAP:URI=\"init.mp4\"\n#EXTINF:10,\ndata000.m4s?sig=1\n#EXT-X-ENDLIST\n";
        let playlist = parse(text, &base()).unwrap();
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
            parse(encrypted, &base()),
            Err(HlsError::Unsupported(_))
        ));
        let master = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1\nlow.m3u8\n";
        assert!(matches!(
            parse(master, &base()),
            Err(HlsError::Unsupported(_))
        ));
        let foreign = "#EXTM3U\n#EXTINF:10,\nhttps://example.com/data000.m4s\n";
        assert!(matches!(
            parse(foreign, &base()),
            Err(HlsError::Unsupported(_))
        ));
        assert!(matches!(
            parse("<html>", &base()),
            Err(HlsError::Unsupported(_))
        ));
        assert!(matches!(
            parse("#EXTM3U\n#EXT-X-ENDLIST\n", &base()),
            Err(HlsError::Unsupported(_))
        ));
    }
}
