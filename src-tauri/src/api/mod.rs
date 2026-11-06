//! SoundCloud api-v2 client. Requests carry `client_id` (scraped from the web
//! page, refreshed on 401/403) and, when signed in, `Authorization: OAuth`.
//! All traffic goes through the shared client from `crate::net`. Endpoints and
//! parameters follow the HAR captures in `docs/har/`.

mod client_id;
pub mod commands;
pub mod models;

use std::fmt;
use std::time::Duration;

use reqwest::{Method, StatusCode, Url};
use serde::de::DeserializeOwned;
use tokio::sync::{Mutex, RwLock};

use models::{
    ArtistShortcut, Comment, FeedItem, Me, Page, PlayHistoryItem, Playlist, SearchPage, Selection,
    StreamUrl, Track, TrackLike, UserProfile,
};

const API_HOST: &str = "api-v2.soundcloud.com";
/// 200 ids a page: enough for 20 000 likes.
const LIKED_IDS_MAX_PAGES: usize = 100;
const API_BASE: &str = "https://api-v2.soundcloud.com";
const WEB_BASE: &str = "https://soundcloud.com";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
/// Delays before retrying a 429 / 5xx answer.
const RETRY_DELAYS: [Duration; 2] = [Duration::from_millis(500), Duration::from_millis(1500)];
const MAX_RETRY_AFTER: Duration = Duration::from_secs(10);
/// `/tracks?ids=` takes at most this many ids per call.
pub const TRACKS_PER_LOOKUP: usize = 50;

#[derive(Debug)]
pub enum ApiError {
    /// 401 even with a fresh `client_id`: the OAuth token is invalid.
    Unauthorized,
    Http(u16),
    Network(String),
    Decode(String),
    ClientId(String),
    /// A cursor or media URL that doesn't point at api-v2.
    BadUrl,
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::Unauthorized => write!(f, "unauthorized"),
            ApiError::Http(status) => write!(f, "HTTP {status}"),
            ApiError::Network(message) => write!(f, "network: {message}"),
            ApiError::Decode(message) => write!(f, "decode: {message}"),
            ApiError::ClientId(message) => write!(f, "client_id: {message}"),
            ApiError::BadUrl => write!(f, "not an api-v2 URL"),
        }
    }
}

/// Where a list request starts: the first page, or a `next_href` cursor.
pub enum PageAt<'a> {
    First,
    Cursor(&'a str),
}

#[derive(Default)]
pub struct Api {
    client_id: RwLock<Option<String>>,
    refresh: Mutex<()>,
}

impl Api {
    pub async fn me(&self, http: &reqwest::Client, token: &str) -> Result<Me, ApiError> {
        self.get(http, api_url("/me", &[]), Some(token)).await
    }

    pub async fn feed(
        &self,
        http: &reqwest::Client,
        token: &str,
        at: PageAt<'_>,
    ) -> Result<Page<FeedItem>, ApiError> {
        let url = page_url(
            at,
            "/stream",
            &[
                ("activityTypes", "TrackPost,TrackRepost,PlaylistPost"),
                ("limit", "20"),
            ],
        )?;
        self.get(http, url, Some(token)).await
    }

    pub async fn track_likes(
        &self,
        http: &reqwest::Client,
        token: &str,
        user_id: u64,
        at: PageAt<'_>,
    ) -> Result<Page<TrackLike>, ApiError> {
        let path = format!("/users/{user_id}/track_likes");
        let url = page_url(at, &path, &[("limit", "24")])?;
        self.get(http, url, Some(token)).await
    }

    pub async fn play_history(
        &self,
        http: &reqwest::Client,
        token: &str,
        at: PageAt<'_>,
    ) -> Result<Page<PlayHistoryItem>, ApiError> {
        let url = page_url(at, "/me/play-history/tracks", &[("limit", "25")])?;
        self.get(http, url, Some(token)).await
    }

    /// The `/discover` shelves.
    pub async fn discover(
        &self,
        http: &reqwest::Client,
        token: &str,
        at: PageAt<'_>,
    ) -> Result<Page<Selection>, ApiError> {
        let url = page_url(at, "/mixed-selections", &[("limit", "10")])?;
        self.get(http, url, Some(token)).await
    }

