//! Hosts the app may fetch from, outside api-v2: audio (HLS playlists and
//! segments), artwork, the licence server and the site's GraphQL endpoint. Every URL handed to us by an API answer or a
//! playlist is checked here first, so a bad answer can't point the shared
//! client anywhere else.

use reqwest::Url;

/// HLS playlists and segments, seen in the HAR captures.
const HLS_HOST: &str = "playback.media-streaming.soundcloud.cloud";
/// Widevine licence server for DRM tracks (per the DRM HAR).
const LICENSE_ENDPOINT: &str = "https://license.media-streaming.soundcloud.cloud/playback/widevine";
/// The site's GraphQL endpoint (track comments, per the track page HAR).
const GRAPHQL_ENDPOINT: &str = "https://graph.soundcloud.com/graphql";
/// Artwork, avatars and waveforms (`i1.`, `al.`, `wave.sndcdn.com`).
const CDN_SUFFIX: &str = ".sndcdn.com";

pub fn is_soundcloud_media(url: &Url) -> bool {
    url.scheme() == "https"
        && url.port().is_none()
        && url
            .host_str()
            .is_some_and(|host| host == HLS_HOST || host.ends_with(CDN_SUFFIX))
}

/// The licence server URL for a stream's `licenseAuthToken`.
pub fn license_url(token: &str) -> Url {
    let mut url = Url::parse(LICENSE_ENDPOINT).expect("valid licence URL");
    url.query_pairs_mut().append_pair("license_token", token);
    url
}

/// The GraphQL endpoint; the caller appends `client_id`.
pub fn graphql_url() -> Url {
    Url::parse(GRAPHQL_ENDPOINT).expect("valid GraphQL URL")
}

/// Parse `raw` and accept it only if it is a SoundCloud media URL.
pub fn media_url(raw: &str) -> Result<Url, String> {
    Url::parse(raw)
        .ok()
        .filter(is_soundcloud_media)
        .ok_or_else(|| "refused a non-SoundCloud media URL".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soundcloud_media_hosts_pass() {
        for raw in [
            "https://playback.media-streaming.soundcloud.cloud/x/aac_160k/y/playlist.m3u8?expires=1",
            "https://i1.sndcdn.com/artworks-abc-t500x500.jpg",
            "https://al.sndcdn.com/labs-e0ea3a45-0-t500x500.jpg?q=1",
            "https://wave.sndcdn.com/abWpAL6hgdUy_m.json",
        ] {
            assert!(media_url(raw).is_ok(), "{raw}");
        }
    }

    #[test]
    fn license_url_carries_the_token_encoded() {
        let url = license_url("a.b+c/d");
        assert_eq!(
            url.host_str(),
            Some("license.media-streaming.soundcloud.cloud")
        );
        assert_eq!(url.path(), "/playback/widevine");
        assert_eq!(
            url.query_pairs()
                .next()
                .map(|(k, v)| (k.into_owned(), v.into_owned())),
            Some(("license_token".into(), "a.b+c/d".into()))
        );
    }

    #[test]
    fn everything_else_is_refused() {
        for raw in [
            "http://i1.sndcdn.com/a.jpg",
            "https://sndcdn.com.evil.example/a.jpg",
            "https://evilsndcdn.com/a.jpg",
            "https://i1.sndcdn.com:444/a.jpg",
            "https://api-v2.soundcloud.com/me",
            "https://example.com/playlist.m3u8",
            "file:///C:/Windows/win.ini",
        ] {
            assert!(media_url(raw).is_err(), "{raw}");
        }
    }
}
