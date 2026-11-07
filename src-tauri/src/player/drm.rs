//! DRM playback (Widevine, CENC) through a hidden WebView2 with the app's own
//! page (`index.html?view=drm`). Decryption happens only inside the licensed
//! CDM of WebView2; nothing here sees keys or clear audio.
//!
//! Rust keeps the network and the state, as everywhere else: the stream is
//! downloaded still encrypted by `player::stream`, the page pulls its
//! segments over IPC and feeds Media Source Extensions, and the CDM's licence
//! messages come back here to be POSTed to SoundCloud's licence server
//! (`license.media-streaming.soundcloud.cloud`, per the DRM HAR) through the
//! shared client. The page only plays, and reports position, end and errors
//! tagged with the player's generation.
//!
//! The window lives while a DRM track is current and is destroyed after a
//! minute of not being needed.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use super::stream::DrmMedia;
use crate::app::{diagnostics, visibility};
use crate::audio::types::HIDDEN_TICK_INTERVAL;
use crate::auth::browser_proxy;
use crate::net::NetState;
use crate::rt::{AppHandle, Rt};
use crate::shared::hosts;

pub const LABEL: &str = "drm";
const PAGE: &str = "index.html?view=drm";
/// Bumped with `PAGE_VERSION` in `src/drm/player.ts`; logged at boot to tell
/// a stale cached page from the current one.
const PAGE_VERSION: u32 = 2;
/// Its own WebView2 profile: it needs flags the main window doesn't have.
const PROFILE_DIR: &str = "drm-profile";
/// No user gesture ever happens in a hidden window.
const AUTOPLAY_ARG: &str = "--autoplay-policy=no-user-gesture-required";
const COMMAND_EVENT: &str = "drm:command";
const IDLE_CLOSE: Duration = Duration::from_secs(60);
const LICENSE_TIMEOUT: Duration = Duration::from_secs(20);
/// No sound this long after a load (window start and Widevine download
/// included) counts as a failure instead of a silently running clock.
const STALL_TIMEOUT: Duration = Duration::from_secs(25);
/// Between page reports (~4 a second) the position is moved on by the
/// clock, but never further than this: a stuck page freezes the time.
const MAX_EXTRAPOLATION: f64 = 1.5;
/// The page's error when Widevine never answers (`src/drm/player.ts`).
const KEY_SYSTEM_STUCK: &str = "key system request timed out";
const GENERATION_HEADER: &str = "x-generation";

/// What the page is told to do.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PageCommand {
    #[serde(rename_all = "camelCase")]
    Load {
        generation: u64,
        /// Seconds per segment; the page fetches them by index.
        durations: Vec<f64>,
        volume: f64,
        /// Start playing here, seconds.
        start: f64,
    },
    Play,
    Pause,
    Seek {
        seconds: f64,
    },
    Volume {
        level: f64,
    },
    Stop,
}

/// What the page reported, for the player actor.
#[derive(Debug)]
pub enum PageEvent {
    Ended,
    Failed(String),
}

struct Current {
    generation: u64,
    media: DrmMedia,
    /// Where playback starts, seconds.
    start: f64,
}

#[derive(Default)]
struct Inner {
    current: Option<Arc<Current>>,
    volume: f64,
    /// The page has booted and listens for commands.
    ready: bool,
    position: f64,
    position_at: Option<Instant>,
    playing: bool,
    last_tick: Option<Instant>,
    /// The page reported the position moving: sound is coming out.
    started: bool,
    /// Licence messages answered for the current track (for the stall log).
    licences: u32,
    /// Start-up stages the page reported for the current track.
    stages: u32,
}

#[derive(Default)]
pub struct DrmState {
    inner: Mutex<Inner>,
    /// Bumped whenever the window is wanted again; a pending close checks it.
    wanted: AtomicU64,
    /// Widevine hung in this profile: delete it before the next window.
    reset_profile: AtomicBool,
}

/// Start `media` (generation `generation`) in the WebView player. `percent`
/// is 0–100, like the actor's volume; the page wants a 0–1 level.
pub fn load(app: &AppHandle, generation: u64, media: DrmMedia, percent: f64, start: f64) {
    let state = app.state::<DrmState>();
    state.wanted.fetch_add(1, Ordering::SeqCst);
    let command = {
        let mut inner = state.inner.lock().unwrap();
        inner.current = Some(Arc::new(Current {
            generation,
            media,
            start,
        }));
        inner.volume = (percent / 100.0).clamp(0.0, 1.0);
        inner.started = false;
        inner.licences = 0;
        inner.stages = 0;
        inner.position = start;
        inner.position_at = None;
        inner.playing = true;
        inner.ready.then(|| load_command(&inner))
    };
    match command {
        Some(command) => send(app, command),
        None if app.get_webview_window(LABEL).is_none() => open(app),
        // Booting: the page asks for the load itself (`drm_boot`).
        None => {}
    }
    watch_for_stall(app, generation);
}