    pub async fn search(
        &self,
        http: &reqwest::Client,
        token: &str,
        query: &str,
        at: PageAt<'_>,
    ) -> Result<SearchPage, ApiError> {
        let url = page_url(
            at,
            "/search",
            &[("q", query), ("facet", "model"), ("limit", "20")],
        )?;
        self.get(http, url, Some(token)).await
    }

    /// A playlist with its full track list (the tail as `{id}` stubs).
    pub async fn playlist(
        &self,
        http: &reqwest::Client,
        token: &str,
        playlist_id: u64,
    ) -> Result<Playlist, ApiError> {
        let url = api_url(
            &format!("/playlists/{playlist_id}"),
            &[("representation", "full")],
        );
        self.get(http, url, Some(token)).await
    }

    /// Full tracks by id, at most [`TRACKS_PER_LOOKUP`] per call. Unknown or
    /// removed ids are simply missing from the answer.
    pub async fn tracks(
        &self,
        http: &reqwest::Client,
        token: &str,
        ids: &[u64],
    ) -> Result<Vec<Track>, ApiError> {
        let ids = ids.iter().map(u64::to_string).collect::<Vec<_>>().join(",");
        self.get(http, api_url("/tracks", &[("ids", &ids)]), Some(token))
            .await
    }

    /// Ids of every track the user liked, newest first: `/me/track_likes/ids`
    /// paged 200 at a time, as the web player loads it.
    pub async fn liked_track_ids(
        &self,
        http: &reqwest::Client,
        token: &str,
    ) -> Result<Vec<u64>, ApiError> {
        let mut ids = Vec::new();
        let mut next: Option<String> = None;
        for _ in 0..LIKED_IDS_MAX_PAGES {
            let at = next.as_deref().map_or(PageAt::First, PageAt::Cursor);
            let url = page_url(at, "/me/track_likes/ids", &[("limit", "200")])?;
            let page: Page<u64> = self.get(http, url, Some(token)).await?;
            let done = page.collection.is_empty();
            ids.extend(page.collection);
            next = page.next_href;
            if done || next.is_none() {
                break;
            }
        }
        Ok(ids)
    }

    /// An api-v2 URL with the current `client_id`, for a request sent from
    /// somewhere other than this client (the hidden site window).
    pub async fn signed_url(&self, http: &reqwest::Client, path: &str) -> Result<Url, ApiError> {
        let client_id = self.client_id(http).await?;
        let mut url = api_url(path, &[]);
        url.query_pairs_mut().append_pair("client_id", &client_id);
        Ok(url)
    }

    pub async fn user(
        &self,
        http: &reqwest::Client,
        token: &str,
        user_id: u64,
    ) -> Result<UserProfile, ApiError> {
        self.get(
            http,
            api_url(&format!("/users/{user_id}"), &[]),
            Some(token),
        )
        .await
    }

    /// An artist's uploads, newest first.
    pub async fn user_tracks(
        &self,
        http: &reqwest::Client,
        token: &str,
        user_id: u64,
        at: PageAt<'_>,
    ) -> Result<Page<Track>, ApiError> {
        let url = page_url(at, &format!("/users/{user_id}/tracks"), &[("limit", "20")])?;
        self.get(http, url, Some(token)).await
    }

    /// Followed artists, most recently active first.
    pub async fn artist_shortcuts(
        &self,
        http: &reqwest::Client,
        token: &str,
        at: PageAt<'_>,
    ) -> Result<Page<ArtistShortcut>, ApiError> {
        let url = page_url(at, "/me/artist-shortcuts", &[("limit", "50")])?;
        self.get(http, url, Some(token)).await
    }

    pub async fn comments(
        &self,
        http: &reqwest::Client,
        token: &str,
        track_id: u64,
        at: PageAt<'_>,
    ) -> Result<Page<Comment>, ApiError> {
        let path = format!("/tracks/{track_id}/comments");
        let url = page_url(at, &path, &[("threaded", "0"), ("limit", "50")])?;
        self.get(http, url, Some(token)).await
    }

