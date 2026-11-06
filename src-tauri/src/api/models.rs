//! api-v2 response models, written against the HAR captures in `docs/har/`
//! (trimmed copies live in `fixtures/`). Only the fields the app uses; unknown
//! fields are ignored so additions on SoundCloud's side don't break decoding.
//!
//! Every model decodes from api-v2's snake_case and serializes to the
//! frontend in camelCase. Stream internals (`media`, `track_authorization`)
//! never leave Rust: a track tells the frontend only its `access`.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The signed-in user (`GET /me`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct Me {
    pub id: u64,
    pub username: String,
    #[serde(default)]
    pub permalink: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub full_name: Option<String>,
}

/// A paged list. `next_href` is the opaque cursor for the following page.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
#[serde(bound(deserialize = "T: DeserializeOwned"))]
pub struct Page<T> {
    #[serde(deserialize_with = "lenient_vec")]
    pub collection: Vec<T>,
    #[serde(default)]
    pub next_href: Option<String>,
}

/// Decode a list item by item, dropping entries that don't fit the model, so
/// one odd item (a new kind, a missing field) doesn't empty a whole page.
fn lenient_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let raw = Option::<Vec<serde_json::Value>>::deserialize(deserializer)?.unwrap_or_default();
    Ok(raw
        .into_iter()
        .filter_map(|value| serde_json::from_value(value).ok())
        .collect())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct User {
    pub id: u64,
    pub username: String,
    #[serde(default)]
    pub permalink: String,
    #[serde(default)]
    pub permalink_url: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub verified: bool,
    #[serde(default)]
    pub followers_count: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct Track {
    pub id: u64,
    #[serde(default)]
    pub urn: String,
    pub title: String,
    #[serde(default)]
    pub user: Option<User>,
    #[serde(default)]
    pub artwork_url: Option<String>,
    /// Playable length in ms (30 s for a Go+ preview).
    #[serde(rename(serialize = "durationMs"))]
    pub duration: u64,
    #[serde(default)]
    pub permalink_url: String,
    #[serde(default)]
    pub waveform_url: Option<String>,
    #[serde(default)]
    pub genre: Option<String>,
    #[serde(default)]
    pub display_date: Option<String>,
    #[serde(default)]
    pub likes_count: Option<u64>,
    #[serde(default)]
    pub reposts_count: Option<u64>,
    #[serde(default)]
    pub playback_count: Option<u64>,
    #[serde(default)]
    pub comment_count: Option<u64>,
    /// Serialized as `access`: what the player can do with this track.
    #[serde(
        default,
        rename(serialize = "access"),
        serialize_with = "serialize_access"
    )]
    pub media: Media,
    #[serde(default, skip_serializing)]
    pub track_authorization: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Media {
    #[serde(default)]
    pub transcodings: Vec<Transcoding>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Transcoding {
    /// api-v2 URL that answers with the signed media URL.
    pub url: String,
    #[serde(default)]
    pub preset: String,
    #[serde(default)]
    pub snipped: bool,
    pub format: TranscodingFormat,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TranscodingFormat {
    pub protocol: String,
    #[serde(default)]
    pub mime_type: String,
}

impl Transcoding {
    /// DRM (`cbc-`/`ctr-encrypted-hls`): needs a licence server, never played.
    pub fn is_encrypted(&self) -> bool {
        self.format.protocol.contains("encrypted")
    }
}

/// Whether a track can be played, and how much of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Access {
    Full,
    /// Only the 30-second Go+ preview.
    Preview,
    /// Nothing playable: DRM-only, region-blocked or no transcodings.
    Blocked,
}

impl Media {
    pub fn access(&self) -> Access {
        let open = self.transcodings.iter().filter(|t| !t.is_encrypted());
        let mut access = Access::Blocked;
        for transcoding in open {
            if !transcoding.snipped {
                return Access::Full;
            }
            access = Access::Preview;
        }
        access
    }
}

fn serialize_access<S: Serializer>(media: &Media, serializer: S) -> Result<S::Ok, S::Error> {
    media.access().serialize(serializer)
}

/// A playlist's track: full for the first few, `{id}` stubs for the rest.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum TrackRef {
    Full(Box<Track>),
    Stub { id: u64 },
}

