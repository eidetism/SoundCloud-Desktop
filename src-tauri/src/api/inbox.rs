//! Direct message commands and the unread poller. Reads use the shared
//! client; marking read and sending are DataDome-guarded writes and go
//! through the hidden site window. The poller checks unread conversations
//! every 60 s while the main window is shown and every 5 min otherwise, and
//! raises a Windows notification for new messages only when the window is
//! not shown.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use serde_json::json;
use tauri::{Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::Notify;
use tokio::time::Instant;

use super::commands::{at, fail, keep, my_id, session, site_write};
use super::messages::{self, Conversation, Message, Resolved};
use super::models::{Page, Track};
use super::{Api, ApiError};
use crate::app::{diagnostics, locale};
use crate::auth::SiteBody;
use crate::rt::AppHandle;

const EVENT: &str = "messages:unread";
const SHOWN_INTERVAL: Duration = Duration::from_secs(60);
const HIDDEN_INTERVAL: Duration = Duration::from_secs(300);
const PREVIEW_CHARS: usize = 160;

#[derive(Default)]
pub struct Inbox {
    unread: AtomicUsize,
    /// Unread conversation → `sent_at` of the last message already notified
    /// (or seen while the window was shown).
    known: Mutex<HashMap<String, String>>,
    wake: Notify,
}

impl Inbox {
    /// Check now instead of at the next tick.
    fn recheck(&self) {
        self.wake.notify_one();
    }
}

pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut last: Option<Instant> = None;
        loop {
            let shown = main_shown(&app);
            let interval = if shown {
                SHOWN_INTERVAL
            } else {
                HIDDEN_INTERVAL
            };
            if last.is_none_or(|at| at.elapsed() >= interval) {
                if check(&app, !shown).await {
                    last = Some(Instant::now());
                } else {
                    last = None;
                }
            }
            let inbox = app.state::<Inbox>();
            tokio::select! {
                () = tokio::time::sleep(SHOWN_INTERVAL) => {}
                () = inbox.wake.notified() => last = None,
            }
        }
    });
}

fn main_shown(app: &AppHandle) -> bool {
    app.get_webview_window("main").is_some_and(|window| {
        window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(true)
    })
}

/// One unread check; `false` when signed out or the request failed.
async fn check(app: &AppHandle, notify: bool) -> bool {
    let Ok((http, token)) = session(app) else {
        if app.state::<Inbox>().unread.swap(0, Ordering::Relaxed) != 0 {
            app.emit(EVENT, 0).ok();
        }
        return false;
    };
    let api = app.state::<Api>();
    let Ok(me) = my_id(app, &api, &http, &token).await else {
        return false;
    };
    let page = match api.unread_conversations(&http, &token, me).await {
        Ok(page) => page,
        Err(error) => {
            diagnostics::warn(format!("[messages] unread: {error}"));
            if matches!(error, ApiError::Unauthorized) {
                crate::auth::verify(app);
            }
            return false;
        }
    };
    let unread: Vec<Conversation> = messages::conversations(page, me)
        .collection
        .into_iter()
        .filter(|c| c.last_message.as_ref().is_some_and(|m| !m.mine))
        .collect();
    let fresh = remember(&app.state::<Inbox>().known, &unread);
    let changed = app
        .state::<Inbox>()
        .unread
        .swap(unread.len(), Ordering::Relaxed)
        != unread.len();
    // A new message in a conversation that was already unread also counts:
    // open lists refetch on the event.
    if changed || !fresh.is_empty() {
        app.emit(EVENT, unread.len()).ok();
    }
    if notify && !fresh.is_empty() {
        show_notification(app, &fresh);
    }
    true
}

/// Record the unread conversations; returns those with a message not seen
/// before. Conversations no longer unread are forgotten.
fn remember<'a>(
    known: &Mutex<HashMap<String, String>>,
    unread: &'a [Conversation],
) -> Vec<&'a Conversation> {
    let mut known = known.lock().unwrap();
    known.retain(|with, _| unread.iter().any(|c| &c.with == with));
    unread
        .iter()
        .filter(|c| {
            let sent = c.last_message.as_ref().map(|m| m.sent_at.clone());
            let sent = sent.unwrap_or_default();
            known.insert(c.with.clone(), sent.clone()).as_ref() != Some(&sent)
        })
        .collect()
}

fn show_notification(app: &AppHandle, fresh: &[&Conversation]) {
    let (title, body) = match fresh {
        [one] => (
            one.peer
                .as_ref()
                .map_or_else(|| "SoundCloud".to_string(), |u| u.username.clone()),
            one.last_message
                .as_ref()
                .map(|m| preview(&m.body))
                .unwrap_or_default(),
        ),
        many => (
            "Pulse".to_string(),
            format!(
                "{} {}",
                locale::pick(
                    "Новые сообщения в диалогах:",
                    "New messages in conversations:"
                ),
                many.len()
            ),
        ),
    };
    if let Err(error) = app.notification().builder().title(title).body(body).show() {
        diagnostics::warn(format!("[messages] notification: {error}"));
    }
}

fn preview(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.char_indices().nth(PREVIEW_CHARS) {
        Some((cut, _)) => format!("{}…", &flat[..cut]),
        None => flat,
    }
}

fn checked_with(with: &str) -> Result<(), String> {
    if messages::valid_with(with) {
        Ok(())
    } else {
        Err("bad conversation".into())
    }
}

/// The number of unread conversations, as last checked.
#[tauri::command]
pub fn messages_unread(inbox: State<'_, Inbox>) -> usize {
    inbox.unread.load(Ordering::Relaxed)
}

