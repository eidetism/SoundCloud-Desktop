//! Discord Rich Presence ("Listening to …"), driven by the player. Updates go
//! to one thread that owns the IPC connection; only the latest one matters,
//! so a backlog is coalesced. Without a running Discord the connection is
//! retried at most every [`RETRY_EVERY`].

use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use discord_rich_presence::activity::{
    Activity, ActivityType, Assets, Button, StatusDisplayType, Timestamps,
};
use discord_rich_presence::{DiscordIpc, DiscordIpcClient};
use tauri::Manager;

use crate::app::{diagnostics, locale};
use crate::rt::AppHandle;
use crate::settings::SettingsStore;

/// The "SC Desktop" application on the Discord developer portal.
const DISCORD_CLIENT_ID: &str = "1568311143817019483";
const RETRY_EVERY: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
pub struct Presence {
    pub title: String,
    pub artist: String,
    pub artwork_url: Option<String>,
    pub track_url: Option<String>,
    pub duration_secs: Option<i64>,
    pub elapsed_secs: i64,
    pub playing: bool,
}

pub struct DiscordState {
    updates: Sender<Option<Presence>>,
}

impl DiscordState {
    pub fn start() -> Self {
        let (updates, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("discord-rpc".into())
            .spawn(move || run(rx))
            .expect("failed to spawn discord thread");
        Self { updates }
    }
}

/// Show `presence`, or clear the status with `None`. Respects the setting.
pub fn update(app: &AppHandle, presence: Option<Presence>) {
    let enabled = app.state::<SettingsStore>().get().discord_rich_presence;
    app.state::<DiscordState>()
        .updates
        .send(presence.filter(|_| enabled))
        .ok();
}

fn run(rx: Receiver<Option<Presence>>) {
    let mut client: Option<DiscordIpcClient> = None;
    let mut next_attempt = Instant::now();
    while let Ok(mut latest) = rx.recv() {
        while let Ok(newer) = rx.try_recv() {
            latest = newer;
        }
        let Some(presence) = latest else {
            if let Some(mut open) = client.take() {
                let _ = open.clear_activity();
                let _ = open.close();
            }
            continue;
        };
        if client.is_none() && Instant::now() >= next_attempt {
            client = connect();
            next_attempt = Instant::now() + RETRY_EVERY;
        }
        if let Some(open) = client.as_mut()
            && let Err(error) = open.set_activity(activity(&presence))
        {
            diagnostics::warn(format!("[Discord] set_activity failed: {error}"));
            client = None;
        }
    }
}

fn connect() -> Option<DiscordIpcClient> {
    let mut client = DiscordIpcClient::new(DISCORD_CLIENT_ID);
    // Failing here usually just means Discord isn't running.
    client.connect().ok()?;
    diagnostics::log("INFO", "[Discord] connected");
    Some(client)
}

fn activity(presence: &Presence) -> Activity<'_> {
    let mut activity = Activity::new()
        .activity_type(ActivityType::Listening)
        .status_display_type(StatusDisplayType::Details)
        .details(&presence.title)
        .state(if presence.playing {
            presence.artist.as_str()
        } else {
            locale::pick("На паузе", "Paused")
        });
    if let Some(artwork) = presence.artwork_url.as_deref() {
        activity = activity.assets(
            Assets::new()
                .large_image(artwork)
                .large_text(&presence.artist),
        );
    }
    if presence.playing {
        let start = now_secs() - presence.elapsed_secs;
        let mut timestamps = Timestamps::new().start(start);
        if let Some(duration) = presence.duration_secs {
            timestamps = timestamps.end(start + duration);
        }
        activity = activity.timestamps(timestamps);
    }
    if let Some(url) = presence.track_url.as_deref() {
        activity = activity.buttons(vec![Button::new(
            locale::pick("Слушать в SoundCloud", "Listen on SoundCloud"),
            url,
        )]);
    }
    activity
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
