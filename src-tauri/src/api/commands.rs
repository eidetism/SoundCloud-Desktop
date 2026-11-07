//! api-v2 commands for the frontend. Lists take an optional `cursor` (the
//! previous page's `nextHref`). Every full track in an answer is also kept
//! in the player's track store, so the frontend can play by id.

use serde_json::{Value, json};
use tauri::{Manager, State};

use crate::api::graph::{self, Comment, CommentPage};
use crate::api::models::{
    ArtistShortcut, CarriesTracks, FeedItem, LibraryItem, Page, PlayHistoryItem, Playlist,
    SearchPage, Selection, Track, TrackLike, User, UserProfile,
};
use crate::api::{Api, ApiError, PageAt, UserSection};
use crate::app::diagnostics;
use crate::auth::{AuthState, Session};
use crate::net::NetState;
use crate::player::PlayerState;
use crate::rt::AppHandle;

/// The shared client and the OAuth token, or an error if signed out.
fn session(app: &AppHandle) -> Result<(reqwest::Client, String), String> {
    let token = app
        .state::<AuthState>()
        .token()
        .ok_or_else(|| "signed out".to_string())?;
    Ok((app.state::<NetState>().client(), token))
}

/// Map an API error for the frontend. A 401 here doesn't sign out by
/// itself: it triggers a `/me` check, and only that check may.
fn fail(app: &AppHandle, error: ApiError) -> String {
    if matches!(error, ApiError::Unauthorized) {
        crate::auth::verify(app);
    }
    error.to_string()
}

fn keep<T: CarriesTracks>(app: &AppHandle, result: Result<T, ApiError>) -> Result<T, String> {
    let value = result.map_err(|error| fail(app, error))?;
    app.state::<PlayerState>().store.remember(&value);
    Ok(value)
}

fn at(cursor: &Option<String>) -> PageAt<'_> {
    cursor.as_deref().map_or(PageAt::First, PageAt::Cursor)
}

async fn my_id(
    app: &AppHandle,
    api: &Api,
    http: &reqwest::Client,
    token: &str,
) -> Result<u64, String> {
    if let Session::SignedIn { user: Some(me), .. } = app.state::<AuthState>().session() {
        return Ok(me.id);
    }
    api.me(http, token)
        .await
        .map(|me| me.id)
        .map_err(|e| fail(app, e))
}

#[tauri::command]
pub async fn api_feed(
    app: AppHandle,
    api: State<'_, Api>,
    cursor: Option<String>,
) -> Result<Page<FeedItem>, String> {
    let (http, token) = session(&app)?;
    keep(&app, api.feed(&http, &token, at(&cursor)).await)
}

#[tauri::command]
pub async fn api_likes(
    app: AppHandle,
    api: State<'_, Api>,
    cursor: Option<String>,
) -> Result<Page<TrackLike>, String> {
    let (http, token) = session(&app)?;
    let user_id = my_id(&app, &api, &http, &token).await?;
    keep(
        &app,
        api.track_likes(&http, &token, user_id, at(&cursor)).await,
    )
}

#[tauri::command]
pub async fn api_history(
    app: AppHandle,
    api: State<'_, Api>,
    cursor: Option<String>,
) -> Result<Page<PlayHistoryItem>, String> {
    let (http, token) = session(&app)?;
    keep(&app, api.play_history(&http, &token, at(&cursor)).await)
}

#[tauri::command]
pub async fn api_discover(
    app: AppHandle,
    api: State<'_, Api>,
    cursor: Option<String>,
) -> Result<Page<Selection>, String> {
    let (http, token) = session(&app)?;
    keep(&app, api.discover(&http, &token, at(&cursor)).await)
}

#[tauri::command]
pub async fn api_search(
    app: AppHandle,
    api: State<'_, Api>,
    query: String,
    cursor: Option<String>,
) -> Result<SearchPage, String> {
    let (http, token) = session(&app)?;
    keep(&app, api.search(&http, &token, &query, at(&cursor)).await)
}

/// A playlist; its `trackIds` resolve through `api_tracks`.
#[tauri::command]
pub async fn api_playlist(
    app: AppHandle,
    api: State<'_, Api>,
    id: u64,
    secret_token: Option<String>,
) -> Result<Playlist, String> {
    let (http, token) = session(&app)?;
    keep(
        &app,
        api.playlist(&http, &token, id, secret_token.as_deref())
            .await,
    )
}