#[tauri::command]
pub async fn api_conversations(
    app: AppHandle,
    api: State<'_, Api>,
    cursor: Option<String>,
) -> Result<Page<Conversation>, String> {
    let (http, token) = session(&app)?;
    let me = my_id(&app, &api, &http, &token).await?;
    let page = api
        .conversations(&http, &token, me, at(&cursor))
        .await
        .map_err(|error| fail(&app, error))?;
    Ok(messages::conversations(page, me))
}

#[tauri::command]
pub async fn api_conversation(
    app: AppHandle,
    api: State<'_, Api>,
    with: String,
) -> Result<Conversation, String> {
    checked_with(&with)?;
    let (http, token) = session(&app)?;
    let me = my_id(&app, &api, &http, &token).await?;
    api.conversation(&http, &token, me, &with)
        .await
        .map_err(|error| fail(&app, error))?
        .view(me)
        .ok_or_else(|| "bad conversation".into())
}

/// A page of messages, newest first.
#[tauri::command]
pub async fn api_messages(
    app: AppHandle,
    api: State<'_, Api>,
    with: String,
    cursor: Option<String>,
) -> Result<Page<Message>, String> {
    checked_with(&with)?;
    let (http, token) = session(&app)?;
    let me = my_id(&app, &api, &http, &token).await?;
    let page = api
        .messages(&http, &token, me, &with, at(&cursor))
        .await
        .map_err(|error| fail(&app, error))?;
    Ok(messages::messages(page, me))
}

async fn mark_read(app: &AppHandle, api: &Api, with: &str) -> Result<(), String> {
    let (http, token) = session(app)?;
    let me = my_id(app, api, &http, &token).await?;
    let url = api
        .signed_url(&http, &format!("/users/{me}/conversations/{with}"))
        .await
        .map_err(|error| fail(app, error))?;
    let result = site_write(
        app,
        "mark read",
        "PUT",
        url.as_str(),
        &token,
        Some(SiteBody::Form("read=true")),
    )
    .await
    .map(drop);
    app.state::<Inbox>().recheck();
    result
}

/// Mark a conversation read, as the site does on opening it.
#[tauri::command]
pub async fn api_conversation_read(
    app: AppHandle,
    api: State<'_, Api>,
    with: String,
) -> Result<(), String> {
    checked_with(&with)?;
    mark_read(&app, &api, &with).await
}

/// Send a message; returns the conversation with it as `lastMessage`. Like
/// the site, the conversation is then marked read on our side.
#[tauri::command]
pub async fn api_message_send(
    app: AppHandle,
    api: State<'_, Api>,
    with: String,
    body: String,
) -> Result<Conversation, String> {
    checked_with(&with)?;
    if with == messages::SYSTEM {
        return Err("system conversation is read-only".into());
    }
    let body = body.trim();
    if body.is_empty() {
        return Err("empty message".into());
    }
    let (http, token) = session(&app)?;
    let me = my_id(&app, &api, &http, &token).await?;
    let url = api
        .signed_url(&http, &format!("/users/{me}/conversations/{with}"))
        .await
        .map_err(|error| fail(&app, error))?;
    let payload = json!({ "contents": body });
    let response = site_write(
        &app,
        "send message",
        "POST",
        url.as_str(),
        &token,
        Some(SiteBody::Json(&payload)),
    )
    .await?;
    let sent: messages::RawConversation = response
        .body
        .ok_or("send message: answer is not JSON")
        .and_then(|body| {
        serde_json::from_value(body).map_err(|_| "send message: unexpected answer")
    })?;
    if let Err(error) = mark_read(&app, &api, &with).await {
        diagnostics::warn(format!("[messages] mark read after send: {error}"));
    }
    sent.view(me).ok_or_else(|| "bad conversation".into())
}

/// The track behind a soundcloud.com track link, for a message preview;
/// `None` for anything else or a track that is gone.
#[tauri::command]
pub async fn api_resolve_track(
    app: AppHandle,
    api: State<'_, Api>,
    url: String,
) -> Result<Option<Track>, String> {
    let Some(page) = messages::track_page(&url) else {
        return Ok(None);
    };
    let (http, token) = session(&app)?;
    match api.resolve(&http, &token, &page).await {
        Ok(Resolved::Track(track)) => keep(&app, Ok(*track)).map(Some),
        Ok(Resolved::Other) | Err(ApiError::Http(404)) => Ok(None),
        Err(error) => Err(fail(&app, error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unread(with: &str, sent_at: &str) -> Conversation {
        Conversation {
            with: with.into(),
            peer: None,
            system: false,
            read: false,
            last_message: Some(Message {
                body: String::new(),
                sender: None,
                mine: false,
                system: false,
                sent_at: sent_at.into(),
            }),
        }
    }

    #[test]
    fn only_new_messages_are_fresh() {
        let known = Mutex::default();
        let first = [unread("1", "a"), unread("2", "a")];
        assert_eq!(remember(&known, &first).len(), 2);
        assert!(remember(&known, &first).is_empty());
        let next = [unread("1", "b"), unread("2", "a")];
        let fresh = remember(&known, &next);
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0].with, "1");
        // Read on the site, then a new message: notified again.
        remember(&known, &[]);
        assert_eq!(remember(&known, &[unread("2", "a")]).len(), 1);
    }

    #[test]
    fn previews_are_flat_and_short() {
        assert_eq!(preview("a\n\n b  c"), "a b c");
        let long = "я".repeat(PREVIEW_CHARS + 5);
        let cut = preview(&long);
        assert_eq!(cut.chars().count(), PREVIEW_CHARS + 1);
        assert!(cut.ends_with('…'));
    }
}