/// Report a failure if the track of `generation` still plays no sound after
/// [`STALL_TIMEOUT`] while it should be playing.
fn watch_for_stall(app: &AppHandle, generation: u64) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(STALL_TIMEOUT).await;
        let (message, reset) = {
            let state = app.state::<DrmState>();
            let inner = state.inner.lock().unwrap();
            let same = inner
                .current
                .as_ref()
                .is_some_and(|current| current.generation == generation);
            if !same || inner.started || !inner.playing {
                return;
            }
            let message = format!(
                "no sound after {} s (page ready: {}, stages: {}, licence answers: {})",
                STALL_TIMEOUT.as_secs(),
                inner.ready,
                inner.stages,
                inner.licences
            );
            // The page runs but never started the track: a broken or stale
            // profile, so the next try starts from a fresh one.
            (message, inner.ready && inner.stages == 0)
        };
        if reset {
            schedule_profile_reset(&app);
        }
        super::send(
            &app,
            super::Command::Drm {
                generation,
                event: PageEvent::Failed(message),
            },
        );
    });
}

pub fn play(app: &AppHandle) {
    set_playing(app, true);
    send(app, PageCommand::Play);
}

pub fn pause(app: &AppHandle) {
    let position = position(app);
    {
        let state = app.state::<DrmState>();
        let mut inner = state.inner.lock().unwrap();
        inner.position = position;
        inner.position_at = None;
    }
    set_playing(app, false);
    send(app, PageCommand::Pause);
}

pub fn seek(app: &AppHandle, seconds: f64) {
    {
        let state = app.state::<DrmState>();
        let mut inner = state.inner.lock().unwrap();
        inner.position = seconds;
        inner.position_at = inner.playing.then(Instant::now);
    }
    send(app, PageCommand::Seek { seconds });
}

pub fn set_volume(app: &AppHandle, percent: f64) {
    let level = (percent / 100.0).clamp(0.0, 1.0);
    app.state::<DrmState>().inner.lock().unwrap().volume = level;
    send(app, PageCommand::Volume { level });
}

/// Whether a DRM track is loaded (playing, paused or ended).
pub fn is_live(app: &AppHandle) -> bool {
    app.state::<DrmState>()
        .inner
        .lock()
        .unwrap()
        .current
        .is_some()
}

/// Position in seconds, moved on by the time since the last report.
pub fn position(app: &AppHandle) -> f64 {
    let state = app.state::<DrmState>();
    let inner = state.inner.lock().unwrap();
    match (inner.playing, inner.position_at) {
        (true, Some(at)) => inner.position + at.elapsed().as_secs_f64().min(MAX_EXTRAPOLATION),
        _ => inner.position,
    }
}

/// Stop and forget the track; the window closes if nothing needs it soon.
pub fn release(app: &AppHandle) {
    let state = app.state::<DrmState>();
    let had_track = {
        let mut inner = state.inner.lock().unwrap();
        inner.playing = false;
        inner.current.take().is_some()
    };
    if had_track {
        send(app, PageCommand::Stop);
    }
    if app.get_webview_window(LABEL).is_none() {
        return;
    }
    let wanted = state.wanted.load(Ordering::SeqCst);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(IDLE_CLOSE).await;
        let state = app.state::<DrmState>();
        if state.wanted.load(Ordering::SeqCst) == wanted && !is_live(&app) {
            close(&app);
        }
    });
}

fn close(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.destroy();
    }
    app.state::<DrmState>().inner.lock().unwrap().ready = false;
}

fn set_playing(app: &AppHandle, playing: bool) {
    let state = app.state::<DrmState>();
    let mut inner = state.inner.lock().unwrap();
    inner.playing = playing;
    if !playing {
        inner.position_at = None;
    }
}

fn load_command(inner: &Inner) -> PageCommand {
    let current = inner.current.as_ref().expect("load_command with a track");
    PageCommand::Load {
        generation: current.generation,
        durations: current.media.durations.clone(),
        volume: inner.volume,
        start: current.start,
    }
}

fn send(app: &AppHandle, command: PageCommand) {
    if !app.state::<DrmState>().inner.lock().unwrap().ready {
        return;
    }
    if let Err(error) = app.emit_to(LABEL, COMMAND_EVENT, command) {
        diagnostics::warn(format!("[drm] command: {error}"));
    }
}