#[tauri::command]
pub async fn api_library(
    app: AppHandle,
    api: State<'_, Api>,
    cursor: Option<String>,
) -> Result<Page<LibraryItem>, String> {
    let (http, token) = session(&app)?;
    keep(&app, api.library(&http, &token, at(&cursor)).await)
}

/// An artist's most played tracks.
#[tauri::command]
pub async fn api_user_top_tracks(
    app: AppHandle,
    api: State<'_, Api>,
    id: u64,
    cursor: Option<String>,
) -> Result<Page<Track>, String> {
    let (http, token) = session(&app)?;
    let page = api
        .user_section(&http, &token, id, UserSection::TopTracks, at(&cursor))
        .await;
    keep(&app, page)
}

/// An artist's albums (`albums: true`) or other playlists.
#[tauri::command]
pub async fn api_user_playlists(
    app: AppHandle,
    api: State<'_, Api>,
    id: u64,
    albums: bool,
    cursor: Option<String>,
) -> Result<Page<Playlist>, String> {
    let (http, token) = session(&app)?;
    let section = if albums {
        UserSection::Albums
    } else {
        UserSection::Playlists
    };
    let page = api
        .user_section(&http, &token, id, section, at(&cursor))
        .await;
    keep(&app, page)
}

/// Full tracks for `ids`, in order; ids SoundCloud no longer knows are skipped.
#[tauri::command]
pub async fn api_tracks(
    app: AppHandle,
    api: State<'_, Api>,
    ids: Vec<u64>,
) -> Result<Vec<Track>, String> {
    let (http, token) = session(&app)?;
    let store = &app.state::<PlayerState>().store;
    let tracks = store
        .resolve(&api, &http, &token, &ids)
        .await
        .map_err(|error| fail(&app, error))?;
    Ok(tracks.iter().map(|track| (**track).clone()).collect())
}

#[tauri::command]
pub async fn api_artist_shortcuts(
    app: AppHandle,
    api: State<'_, Api>,
    cursor: Option<String>,
) -> Result<Page<ArtistShortcut>, String> {
    let (http, token) = session(&app)?;
    keep(&app, api.artist_shortcuts(&http, &token, at(&cursor)).await)
}

/// Ids of all liked tracks, newest first.
#[tauri::command]
pub async fn api_liked_ids(app: AppHandle, api: State<'_, Api>) -> Result<Vec<u64>, String> {
    let (http, token) = session(&app)?;
    api.liked_track_ids(&http, &token)
        .await
        .map_err(|error| fail(&app, error))
}

#[tauri::command]
pub async fn api_user(app: AppHandle, api: State<'_, Api>, id: u64) -> Result<UserProfile, String> {
    let (http, token) = session(&app)?;
    keep(&app, api.user(&http, &token, id).await)
}

#[tauri::command]
pub async fn api_user_tracks(
    app: AppHandle,
    api: State<'_, Api>,
    id: u64,
    cursor: Option<String>,
) -> Result<Page<Track>, String> {
    let (http, token) = session(&app)?;
    keep(&app, api.user_tracks(&http, &token, id, at(&cursor)).await)
}

/// Send a DataDome-guarded write from the hidden soundcloud.com window. A
/// 401 triggers the `/me` check; any other non-2xx status is an error.
async fn site_write(
    app: &AppHandle,
    what: &str,
    method: &str,
    url: &str,
    token: &str,
    body: Option<&Value>,
) -> Result<crate::auth::SiteResponse, String> {
    let response = crate::auth::site_request(app, method, url, token, body).await?;
    if !(200..300).contains(&response.status) {
        if response.status == 401 {
            crate::auth::verify(app);
        }
        diagnostics::warn(format!("[api] {what} -> {}", response.status));
        return Err(format!("{what} failed: HTTP {}", response.status));
    }
    Ok(response)
}

/// A GraphQL mutation through the site window; returns its `data`.
async fn site_mutation(
    app: &AppHandle,
    api: &Api,
    operation: &str,
    document: &str,
    variables: Value,
) -> Result<Value, String> {
    let (http, token) = session(app)?;
    let url = api
        .signed_graphql_url(&http)
        .await
        .map_err(|error| fail(app, error))?;
    let body = graph::request(operation, document, variables);
    let response = site_write(app, operation, "POST", url.as_str(), &token, Some(&body)).await?;
    let answer = response
        .body
        .ok_or_else(|| format!("{operation}: answer is not JSON"))?;
    graph::data(answer)
        .inspect_err(|error| diagnostics::warn(format!("[api] {operation}: {error}")))
}