impl TrackRef {
    pub fn id(&self) -> u64 {
        match self {
            TrackRef::Full(track) => track.id,
            TrackRef::Stub { id } => *id,
        }
    }
}

/// Track lists go to the frontend as ids only; the player hydrates them.
fn serialize_track_ids<S: Serializer>(
    tracks: &[TrackRef],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(tracks.iter().map(TrackRef::id))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct Playlist {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub permalink_url: String,
    #[serde(default)]
    pub artwork_url: Option<String>,
    #[serde(default)]
    pub user: Option<User>,
    #[serde(default)]
    pub track_count: u64,
    /// Total length in ms.
    #[serde(default, rename(serialize = "durationMs"))]
    pub duration: u64,
    #[serde(default)]
    pub is_album: bool,
    #[serde(default)]
    pub likes_count: Option<u64>,
    #[serde(default)]
    pub display_date: Option<String>,
    /// Absent in list answers; present (mostly as stubs) with `representation=full`.
    #[serde(
        default,
        rename(serialize = "trackIds"),
        serialize_with = "serialize_track_ids"
    )]
    pub tracks: Vec<TrackRef>,
}

/// A generated playlist: Your Mix, Weekly Wave, Daily Drops, stations…
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct SystemPlaylist {
    /// A URN such as `soundcloud:system-playlists:your-moods:123:4`.
    pub urn: String,
    pub title: String,
    #[serde(default)]
    pub short_title: Option<String>,
    #[serde(default)]
    pub short_description: Option<String>,
    #[serde(default)]
    pub permalink_url: String,
    #[serde(default)]
    pub calculated_artwork_url: Option<String>,
    #[serde(default)]
    pub tracking_feature_name: Option<String>,
    #[serde(
        default,
        rename(serialize = "trackIds"),
        serialize_with = "serialize_track_ids"
    )]
    pub tracks: Vec<TrackRef>,
}

/// A `/discover` shelf ("Mixed for you", "Made for you", …).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct Selection {
    pub urn: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub tracking_feature_name: Option<String>,
    pub items: Page<SelectionItem>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SelectionItem {
    SystemPlaylist(SystemPlaylist),
    Playlist(Playlist),
    User(User),
}

/// An entry of the following feed (`GET /stream`).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct FeedItem {
    /// `track`, `track-repost`, `playlist` or `playlist-repost`.
    #[serde(rename = "type")]
    pub kind: String,
    pub created_at: String,
    /// Who posted or reposted it.
    pub user: User,
    #[serde(default)]
    pub caption: Option<String>,
    #[serde(default)]
    pub track: Option<Track>,
    #[serde(default)]
    pub playlist: Option<Playlist>,
}

/// `GET /users/{id}/track_likes`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct TrackLike {
    pub created_at: String,
    pub track: Track,
}

/// `GET /me/play-history/tracks`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct PlayHistoryItem {
    /// Unix time in ms.
    pub played_at: u64,
    pub track: Track,
}