fn open(app: &AppHandle) {
    let profile = match app.path().app_data_dir() {
        Ok(dir) => dir.join(PROFILE_DIR),
        Err(error) => return fail_current(app, format!("profile dir: {error}")),
    };
    if app
        .state::<DrmState>()
        .reset_profile
        .swap(false, Ordering::SeqCst)
    {
        reset_profile(&profile);
    }
    let built = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App(page_url().into()))
        .title("Pulse DRM")
        .visible(false)
        .focused(false)
        .skip_taskbar(true)
        .data_directory(profile.clone())
        .additional_browser_args(&format!("{} {AUTOPLAY_ARG}", browser_proxy::DEFAULT_ARGS))
        .build();
    if let Err(error) = built {
        fail_current(app, format!("window: {error}"));
    }
    log_widevine(&profile);
}

/// Widevine never answered in this profile (the page timed out asking for
/// it): close the window, and the next one starts from a fresh profile, so
/// WebView2 installs the module again. The profile holds nothing else.
fn schedule_profile_reset(app: &AppHandle) {
    diagnostics::warn("[drm] Widevine did not answer: the DRM profile will be recreated");
    app.state::<DrmState>()
        .reset_profile
        .store(true, Ordering::SeqCst);
    close(app);
}

fn reset_profile(profile: &std::path::Path) {
    match std::fs::remove_dir_all(profile) {
        Ok(()) => diagnostics::log("INFO", "[drm] DRM profile recreated"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => diagnostics::warn(format!("[drm] DRM profile reset: {error}")),
    }
}

/// Whether WebView2 has the Widevine module in this profile yet (it fetches
/// it on first use); without it no licence request is ever made.
fn log_widevine(profile: &std::path::Path) {
    let dir = profile.join("EBWebView").join("WidevineCdm");
    let versions: Vec<String> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.path().is_dir())
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    let found = if versions.is_empty() {
        "none".to_string()
    } else {
        versions.join(", ")
    };
    diagnostics::log("INFO", format!("[drm] Widevine module in profile: {found}"));
}

/// The page with a per-launch tag: WebView2 keeps an HTTP cache in the
/// profile, and a cached `index.html` from an older build would load an old
/// player script.
fn page_url() -> String {
    static LAUNCH: std::sync::OnceLock<u128> = std::sync::OnceLock::new();
    let launch = LAUNCH.get_or_init(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_millis())
    });
    format!("{PAGE}&build={}-{launch}", env!("CARGO_PKG_VERSION"))
}

fn fail_current(app: &AppHandle, message: String) {
    diagnostics::warn(format!("[drm] {message}"));
    let generation = app
        .state::<DrmState>()
        .inner
        .lock()
        .unwrap()
        .current
        .as_ref()
        .map(|current| current.generation);
    if let Some(generation) = generation {
        super::send(
            app,
            super::Command::Drm {
                generation,
                event: PageEvent::Failed(message),
            },
        );
    }
}

/// The track for `generation`, if it is still the current one.
fn current(app: &AppHandle, generation: u64) -> Option<Arc<Current>> {
    app.state::<DrmState>()
        .inner
        .lock()
        .unwrap()
        .current
        .clone()
        .filter(|current| current.generation == generation)
}

/// Only the DRM page may call these commands.
fn from_page(webview: &tauri::Webview<Rt>) -> Result<(), String> {
    if webview.label() == LABEL {
        Ok(())
    } else {
        Err("not the DRM player".into())
    }
}

/// The page booted: from now on it gets commands. Returns the track to load,
/// if one is waiting.
#[tauri::command]
pub fn drm_boot(
    webview: tauri::Webview<Rt>,
    app: AppHandle,
    page: Option<u32>,
) -> Result<Option<PageCommand>, String> {
    from_page(&webview)?;
    let page = match page {
        Some(version) if version == PAGE_VERSION => format!("v{version}"),
        Some(version) => format!("v{version}, expected v{PAGE_VERSION}"),
        None => format!("stale, expected v{PAGE_VERSION}"),
    };
    diagnostics::log("INFO", format!("[drm] player page ready ({page})"));
    let state = app.state::<DrmState>();
    let mut inner = state.inner.lock().unwrap();
    inner.ready = true;
    Ok(inner.current.is_some().then(|| load_command(&inner)))
}

/// A diagnostic line from the page (boot capabilities, a caught error),
/// logged independently of any track. Redacted like every other log line.
#[tauri::command]
pub fn drm_diag(webview: tauri::Webview<Rt>, message: String) -> Result<(), String> {
    from_page(&webview)?;
    diagnostics::log("INFO", format!("[drm] diag: {message}"));
    Ok(())
}

/// Segment `index` of the current track, still encrypted; `-1` is the init
/// segment.
#[tauri::command]
pub fn drm_segment(
    webview: tauri::Webview<Rt>,
    app: AppHandle,
    generation: u64,
    index: i64,
) -> Result<Response, String> {
    from_page(&webview)?;
    let current = current(&app, generation).ok_or("stale track")?;
    let bytes = if index < 0 {
        current.media.init.clone()
    } else {
        current
            .media
            .segments
            .get(index as usize)
            .ok_or("no such segment")?
            .clone()
    };
    Ok(Response::new(bytes))
}