/// Like (`liked: true`) or unlike a track: `PUT`/`DELETE
/// /users/{me}/track_likes/{id}` (per HAR), from the site window.
#[tauri::command]
pub async fn api_set_like(
    app: AppHandle,
    api: State<'_, Api>,
    id: u64,
    liked: bool,
) -> Result<(), String> {
    let (http, token) = session(&app)?;
    let me = my_id(&app, &api, &http, &token).await?;
    let url = api
        .signed_url(&http, &format!("/users/{me}/track_likes/{id}"))
        .await
        .map_err(|error| fail(&app, error))?;
    let method = if liked { "PUT" } else { "DELETE" };
    match site_write(&app, "like", method, url.as_str(), &token, None).await {
        // Unliking a track that isn't liked.
        Err(error) if !liked && error.ends_with("HTTP 404") => Ok(()),
        other => other.map(drop),
    }
}

/// A page of a track's comments with replies; `cursor` is the previous
/// page's `cursor`. `creatorUrn` marks the uploader's own likes.
#[tauri::command]
pub async fn api_comments(
    app: AppHandle,
    api: State<'_, Api>,
    track_urn: String,
    creator_urn: String,
    cursor: Option<String>,
) -> Result<CommentPage, String> {
    let (http, token) = session(&app)?;
    api.comments(&http, &token, &track_urn, &creator_urn, cursor.as_deref())
        .await
        .map_err(|error| {
            diagnostics::warn(format!("[api] comments: {error}"));
            fail(&app, error)
        })
}

/// Post a comment at `atMs`. A reply is a comment whose body starts with
/// `@permalink: `, at the parent's time (as the site sends it).
#[tauri::command]
pub async fn api_comment_create(
    app: AppHandle,
    api: State<'_, Api>,
    track_urn: String,
    at_ms: u64,
    body: String,
) -> Result<Comment, String> {
    let body = body.trim();
    if body.is_empty() {
        return Err("empty comment".into());
    }
    let variables = json!({
        "input": { "trackUrn": track_urn, "timestamp": at_ms, "body": body, "token": null }
    });
    let data = site_mutation(
        &app,
        &api,
        "CreateTrackComment",
        graph::CREATE_COMMENT,
        variables,
    )
    .await?;
    graph::created_comment(json!({ "data": data }))
}

#[tauri::command]
pub async fn api_comment_delete(
    app: AppHandle,
    api: State<'_, Api>,
    comment_urn: String,
) -> Result<(), String> {
    let variables = json!({ "input": { "commentUrn": comment_urn } });
    let data = site_mutation(
        &app,
        &api,
        "DeleteTrackComment",
        graph::DELETE_COMMENT,
        variables,
    )
    .await?;
    match data.get("deleteTrackComment") {
        Some(Value::Bool(true)) => Ok(()),
        _ => Err("comment was not deleted".into()),
    }
}

#[tauri::command]
pub async fn api_comment_like(
    app: AppHandle,
    api: State<'_, Api>,
    comment_urn: String,
    track_urn: String,
    liked: bool,
) -> Result<(), String> {
    let variables =
        json!({ "commentUrn": comment_urn, "trackUrn": track_urn, "secretToken": null });
    if liked {
        let data = site_mutation(
            &app,
            &api,
            "LikeTrackComment",
            graph::LIKE_COMMENT,
            variables,
        )
        .await?;
        graph::expect_type(&data["upsertInteraction"], "Interaction")
    } else {
        let data = site_mutation(
            &app,
            &api,
            "UnlikeTrackComment",
            graph::UNLIKE_COMMENT,
            variables,
        )
        .await?;
        match data.get("removeInteraction") {
            Some(Value::Bool(true)) => Ok(()),
            _ => Err("like was not removed".into()),
        }
    }
}

/// Repost (`reposted: true`) or un-repost a track.
#[tauri::command]
pub async fn api_set_repost(
    app: AppHandle,
    api: State<'_, Api>,
    track_urn: String,
    reposted: bool,
) -> Result<(), String> {
    let variables = json!({ "trackUrn": track_urn });
    if reposted {
        let data = site_mutation(&app, &api, "RepostTrack", graph::REPOST_TRACK, variables).await?;
        graph::expect_type(&data["repostTrack"], "Repost")
    } else {
        let data = site_mutation(
            &app,
            &api,
            "UnrepostTrack",
            graph::UNREPOST_TRACK,
            variables,
        )
        .await?;
        graph::expect_type(&data["removeTrackRepost"], "RemoveRepostSuccess")
    }
}