/// `GET /search` answers with mixed kinds.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SearchItem {
    Track(Track),
    Playlist(Playlist),
    User(User),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct SearchPage {
    #[serde(flatten)]
    pub page: Page<SearchItem>,
    #[serde(default)]
    pub total_results: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct Comment {
    pub id: u64,
    pub body: String,
    pub created_at: String,
    /// Position in the track, ms.
    #[serde(default)]
    pub timestamp: Option<u64>,
    pub user: User,
}

/// The signed media URL behind a transcoding.
#[derive(Debug, Deserialize)]
pub struct StreamUrl {
    pub url: String,
    /// Present only on DRM streams.
    #[serde(default, rename = "licenseAuthToken")]
    pub license_auth_token: Option<String>,
}

/// Full tracks carried by a response, for the player's track store.
pub trait CarriesTracks {
    fn for_each_track(&self, f: &mut dyn FnMut(&Track));
}

impl CarriesTracks for Track {
    fn for_each_track(&self, f: &mut dyn FnMut(&Track)) {
        f(self)
    }
}

impl CarriesTracks for TrackRef {
    fn for_each_track(&self, f: &mut dyn FnMut(&Track)) {
        if let TrackRef::Full(track) = self {
            f(track)
        }
    }
}

impl CarriesTracks for Playlist {
    fn for_each_track(&self, f: &mut dyn FnMut(&Track)) {
        self.tracks.iter().for_each(|t| t.for_each_track(f))
    }
}

impl CarriesTracks for SystemPlaylist {
    fn for_each_track(&self, f: &mut dyn FnMut(&Track)) {
        self.tracks.iter().for_each(|t| t.for_each_track(f))
    }
}

impl CarriesTracks for SelectionItem {
    fn for_each_track(&self, f: &mut dyn FnMut(&Track)) {
        match self {
            SelectionItem::SystemPlaylist(p) => p.for_each_track(f),
            SelectionItem::Playlist(p) => p.for_each_track(f),
            SelectionItem::User(_) => {}
        }
    }
}

impl CarriesTracks for Selection {
    fn for_each_track(&self, f: &mut dyn FnMut(&Track)) {
        self.items.for_each_track(f)
    }
}

impl CarriesTracks for FeedItem {
    fn for_each_track(&self, f: &mut dyn FnMut(&Track)) {
        self.track.iter().for_each(&mut *f);
        self.playlist.iter().for_each(|p| p.for_each_track(f));
    }
}

impl CarriesTracks for TrackLike {
    fn for_each_track(&self, f: &mut dyn FnMut(&Track)) {
        f(&self.track)
    }
}

impl CarriesTracks for PlayHistoryItem {
    fn for_each_track(&self, f: &mut dyn FnMut(&Track)) {
        f(&self.track)
    }
}

impl CarriesTracks for SearchItem {
    fn for_each_track(&self, f: &mut dyn FnMut(&Track)) {
        match self {
            SearchItem::Track(t) => f(t),
            SearchItem::Playlist(p) => p.for_each_track(f),
            SearchItem::User(_) => {}
        }
    }
}

impl CarriesTracks for SearchPage {
    fn for_each_track(&self, f: &mut dyn FnMut(&Track)) {
        self.page.for_each_track(f)
    }
}

/// An artist's profile as `/users/{id}` returns it.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct UserProfile {
    pub id: u64,
    pub username: String,
    #[serde(default)]
    pub permalink_url: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub full_name: Option<String>,
    #[serde(default)]
    pub city: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub verified: bool,
    #[serde(default)]
    pub followers_count: Option<u64>,
    #[serde(default)]
    pub followings_count: Option<u64>,
    #[serde(default)]
    pub track_count: Option<u64>,
    #[serde(default)]
    pub playlist_count: Option<u64>,
}

impl CarriesTracks for UserProfile {
    fn for_each_track(&self, _: &mut dyn FnMut(&Track)) {}
}

/// A followed artist as `/me/artist-shortcuts` lists it (the "following" row
/// of the web home), with whether they posted something unseen.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct ArtistShortcut {
    pub user: User,
    #[serde(default = "seen")]
    pub has_read: bool,
}

fn seen() -> bool {
    true
}

impl CarriesTracks for ArtistShortcut {
    fn for_each_track(&self, _: &mut dyn FnMut(&Track)) {}
}

impl CarriesTracks for Comment {
    fn for_each_track(&self, _: &mut dyn FnMut(&Track)) {}
}

impl<T: CarriesTracks> CarriesTracks for Page<T> {
    fn for_each_track(&self, f: &mut dyn FnMut(&Track)) {
        self.collection
            .iter()
            .for_each(|item| item.for_each_track(f))
    }
}

