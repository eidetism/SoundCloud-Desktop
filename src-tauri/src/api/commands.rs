//! api-v2 commands for the frontend. Lists take an optional `cursor` (the
//! previous page's `nextHref`). Every full track in an answer is also kept
//! in the player's track store, so the frontend can play by id.

use tauri::{Manager, State};

use crate::api::models::{
    CarriesTracks, Comment, FeedItem, Page, PlayHistoryItem, Playlist, SearchPage, Selection,
    Track, TrackLike,
};
use crate::api::{Api, ApiError, PageAt};
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
) -> Result<Playlist, String> {
    let (http, token) = session(&app)?;
    keep(&app, api.playlist(&http, &token, id).await)
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
pub async fn api_comments(
    app: AppHandle,
    api: State<'_, Api>,
    track_id: u64,
    cursor: Option<String>,
) -> Result<Page<Comment>, String> {
    let (http, token) = session(&app)?;
    keep(
        &app,
        api.comments(&http, &token, track_id, at(&cursor)).await,
    )
}