/// Ids of all reposted tracks.
#[tauri::command]
pub async fn api_reposted_ids(app: AppHandle, api: State<'_, Api>) -> Result<Vec<u64>, String> {
    let (http, token) = session(&app)?;
    api.reposted_track_ids(&http, &token)
        .await
        .map_err(|error| fail(&app, error))
}

/// Ids of all followed users.
#[tauri::command]
pub async fn api_following_ids(app: AppHandle, api: State<'_, Api>) -> Result<Vec<u64>, String> {
    let (http, token) = session(&app)?;
    let me = my_id(&app, &api, &http, &token).await?;
    api.following_ids(&http, &token, me)
        .await
        .map_err(|error| fail(&app, error))
}

#[tauri::command]
pub async fn api_related(
    app: AppHandle,
    api: State<'_, Api>,
    id: u64,
) -> Result<Page<Track>, String> {
    let (http, token) = session(&app)?;
    let me = my_id(&app, &api, &http, &token).await?;
    keep(&app, api.related(&http, &token, id, me).await)
}

/// Playlists (`albums: false`) or albums with this track.
#[tauri::command]
pub async fn api_track_sets(
    app: AppHandle,
    api: State<'_, Api>,
    id: u64,
    albums: bool,
    cursor: Option<String>,
) -> Result<Page<Playlist>, String> {
    let (http, token) = session(&app)?;
    keep(
        &app,
        api.track_sets(&http, &token, id, albums, at(&cursor)).await,
    )
}

#[tauri::command]
pub async fn api_likers(
    app: AppHandle,
    api: State<'_, Api>,
    id: u64,
    cursor: Option<String>,
) -> Result<Page<User>, String> {
    let (http, token) = session(&app)?;
    keep(&app, api.likers(&http, &token, id, at(&cursor)).await)
}

/// The signed-in user's own playlists (for "add to playlist").
#[tauri::command]
pub async fn api_own_playlists(
    app: AppHandle,
    api: State<'_, Api>,
    cursor: Option<String>,
) -> Result<Page<Playlist>, String> {
    let (http, token) = session(&app)?;
    let me = my_id(&app, &api, &http, &token).await?;
    keep(
        &app,
        api.own_playlists(&http, &token, me, at(&cursor)).await,
    )
}

/// Add `trackId` to the end of a playlist (`present: true`) or remove it.
/// The site sends the whole new track list (`PUT /playlists/{id}`), so the
/// current list is read fresh first. Returns the updated track ids.
#[tauri::command]
pub async fn api_playlist_set_track(
    app: AppHandle,
    api: State<'_, Api>,
    playlist_id: u64,
    track_id: u64,
    present: bool,
) -> Result<Vec<u64>, String> {
    let (http, token) = session(&app)?;
    let playlist = api
        .playlist(&http, &token, playlist_id, None)
        .await
        .map_err(|error| fail(&app, error))?;
    let current: Vec<u64> = playlist.tracks.iter().map(|t| t.id()).collect();
    let Some(next) = edit_track_list(&current, track_id, present) else {
        return Ok(current);
    };
    let url = api
        .signed_url(&http, &format!("/playlists/{playlist_id}"))
        .await
        .map_err(|error| fail(&app, error))?;
    let body = json!({ "playlist": { "tracks": next } });
    site_write(
        &app,
        "playlist edit",
        "PUT",
        url.as_str(),
        &token,
        Some(&body),
    )
    .await?;
    Ok(next)
}

/// The new track list, or `None` if nothing changes.
fn edit_track_list(current: &[u64], track_id: u64, present: bool) -> Option<Vec<u64>> {
    let has = current.contains(&track_id);
    match (present, has) {
        (true, false) => Some(current.iter().copied().chain([track_id]).collect()),
        (false, true) => Some(
            current
                .iter()
                .copied()
                .filter(|&id| id != track_id)
                .collect(),
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::edit_track_list;

    #[test]
    fn track_list_edits() {
        assert_eq!(edit_track_list(&[1, 2], 3, true), Some(vec![1, 2, 3]));
        assert_eq!(edit_track_list(&[1, 2, 3], 2, false), Some(vec![1, 3]));
        assert_eq!(edit_track_list(&[1, 2], 2, true), None);
        assert_eq!(edit_track_list(&[1, 2], 3, false), None);
    }
}