impl<T: CarriesTracks> CarriesTracks for Vec<T> {
    fn for_each_track(&self, f: &mut dyn FnMut(&Track)) {
        self.iter().for_each(|item| item.for_each_track(f))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture<T: DeserializeOwned>(json: &str) -> T {
        serde_json::from_str(json).expect("fixture decodes")
    }

    fn track_count(value: &impl CarriesTracks) -> usize {
        let mut count = 0;
        value.for_each_track(&mut |_| count += 1);
        count
    }

    #[test]
    fn me_decodes_and_ignores_unknown_fields() {
        let json = r#"{
            "avatar_url": "https://i1.sndcdn.com/avatars-000-large.jpg",
            "city": "Berlin",
            "followers_count": 12,
            "full_name": "",
            "id": 123456,
            "kind": "user",
            "permalink": "someone",
            "username": "Someone",
            "urn": "soundcloud:users:123456"
        }"#;
        let me: Me = serde_json::from_str(json).unwrap();
        assert_eq!(me.id, 123456);
        assert_eq!(me.username, "Someone");
        assert_eq!(me.permalink, "someone");
    }

    #[test]
    fn me_tolerates_missing_optional_fields() {
        let me: Me = serde_json::from_str(r#"{"id":1,"username":"u","avatar_url":null}"#).unwrap();
        assert_eq!(me.avatar_url, None);
        assert_eq!(me.permalink, "");
    }

    #[test]
    fn me_serializes_in_camel_case_for_the_frontend() {
        let me = Me {
            id: 1,
            username: "u".into(),
            permalink: "u".into(),
            avatar_url: None,
            full_name: None,
        };
        assert_eq!(
            serde_json::to_string(&me).unwrap(),
            r#"{"id":1,"username":"u","permalink":"u","avatarUrl":null,"fullName":null}"#
        );
    }

    #[test]
    fn feed_decodes_tracks_and_reposts() {
        let page: Page<FeedItem> = fixture(include_str!("fixtures/stream.json"));
        assert_eq!(page.collection.len(), 6);
        assert!(page.collection.iter().any(|i| i.kind == "track-repost"));
        assert!(
            page.collection
                .iter()
                .all(|i| i.track.is_some() || i.playlist.is_some())
        );
        assert!(
            page.next_href
                .unwrap()
                .starts_with("https://api-v2.soundcloud.com/stream?")
        );
    }

    #[test]
    fn likes_and_history_decode() {
        let likes: Page<TrackLike> = fixture(include_str!("fixtures/track_likes.json"));
        assert_eq!(likes.collection.len(), 3);
        assert!(likes.next_href.is_some());
        let history: Page<PlayHistoryItem> = fixture(include_str!("fixtures/play_history.json"));
        assert_eq!(history.collection.len(), 3);
        assert!(history.collection[0].played_at > 1_700_000_000_000);
    }

    #[test]
    fn discover_shelves_decode_every_item_kind() {
        let page: Page<Selection> = fixture(include_str!("fixtures/mixed_selections.json"));
        assert_eq!(page.collection.len(), 12);
        let items: Vec<&SelectionItem> = page
            .collection
            .iter()
            .flat_map(|s| &s.items.collection)
            .collect();
        assert!(
            items
                .iter()
                .any(|i| matches!(i, SelectionItem::SystemPlaylist(_)))
        );
        assert!(
            items
                .iter()
                .any(|i| matches!(i, SelectionItem::Playlist(_)))
        );
        assert!(items.iter().any(|i| matches!(i, SelectionItem::User(_))));
        let mix = page
            .collection
            .iter()
            .find(|s| s.tracking_feature_name.as_deref() == Some("your-moods"))
            .expect("the Your Mix shelf");
        let SelectionItem::SystemPlaylist(first) = &mix.items.collection[0] else {
            panic!("a mix is a system playlist");
        };
        assert!(
            first
                .urn
                .starts_with("soundcloud:system-playlists:your-moods:")
        );
        // Mixes carry stubs only: ids for the player to hydrate.
        assert_eq!(first.tracks.len(), 3);
        assert!(
            first
                .tracks
                .iter()
                .all(|t| matches!(t, TrackRef::Stub { .. }))
        );
    }

    #[test]
    fn search_decodes_mixed_kinds() {
        let page: SearchPage = fixture(include_str!("fixtures/search.json"));
        assert!(page.total_results > 0);
        let kinds: Vec<&str> = page
            .page
            .collection
            .iter()
            .map(|i| match i {
                SearchItem::Track(_) => "track",
                SearchItem::Playlist(_) => "playlist",
                SearchItem::User(_) => "user",
            })
            .collect();
        assert!(kinds.contains(&"track") && kinds.contains(&"playlist") && kinds.contains(&"user"));
        assert!(page.page.next_href.is_some());
    }

    #[test]
    fn full_playlist_mixes_tracks_and_stubs() {
        let playlist: Playlist = fixture(include_str!("fixtures/playlist.json"));
        assert_eq!(playlist.tracks.len(), 7);
        assert_eq!(track_count(&playlist), 5);
        let json = serde_json::to_value(&playlist).unwrap();
        let ids = json["trackIds"].as_array().unwrap();
        assert_eq!(ids.len(), 7);
        assert!(ids.iter().all(|id| id.is_u64()));
    }

    #[test]
    fn artist_shortcuts_decode() {
        let page: Page<ArtistShortcut> = fixture(include_str!("fixtures/artist_shortcuts.json"));
        assert_eq!(page.collection.len(), 4);
        assert!(page.collection.iter().all(|s| !s.user.username.is_empty()));
        assert!(page.collection.iter().any(|s| !s.has_read));
    }

    #[test]
    fn user_profile_decodes() {
        let user: UserProfile = fixture(include_str!("fixtures/user.json"));
        assert_eq!(user.username, "lucybedroque");
        assert_eq!(user.track_count, Some(146));
        assert!(user.verified);
    }

    #[test]
    fn liked_ids_decode() {
        let page: Page<u64> = fixture(include_str!("fixtures/track_like_ids.json"));
        assert_eq!(page.collection, [1934849951, 1839756210, 2251312517]);
        assert!(page.next_href.is_some());
    }

    #[test]
    fn tracks_by_ids_and_comments_decode() {
        let tracks: Vec<Track> = fixture(include_str!("fixtures/tracks.json"));
        assert!(!tracks.is_empty());
        assert!(tracks.iter().all(|t| t.duration > 0 && !t.urn.is_empty()));
        let comments: Page<Comment> = fixture(include_str!("fixtures/comments.json"));
        assert_eq!(comments.collection.len(), 3);
    }

    #[test]
    fn stream_internals_never_reach_the_frontend() {
        let tracks: Vec<Track> = fixture(include_str!("fixtures/tracks.json"));
        let json = serde_json::to_value(&tracks[0]).unwrap();
        assert_eq!(json["access"], "full");
        assert!(json.get("media").is_none());
        assert!(json.get("trackAuthorization").is_none());
        assert!(json.get("durationMs").unwrap().is_u64());
    }

    #[test]
    fn one_broken_item_does_not_empty_the_page() {
        let page: Page<User> = fixture(
            r#"{"collection":[{"id":1,"username":"a"},{"oops":true},{"id":2,"username":"b"}],"next_href":null}"#,
        );
        assert_eq!(page.collection.len(), 2);
    }

    fn media(transcodings: &[(&str, bool)]) -> Media {
        Media {
            transcodings: transcodings
                .iter()
                .map(|(protocol, snipped)| Transcoding {
                    url: String::new(),
                    preset: "aac_160k".into(),
                    snipped: *snipped,
                    format: TranscodingFormat {
                        protocol: (*protocol).into(),
                        mime_type: String::new(),
                    },
                })
                .collect(),
        }
    }

    #[test]
    fn access_ignores_drm_and_knows_previews() {
        assert_eq!(media(&[("hls", false)]).access(), Access::Full);
        assert_eq!(media(&[("hls", true)]).access(), Access::Preview);
        assert_eq!(
            media(&[("hls", true), ("hls", false)]).access(),
            Access::Full
        );
        assert_eq!(
            media(&[("ctr-encrypted-hls", false), ("cbc-encrypted-hls", false)]).access(),
            Access::Blocked
        );
        assert_eq!(media(&[]).access(), Access::Blocked);
    }

    #[test]
    fn a_drm_only_track_from_the_captures_is_blocked() {
        let track: Track = fixture(include_str!("fixtures/track_drm.json"));
        assert_eq!(track.media.transcodings.len(), 10);
        assert_eq!(track.media.access(), Access::Blocked);
    }
}