    /// Exchange a transcoding URL for the signed media URL.
    pub async fn stream_url(
        &self,
        http: &reqwest::Client,
        token: &str,
        transcoding_url: &str,
        track_authorization: Option<&str>,
    ) -> Result<StreamUrl, ApiError> {
        let mut url = api_v2_url(transcoding_url)?;
        if let Some(auth) = track_authorization.filter(|a| !a.is_empty()) {
            url.query_pairs_mut()
                .append_pair("track_authorization", auth);
        }
        self.get(http, url, Some(token)).await
    }

    /// Record a play, as the web player does when playback starts; it feeds
    /// the account's history and recommendations.
    pub async fn record_play(
        &self,
        http: &reqwest::Client,
        token: &str,
        track_urn: &str,
    ) -> Result<(), ApiError> {
        let body = serde_json::json!({ "track_urn": track_urn });
        self.send(
            http,
            Method::POST,
            api_url("/me/play-history", &[]),
            Some(&body),
            Some(token),
        )
        .await
        .map(drop)
    }

    async fn client_id(&self, http: &reqwest::Client) -> Result<String, ApiError> {
        if let Some(id) = self.client_id.read().await.clone() {
            return Ok(id);
        }
        self.refresh_client_id(http, None).await
    }

    /// Re-scrape `client_id`. Concurrent callers coalesce: if another task
    /// already replaced `stale`, its result is reused.
    async fn refresh_client_id(
        &self,
        http: &reqwest::Client,
        stale: Option<&str>,
    ) -> Result<String, ApiError> {
        let _guard = self.refresh.lock().await;
        if let Some(current) = self.client_id.read().await.clone()
            && Some(current.as_str()) != stale
        {
            return Ok(current);
        }
        let html = http
            .get(WEB_BASE)
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?
            .text()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        let id = client_id::extract(&html)
            .ok_or_else(|| ApiError::ClientId("not found on soundcloud.com".into()))?;
        *self.client_id.write().await = Some(id.clone());
        Ok(id)
    }

    async fn get<T: DeserializeOwned>(
        &self,
        http: &reqwest::Client,
        url: Url,
        token: Option<&str>,
    ) -> Result<T, ApiError> {
        let body = self.send(http, Method::GET, url, None, token).await?;
        serde_json::from_slice(&body).map_err(|e| ApiError::Decode(e.to_string()))
    }

    async fn send(
        &self,
        http: &reqwest::Client,
        method: Method,
        url: Url,
        json: Option<&serde_json::Value>,
        token: Option<&str>,
    ) -> Result<Vec<u8>, ApiError> {
        let mut client_id = self.client_id(http).await?;
        let mut refreshed = false;
        let mut retries = RETRY_DELAYS.iter();
        loop {
            let mut target = url.clone();
            target
                .query_pairs_mut()
                .append_pair("client_id", &client_id);
            let mut request = http
                .request(method.clone(), target)
                .timeout(REQUEST_TIMEOUT);
            if let Some(token) = token {
                request = request.header("Authorization", format!("OAuth {token}"));
            }
            if let Some(json) = json {
                request = request.json(json);
            }
            let response = request
                .send()
                .await
                .map_err(|e| ApiError::Network(e.without_url().to_string()))?;
            let status = response.status();
            if status.is_success() {
                return response
                    .bytes()
                    .await
                    .map(|body| body.to_vec())
                    .map_err(|e| ApiError::Network(e.without_url().to_string()));
            }
            match status {
                // A stale client_id also answers 401/403: refresh it once first.
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN if !refreshed => {
                    refreshed = true;
                    client_id = self.refresh_client_id(http, Some(&client_id)).await?;
                }
                StatusCode::UNAUTHORIZED => return Err(ApiError::Unauthorized),
                StatusCode::TOO_MANY_REQUESTS => match retries.next() {
                    Some(delay) => {
                        tokio::time::sleep(retry_after(&response).unwrap_or(*delay)).await
                    }
                    None => return Err(ApiError::Http(status.as_u16())),
                },
                _ if status.is_server_error() => match retries.next() {
                    Some(delay) => tokio::time::sleep(*delay).await,
                    None => return Err(ApiError::Http(status.as_u16())),
                },
                _ => return Err(ApiError::Http(status.as_u16())),
            }
        }
    }
}

