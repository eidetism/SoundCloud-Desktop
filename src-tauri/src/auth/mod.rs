//! Session: the OAuth token (Credential Manager) plus a cached profile so the
//! app can greet the user offline. Only a 401 on `/me` signs the user out;
//! network errors, timeouts and 5xx keep the session.

pub mod browser_proxy;
pub mod forwarder;
mod login;
mod site;

pub use login::LABEL as LOGIN_WINDOW;
pub use site::{
    LABEL as SITE_WINDOW, SiteBody, SiteResponse, close as close_site, request as site_request,
};

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{Emitter, Manager, State};

use crate::api::models::Me;
use crate::api::{Api, ApiError};
use crate::app::diagnostics;
use crate::net::NetState;
use crate::rt::AppHandle;
use crate::secrets::{self, Secret};

const PROFILE_CACHE: &str = "session.json";
const EVENT: &str = "auth:changed";

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum Session {
    SignedOut,
    #[serde(rename_all = "camelCase")]
    SignedIn {
        /// Cached profile; `None` only if the cache was lost.
        user: Option<Me>,
        /// `false` until `/me` confirms the token in this run.
        verified: bool,
    },
}

pub struct AuthState {
    token: Mutex<Option<String>>,
    session: Mutex<Session>,
    profile_cache: PathBuf,
    /// Keeps the login window's proxy forwarder alive while it is open.
    login: login::LoginSlot,
    site: site::SiteSlot,
}

impl AuthState {
    pub fn init(data_dir: &Path) -> Self {
        let profile_cache = data_dir.join(PROFILE_CACHE);
        let token = secrets::load(Secret::OAuthToken).unwrap_or_else(|error| {
            diagnostics::warn(format!("[auth] {error}"));
            None
        });
        let session = match &token {
            Some(_) => Session::SignedIn {
                user: read_profile(&profile_cache),
                verified: false,
            },
            None => Session::SignedOut,
        };
        Self {
            token: Mutex::new(token),
            session: Mutex::new(session),
            profile_cache,
            login: login::LoginSlot::default(),
            site: site::SiteSlot::default(),
        }
    }

    pub fn session(&self) -> Session {
        self.session.lock().unwrap().clone()
    }

    pub fn token(&self) -> Option<String> {
        self.token.lock().unwrap().clone()
    }

    fn set_session(&self, app: &AppHandle, session: Session) {
        *self.session.lock().unwrap() = session.clone();
        app.emit(EVENT, session).ok();
    }

    /// Persist a token that `/me` just accepted.
    fn sign_in(&self, app: &AppHandle, token: String, me: Me) -> Result<(), String> {
        secrets::store(Secret::OAuthToken, &token)?;
        *self.token.lock().unwrap() = Some(token);
        write_profile(&self.profile_cache, &me);
        diagnostics::log("INFO", format!("[auth] signed in as {}", me.username));
        self.set_session(
            app,
            Session::SignedIn {
                user: Some(me),
                verified: true,
            },
        );
        Ok(())
    }

    /// Forget the token and the cached profile. The login WebView profile
    /// (and its DataDome cookie) is deliberately kept.
    fn sign_out(&self, app: &AppHandle) -> Result<(), String> {
        secrets::delete(Secret::OAuthToken)?;
        *self.token.lock().unwrap() = None;
        site::close(app);
        let _ = std::fs::remove_file(&self.profile_cache);
        crate::player::send(app, crate::player::Command::Reset);
        self.set_session(app, Session::SignedOut);
        Ok(())
    }
}

fn read_profile(path: &Path) -> Option<Me> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

fn write_profile(path: &Path, me: &Me) {
    #[derive(Serialize)]
    struct Stored<'a> {
        id: u64,
        username: &'a str,
        permalink: &'a str,
        avatar_url: &'a Option<String>,
        full_name: &'a Option<String>,
    }
    let stored = Stored {
        id: me.id,
        username: &me.username,
        permalink: &me.permalink,
        avatar_url: &me.avatar_url,
        full_name: &me.full_name,
    };
    if let Err(error) = serde_json::to_vec(&stored)
        .map_err(|e| e.to_string())
        .and_then(|bytes| std::fs::write(path, bytes).map_err(|e| e.to_string()))
    {
        diagnostics::warn(format!("[auth] profile cache: {error}"));
    }
}

/// Confirm the stored token with `/me` in the background: on startup, and
/// whenever another endpoint answers 401. Only a 401 here signs out.
pub fn verify(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let auth = app.state::<AuthState>();
        let Some(token) = auth.token() else {
            return;
        };
        let http = app.state::<NetState>().client();
        match app.state::<Api>().me(&http, &token).await {
            Ok(me) => {
                write_profile(&auth.profile_cache, &me);
                auth.set_session(
                    &app,
                    Session::SignedIn {
                        user: Some(me),
                        verified: true,
                    },
                );
            }
            Err(ApiError::Unauthorized) => {
                diagnostics::log("INFO", "[auth] stored token rejected by /me; signing out");
                if let Err(error) = auth.sign_out(&app) {
                    diagnostics::warn(format!("[auth] {error}"));
                }
            }
            Err(error) => diagnostics::warn(format!(
                "[auth] /me check failed, keeping the session: {error}"
            )),
        }
    });
}

#[tauri::command]
pub fn auth_status(auth: State<'_, AuthState>) -> Session {
    auth.session()
}

#[tauri::command]
pub async fn auth_login_start(app: AppHandle) -> Result<(), String> {
    login::open(app).await
}

#[tauri::command]
pub fn auth_logout(app: AppHandle, auth: State<'_, AuthState>) -> Result<(), String> {
    auth.sign_out(&app)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_serializes_with_a_state_tag() {
        assert_eq!(
            serde_json::to_string(&Session::SignedOut).unwrap(),
            r#"{"state":"signedOut"}"#
        );
        let signed_in = Session::SignedIn {
            user: None,
            verified: false,
        };
        assert_eq!(
            serde_json::to_string(&signed_in).unwrap(),
            r#"{"state":"signedIn","user":null,"verified":false}"#
        );
    }

    #[test]
    fn profile_cache_round_trips() {
        let dir = std::env::temp_dir().join(format!("scd-profile-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(PROFILE_CACHE);
        let me = Me {
            id: 7,
            username: "user".into(),
            permalink: "user".into(),
            avatar_url: Some("https://i1.sndcdn.com/a.jpg".into()),
            full_name: None,
        };
        write_profile(&path, &me);
        assert_eq!(read_profile(&path), Some(me));
        std::fs::remove_dir_all(&dir).ok();
    }
}