/// A CDM message (service certificate or licence request) for the current
/// track: POSTed as-is to SoundCloud's Widevine licence server, the answer
/// goes back to the CDM unread.
#[tauri::command]
pub async fn drm_license(
    webview: tauri::Webview<Rt>,
    app: AppHandle,
    request: Request<'_>,
) -> Result<Response, String> {
    from_page(&webview)?;
    let generation: u64 = request
        .headers()
        .get(GENERATION_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
        .ok_or("missing generation")?;
    let InvokeBody::Raw(challenge) = request.body() else {
        return Err("licence message must be raw bytes".into());
    };
    let current = current(&app, generation).ok_or("stale track")?;
    let url = hosts::license_url(&current.media.license_token);
    let response = app
        .state::<NetState>()
        .client()
        .post(url)
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
        .body(challenge.clone())
        .timeout(LICENSE_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("licence: {}", e.without_url()))?;
    let status = response.status();
    if !status.is_success() {
        diagnostics::warn(format!("[drm] licence server answered {status}"));
        return Err(format!("licence: HTTP {status}"));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("licence: {}", e.without_url()))?;
    {
        let state = app.state::<DrmState>();
        let mut inner = state.inner.lock().unwrap();
        if inner
            .current
            .as_ref()
            .is_some_and(|c| c.generation == generation)
        {
            inner.licences += 1;
        }
    }
    diagnostics::log(
        "INFO",
        format!(
            "[drm] licence message {} B -> answer {} B",
            challenge.len(),
            bytes.len()
        ),
    );
    Ok(Response::new(bytes.to_vec()))
}

/// Progress from the page: `position` (seconds, ~4 per second while
/// playing), `ended`, `error` with a message, or a start-up `stage` (logged
/// only).
#[tauri::command]
pub fn drm_report(
    webview: tauri::Webview<Rt>,
    app: AppHandle,
    generation: u64,
    event: String,
    position: f64,
    message: Option<String>,
) -> Result<(), String> {
    from_page(&webview)?;
    if current(&app, generation).is_none() {
        return Ok(());
    }
    if event != "position" {
        diagnostics::log(
            "INFO",
            format!(
                "[drm] page {event} at {position:.1}s{}",
                message
                    .as_deref()
                    .map(|m| format!(": {m}"))
                    .unwrap_or_default()
            ),
        );
    }
    match event.as_str() {
        "position" => report_position(&app, position),
        "stage" => {
            let state = app.state::<DrmState>();
            state.inner.lock().unwrap().stages += 1;
        }
        "ended" => super::send(
            &app,
            super::Command::Drm {
                generation,
                event: PageEvent::Ended,
            },
        ),
        "error" => {
            let message = message.unwrap_or_default();
            if message.contains(KEY_SYSTEM_STUCK) {
                schedule_profile_reset(&app);
            }
            super::send(
                &app,
                super::Command::Drm {
                    generation,
                    event: PageEvent::Failed(message),
                },
            )
        }
        other => return Err(format!("unknown report {other}")),
    }
    Ok(())
}

/// Store the position and pass it on as `audio:tick`, like the engine's
/// tick thread (sparser while every page is hidden).
fn report_position(app: &AppHandle, position: f64) {
    let emit = {
        let state = app.state::<DrmState>();
        let mut inner = state.inner.lock().unwrap();
        let start = inner.current.as_ref().map_or(0.0, |current| current.start);
        if position > start + 0.2 {
            inner.started = true;
        }
        inner.position = position;
        inner.position_at = inner.playing.then(Instant::now);
        let due = !visibility::pages_hidden()
            || inner
                .last_tick
                .is_none_or(|at| at.elapsed() >= HIDDEN_TICK_INTERVAL);
        if due {
            inner.last_tick = Some(Instant::now());
        }
        due
    };
    if emit {
        app.emit("audio:tick", position).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_commands_match_the_page_types() {
        let load = PageCommand::Load {
            generation: 7,
            durations: vec![10.008, 4.5],
            volume: 0.6,
            start: 12.5,
        };
        assert_eq!(
            serde_json::to_value(load).unwrap(),
            serde_json::json!({"kind": "load", "generation": 7, "durations": [10.008, 4.5], "volume": 0.6, "start": 12.5})
        );
        assert_eq!(
            serde_json::to_value(PageCommand::Seek { seconds: 3.0 }).unwrap(),
            serde_json::json!({"kind": "seek", "seconds": 3.0})
        );
        assert_eq!(
            serde_json::to_value(PageCommand::Stop).unwrap(),
            serde_json::json!({"kind": "stop"})
        );
    }
}