fn api_url(path: &str, params: &[(&str, &str)]) -> Url {
    let mut url = Url::parse(&format!("{API_BASE}{path}")).expect("static api-v2 path");
    if !params.is_empty() {
        url.query_pairs_mut().extend_pairs(params);
    }
    url
}

/// First page: `path` with `params` and `linked_partitioning=1` (which makes
/// api-v2 answer with `next_href`). Later pages: the cursor as given.
fn page_url(at: PageAt<'_>, path: &str, params: &[(&str, &str)]) -> Result<Url, ApiError> {
    match at {
        PageAt::First => {
            let mut url = api_url(path, params);
            url.query_pairs_mut()
                .append_pair("linked_partitioning", "1");
            Ok(url)
        }
        PageAt::Cursor(href) => api_v2_url(href),
    }
}

/// Accept a URL from an api-v2 answer (`next_href`, transcoding URL) only if
/// it points back at api-v2 over https. Any `client_id` in it is dropped: the
/// current one is appended on send.
fn api_v2_url(href: &str) -> Result<Url, ApiError> {
    let mut url = Url::parse(href).map_err(|_| ApiError::BadUrl)?;
    if url.scheme() != "https" || url.host_str() != Some(API_HOST) || url.port().is_some() {
        return Err(ApiError::BadUrl);
    }
    let kept: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(key, _)| key != "client_id")
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    url.set_query(None);
    if !kept.is_empty() {
        url.query_pairs_mut().extend_pairs(kept);
    }
    Ok(url)
}

/// `Retry-After` in seconds, capped so a hostile value can't stall the app.
fn retry_after(response: &reqwest::Response) -> Option<Duration> {
    let seconds = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?;
    Some(Duration::from_secs(seconds).min(MAX_RETRY_AFTER))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_page_asks_for_a_cursor() {
        let url = page_url(PageAt::First, "/me/play-history/tracks", &[("limit", "25")]).unwrap();
        assert_eq!(
            url.as_str(),
            "https://api-v2.soundcloud.com/me/play-history/tracks?limit=25&linked_partitioning=1"
        );
    }

    #[test]
    fn search_query_is_encoded() {
        let url = page_url(PageAt::First, "/search", &[("q", "a&b c")]).unwrap();
        assert_eq!(url.query(), Some("q=a%26b+c&linked_partitioning=1"));
    }

    #[test]
    fn cursor_from_a_capture_is_kept_as_is() {
        let href = "https://api-v2.soundcloud.com/users/1000001/track_likes?offset=2026-10-30T14%3A21%3A25.978Z%2Cuser-track-likes%2C000-1&limit=24";
        let url = page_url(PageAt::Cursor(href), "/ignored", &[]).unwrap();
        assert_eq!(url.as_str(), href);
    }

    #[test]
    fn cursor_drops_a_stale_client_id() {
        let url =
            api_v2_url("https://api-v2.soundcloud.com/stream?offset=9&client_id=OLD&limit=10")
                .unwrap();
        assert_eq!(url.query(), Some("offset=9&limit=10"));
    }

    #[test]
    fn foreign_urls_are_refused() {
        for href in [
            "https://evil.example/stream",
            "http://api-v2.soundcloud.com/stream",
            "https://api-v2.soundcloud.com.evil.example/stream",
            "https://api-v2.soundcloud.com:8443/stream",
            "not a url",
        ] {
            assert!(api_v2_url(href).is_err(), "{href}");
        }
    }

    #[test]
    fn stream_url_answers_decode() {
        let plain: StreamUrl =
            serde_json::from_str(include_str!("fixtures/stream_hls.json")).unwrap();
        assert!(plain.url.contains("/playlist.m3u8"));
        assert!(plain.license_auth_token.is_none());
        let drm: StreamUrl =
            serde_json::from_str(include_str!("fixtures/stream_encrypted.json")).unwrap();
        assert!(drm.license_auth_token.is_some());
    }
}
