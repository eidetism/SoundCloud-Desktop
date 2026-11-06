//! The player: owns the queue and the current track, turns tracks into audio
//! and drives the audio engine, the system media overlay (SMTC), Discord and
//! the tray. The frontend only sends commands and renders `player:state`;
//! the position comes separately as `audio:tick` and never enters this state.
//!
//! One actor task processes every command in order (from the frontend, SMTC,
//! the tray, the audio tick thread). Slow work (fetching, decoding) runs in
//! spawned tasks that report back as commands tagged with a generation, so a
//! late answer for a track the user already skipped is dropped.

mod artwork;
pub mod commands;
pub mod drm;
mod hls;
mod queue;
mod store;
mod stream;
pub mod waveform;

use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use serde::Serialize;
use tauri::{Emitter, Manager};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::api::Api;
use crate::api::models::Track;
use crate::app::diagnostics;
use crate::audio::engine::{self, Prepared};
use crate::audio::state::AudioState;
use crate::audio::types::MediaCmd;
use crate::auth::AuthState;
use crate::discord::{self, Presence};
use crate::net::NetState;
use crate::rt::AppHandle;
use crate::settings::SettingsStore;
use queue::Queue;
pub use queue::Repeat;
pub use store::TrackStore;
use stream::{Audio, DrmMedia, Payload, StreamError};

const EVENT: &str = "player:state";
/// "Previous" past this point restarts the track instead.
const RESTART_THRESHOLD_SECS: f64 = 3.0;
/// Tracks remembered for "previous".
const HISTORY_LIMIT: usize = 200;
/// Next tracks listed in the queue panel.
const UPCOMING_SHOWN: usize = 50;
/// Volume is saved once the slider has been still this long.
const VOLUME_SAVE_DELAY: Duration = Duration::from_millis(800);

pub enum Command {
    /// Replace the queue with `ids` and play `ids[start]`.
    PlayTracks {
        ids: Vec<u64>,
        start: usize,
    },
    Toggle,
    Play,
    Pause,
    Next,
    Previous,
    Seek(f64),
    SeekBy(f64),
    SetVolume(f64),
    SetShuffle(bool),
    SetRepeat(Repeat),
    PlayNext(Vec<u64>),
    Append(Vec<u64>),
    /// Jump this many tracks ahead in play order (picked in the queue panel).
    SkipAhead(usize),
    /// Stop and forget the queue (sign-out).
    Reset,
    /// The audio engine ran out of samples.
    TrackEnded,
    Loaded {
        generation: u64,
        result: Result<Box<Loaded>, StreamError>,
    },
    Prefetched {
        id: u64,
        audio: Audio,
    },
    SaveVolume(u64),
    /// The DRM page reported on the track of `generation`.
    Drm {
        generation: u64,
        event: drm::PageEvent,
    },
    /// Re-send the Discord status (after its setting changed).
    RefreshPresence,
}

pub struct Loaded {
    track: Arc<Track>,
    source: Source,
    preview: bool,
}

/// Where a loaded track plays.
enum Source {
    /// Decoded for the Rust audio engine.
    Engine(Prepared),
    /// Still encrypted, for the WebView player.
    Drm(DrmMedia),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    #[default]
    Idle,
    Loading,
    Playing,
    Paused,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlayerError {
    /// DRM-only, withdrawn or region-blocked.
    Unavailable,
    Network,
    SignedOut,
}

/// What the frontend renders. No position: see the module docs.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub status: Status,
    pub track: Option<Track>,
    /// Index of the current track in the queue's list order.
    pub index: Option<usize>,
    pub queue_length: usize,
    /// The next tracks in play order, for the queue panel.
    pub upcoming: Vec<u64>,
    pub shuffle: bool,
    pub repeat: Repeat,
    pub volume: f64,
    /// Only the 30-second Go+ preview is playing.
    pub preview: bool,
    pub error: Option<PlayerError>,
}

pub struct PlayerState {
    tx: UnboundedSender<Command>,
    rx: Mutex<Option<UnboundedReceiver<Command>>>,
    snapshot: RwLock<Snapshot>,
    pub store: TrackStore,
}

impl PlayerState {
    pub fn new() -> Self {
        let (tx, rx) = unbounded_channel();
        Self {
            tx,
            rx: Mutex::new(Some(rx)),
            snapshot: RwLock::default(),
            store: TrackStore::default(),
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.read().unwrap().clone()
    }
}

/// Queue a command for the player. Callable from any thread.
pub fn send(app: &AppHandle, command: Command) {
    app.state::<PlayerState>().tx.send(command).ok();
}

/// Start the actor. Call once, after `PlayerState` is managed.
pub fn start(app: &AppHandle) {
    let rx = app
        .state::<PlayerState>()
        .rx
        .lock()
        .unwrap()
        .take()
        .expect("player started twice");
    let volume = app.state::<SettingsStore>().get().volume;
    engine::set_volume(&app.state::<AudioState>(), volume);
    let mut actor = Actor {
        app: app.clone(),
        queue: Queue::with_seed(seed()),
        status: Status::Idle,
        current: None,
        preview: false,
        error: None,
        generation: 0,
        prefetched: None,
        prefetching: None,
        unavailable_skips: 0,
        history: Vec::new(),
        last_started: None,
        going_back: false,
        volume,
        volume_generation: 0,
        drm: false,
    };
    actor.publish();
    tauri::async_runtime::spawn(actor.run(rx));
}

fn seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15)
}

struct Actor {
    app: AppHandle,
    queue: Queue,
    status: Status,
    current: Option<Arc<Track>>,
    preview: bool,
    error: Option<PlayerError>,
    /// Bumped on every load; stale `Loaded` answers are dropped.
    generation: u64,
    /// Audio of the track expected to play next.
    prefetched: Option<(u64, Audio)>,
    prefetching: Option<u64>,
    /// Unavailable tracks skipped in a row, to stop on an all-blocked queue.
    unavailable_skips: usize,
    /// Tracks that actually played, oldest first: "previous" walks back
    /// through these, whatever the queue order or shuffle did.
    history: Vec<u64>,
    last_started: Option<u64>,
    /// The load in flight was started by "previous": don't record it.
    going_back: bool,
    volume: f64,
    volume_generation: u64,
    /// The current track plays in the WebView (DRM), not the audio engine.
    drm: bool,
}

impl Actor {
    async fn run(mut self, mut rx: UnboundedReceiver<Command>) {
        while let Some(command) = rx.recv().await {
            self.handle(command);
        }
    }

    fn audio(&self) -> tauri::State<'_, AudioState> {
        self.app.state::<AudioState>()
    }

    // The current track's playback, wherever it runs.

    fn live(&self) -> bool {
        if self.drm {
            drm::is_live(&self.app)
        } else {
            engine::has_live_track(&self.audio())
        }
    }

    fn position(&self) -> f64 {
        if self.drm {
            drm::position(&self.app)
        } else {
            engine::position(&self.audio())
        }
    }

    fn resume(&self) {
        if self.drm {
            drm::play(&self.app);
        } else {
            engine::play(&self.audio());
        }
    }

    fn halt(&self) {
        if self.drm {
            drm::pause(&self.app);
        } else {
            engine::pause(&self.audio());
        }
    }

    fn seek_to(&self, seconds: f64) -> Result<(), String> {
        if self.drm {
            drm::seek(&self.app, seconds);
            Ok(())
        } else {
            engine::seek(&self.audio(), seconds)
        }
    }

    /// Stop whatever plays (both backends).
    fn stop_all(&mut self) {
        engine::stop(&self.audio());
        if self.drm {
            drm::release(&self.app);
            self.drm = false;
        }
    }

    fn handle(&mut self, command: Command) {
        match command {
            Command::PlayTracks { ids, start } => {
                self.queue.replace(ids, start);
                self.unavailable_skips = 0;
                self.load_current();
            }
            Command::Toggle => match self.status {
                Status::Playing => self.pause(),
                Status::Paused | Status::Idle => self.play(),
                Status::Loading => {}
            },
            Command::Play => self.play(),
            Command::Pause => self.pause(),
            Command::Next => {
                self.unavailable_skips = 0;
                if self.queue.skip_forward().is_some() {
                    self.load_current();
                }
            }
            Command::Previous => self.previous(),
            Command::Seek(seconds) => self.seek(seconds),
            Command::SeekBy(delta) => {
                let target = self.position() + delta;
                self.seek(target.max(0.0));
            }
            Command::SetVolume(percent) => self.set_volume(percent),
            Command::SaveVolume(generation) => {
                if generation == self.volume_generation {
                    let volume = self.volume;
                    let saved = self
                        .app
                        .state::<SettingsStore>()
                        .update(|settings| settings.volume = volume);
                    if let Err(error) = saved {
                        diagnostics::warn(format!("[player] {error}"));
                    }
                }
            }
            Command::SetShuffle(on) => {
                self.queue.set_shuffle(on);
                self.after_queue_change();
            }
            Command::SetRepeat(repeat) => {
                self.queue.set_repeat(repeat);
                self.after_queue_change();
            }
            Command::PlayNext(ids) => {
                self.queue.play_next(&ids);
                self.after_queue_change();
            }
            Command::Append(ids) => {
                let was_empty = self.queue.current().is_none();
                self.queue.append(&ids);
                if was_empty {
                    self.load_current();
                } else {
                    self.after_queue_change();
                }
            }
            Command::SkipAhead(steps) => {
                self.unavailable_skips = 0;
                if self.queue.skip_ahead(steps).is_some() {
                    self.load_current();
                }
            }
            Command::Reset => self.reset(),
            Command::RefreshPresence => self.update_discord(),
            Command::TrackEnded => self.track_ended(),
            Command::Loaded { generation, result } => {
                if generation == self.generation {
                    self.loaded(result);
                }
            }
            Command::Drm { generation, event } => {
                if generation == self.generation && self.drm {
                    match event {
                        drm::PageEvent::Ended => self.track_ended(),
                        drm::PageEvent::Failed(message) => {
                            diagnostics::warn(format!("[player] DRM playback: {message}"));
                            self.stop_all();
                            self.load_failed(StreamError::Unavailable);
                        }
                    }
                }
            }
            Command::Prefetched { id, audio } => {
                if self.prefetching == Some(id) {
                    self.prefetching = None;
                }
                if self.queue.peek_after_end() == Some(id) {
                    self.prefetched = Some((id, audio));
                }
            }
        }
    }

    fn play(&mut self) {
        if self.live() {
            self.resume();
            self.set_status(Status::Playing);
        } else if self.queue.current().is_some() {
            // Ended, failed or never started: (re)load from the start.
            self.load_current();
        }
    }

    fn pause(&mut self) {
        if self.status == Status::Playing {
            self.halt();
            self.set_status(Status::Paused);
        }
    }

    fn previous(&mut self) {
        let position = self.position();
        if position > RESTART_THRESHOLD_SECS {
            self.seek(0.0);
            return;
        }
        self.unavailable_skips = 0;
        if let Some(id) = self.history.pop() {
            self.queue.focus(id);
            self.going_back = true;
            self.load_current();
            return;
        }
        let before = self.queue.current();
        match self.queue.skip_back() {
            Some(id) if Some(id) != before || !self.live() => self.load_current(),
            Some(_) => self.seek(0.0),
            None => {}
        }
    }

    fn seek(&mut self, seconds: f64) {
        if !self.live() {
            return;
        }
        if let Err(error) = self.seek_to(seconds) {
            diagnostics::warn(format!("[player] seek: {error}"));
        }
        engine::send_media(&self.audio(), MediaCmd::SetPosition(seconds));
        self.update_discord();
    }

    fn set_volume(&mut self, percent: f64) {
        self.volume = percent.clamp(0.0, 100.0);
        engine::set_volume(&self.audio(), self.volume);
        drm::set_volume(&self.app, self.volume);
        self.volume_generation += 1;
        let generation = self.volume_generation;
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(VOLUME_SAVE_DELAY).await;
            send(&app, Command::SaveVolume(generation));
        });
        self.publish();
    }

    fn reset(&mut self) {
        self.generation += 1;
        self.queue.replace(Vec::new(), 0);
        self.history.clear();
        self.last_started = None;
        self.current = None;
        self.prefetched = None;
        self.error = None;
        self.preview = false;
        self.stop_all();
        self.set_status(Status::Idle);
    }

    fn track_ended(&mut self) {
        if self.queue.repeat() == Repeat::One && self.seek_to(0.0).is_ok() {
            self.resume();
            self.update_discord();
            return;
        }
        self.unavailable_skips = 0;
        match self.queue.advance_after_end() {
            Some(_) => self.load_current(),
            None => self.set_status(Status::Paused),
        }
    }

    fn after_queue_change(&mut self) {
        if self
            .prefetched
            .as_ref()
            .is_some_and(|(id, _)| self.queue.peek_after_end() != Some(*id))
        {
            self.prefetched = None;
        }
        if matches!(self.status, Status::Playing | Status::Paused) {
            self.prefetch_next();
        }
        self.publish();
    }

    /// Stop what plays and start loading the queue's current track.
    fn load_current(&mut self) {
        self.generation += 1;
        let Some(id) = self.queue.current() else {
            self.reset();
            return;
        };
        self.stop_all();
        let store = &self.app.state::<PlayerState>().store;
        self.current = store.get(id);
        self.error = None;
        self.preview = false;
        self.set_status(Status::Loading);

        let prefetched = match self.prefetched.take() {
            Some((prefetched_id, audio)) if prefetched_id == id => Some(audio),
            _ => None,
        };
        let app = self.app.clone();
        let generation = self.generation;
        tauri::async_runtime::spawn(async move {
            let result = load(&app, id, prefetched).await.map(Box::new);
            send(&app, Command::Loaded { generation, result });
        });
    }

    fn loaded(&mut self, result: Result<Box<Loaded>, StreamError>) {
        let loaded = match result {
            Ok(loaded) => loaded,
            Err(error) => return self.load_failed(error),
        };
        let Loaded {
            track,
            source,
            preview,
        } = *loaded;
        match source {
            Source::Engine(prepared) => engine::commit(&self.audio(), prepared, true),
            Source::Drm(media) => {
                diagnostics::log(
                    "INFO",
                    format!(
                        "[player] track {} plays via DRM ({} segments)",
                        track.id,
                        media.segments.len()
                    ),
                );
                drm::load(&self.app, self.generation, media, self.volume);
                self.drm = true;
            }
        }
        self.unavailable_skips = 0;
        self.remember_started(track.id);
        self.current = Some(track.clone());
        self.preview = preview;
        self.set_status(Status::Playing);
        self.announce(&track);
        self.prefetch_next();
    }

    fn remember_started(&mut self, id: u64) {
        let going_back = std::mem::take(&mut self.going_back);
        if let Some(previous) = self.last_started.replace(id)
            && previous != id
            && !going_back
            && self.history.last() != Some(&previous)
        {
            self.history.push(previous);
            if self.history.len() > HISTORY_LIMIT {
                self.history.remove(0);
            }
        }
    }

    fn load_failed(&mut self, error: StreamError) {
        self.going_back = false;
        let id = self.queue.current();
        diagnostics::warn(format!("[player] track {id:?} failed: {error}"));
        match error {
            StreamError::Unavailable => {
                self.error = Some(PlayerError::Unavailable);
                self.unavailable_skips += 1;
                if self.unavailable_skips < self.queue.len() && self.queue.skip_forward().is_some()
                {
                    return self.load_current();
                }
            }
            StreamError::Unauthorized => {
                self.error = Some(PlayerError::SignedOut);
                crate::auth::verify(&self.app);
            }
            StreamError::Expired | StreamError::Network(_) => {
                self.error = Some(PlayerError::Network);
            }
        }
        self.set_status(Status::Paused);
    }

    /// Tell the system, Discord and SoundCloud that `track` started.
    fn announce(&self, track: &Arc<Track>) {
        let title = track.title.clone();
        let artist = artist_of(track);
        let duration_secs = track.duration as f64 / 1000.0;
        let cover = artwork::of(track);
        engine::send_media(
            &self.audio(),
            MediaCmd::SetMetadata {
                title: title.clone(),
                artist: artist.clone(),
                cover_url: None,
                duration_secs,
            },
        );
        engine::send_media(&self.audio(), MediaCmd::SetPlaying(true));
        crate::app::tray::set_now_playing(&self.app, Some(&format!("{artist} — {title}")));

        let app = self.app.clone();
        let id = track.id;
        let urn = track.urn.clone();
        tauri::async_runtime::spawn(async move {
            let http = app.state::<NetState>().client();
            if let Some(token) = app.state::<AuthState>().token()
                && let Err(error) = app.state::<Api>().record_play(&http, &token, &urn).await
            {
                diagnostics::warn(format!("[player] play-history: {error}"));
            }
            let Some(cover) = cover else { return };
            let Ok(dir) = app.path().app_cache_dir() else {
                return;
            };
            if let Some(file) = artwork::cover_file(&http, &cover, &dir.join("smtc"), id).await {
                engine::send_media(
                    &app.state::<AudioState>(),
                    MediaCmd::SetMetadata {
                        title,
                        artist,
                        cover_url: Some(format!("file://{}", file.display())),
                        duration_secs,
                    },
                );
            }
        });
    }

    fn prefetch_next(&mut self) {
        let Some(next) = self.queue.peek_after_end() else {
            return;
        };
        let current = self.current.as_ref().map(|t| t.id);
        let have = self.prefetched.as_ref().map(|(id, _)| *id);
        if Some(next) == current || Some(next) == have || Some(next) == self.prefetching {
            return;
        }
        self.prefetching = Some(next);
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move {
            match fetch_audio(&app, next).await {
                Ok((_, audio)) => send(&app, Command::Prefetched { id: next, audio }),
                Err(error) => {
                    diagnostics::log("INFO", format!("[player] prefetch {next}: {error}"))
                }
            }
        });
    }

    fn set_status(&mut self, status: Status) {
        self.status = status;
        if matches!(status, Status::Playing | Status::Paused) {
            engine::send_media(
                &self.audio(),
                MediaCmd::SetPlaying(status == Status::Playing),
            );
        }
        if status == Status::Idle {
            crate::app::tray::set_now_playing(&self.app, None);
        }
        self.update_discord();
        self.publish();
    }

    fn update_discord(&self) {
        let presence = match (&self.current, self.status) {
            (Some(track), Status::Playing | Status::Paused) => Some(Presence {
                title: track.title.clone(),
                artist: artist_of(track),
                artwork_url: artwork::of(track),
                track_url: Some(track.permalink_url.clone()).filter(|url| !url.is_empty()),
                duration_secs: Some((track.duration / 1000) as i64),
                elapsed_secs: self.position() as i64,
                playing: self.status == Status::Playing,
            }),
            _ => None,
        };
        discord::update(&self.app, presence);
    }

    fn publish(&mut self) {
        let snapshot = Snapshot {
            status: self.status,
            track: self.current.as_deref().cloned(),
            index: self.queue.index(),
            queue_length: self.queue.len(),
            upcoming: self.queue.upcoming(UPCOMING_SHOWN),
            shuffle: self.queue.shuffle(),
            repeat: self.queue.repeat(),
            volume: self.volume,
            preview: self.preview,
            error: self.error,
        };
        *self.app.state::<PlayerState>().snapshot.write().unwrap() = snapshot.clone();
        self.app.emit(EVENT, snapshot).ok();
    }
}

fn artist_of(track: &Track) -> String {
    track
        .user
        .as_ref()
        .map(|user| user.username.clone())
        .unwrap_or_default()
}

/// Fetch a track's audio; on stale signatures refetch the track and retry once.
async fn fetch_audio(app: &AppHandle, id: u64) -> Result<(Arc<Track>, Audio), StreamError> {
    let token = app
        .state::<AuthState>()
        .token()
        .ok_or(StreamError::Unauthorized)?;
    let http = app.state::<NetState>().client();
    let api = app.state::<Api>();
    let store = &app.state::<PlayerState>().store;
    let mut force = false;
    loop {
        let track = store
            .load(&api, &http, &token, id, force)
            .await?
            .ok_or(StreamError::Unavailable)?;
        match stream::fetch(&api, &http, &token, &track).await {
            Err(StreamError::Expired) if !force => force = true,
            Err(StreamError::Expired) => return Err(StreamError::Unavailable),
            other => return other.map(|audio| (track, audio)),
        }
    }
}

async fn load(app: &AppHandle, id: u64, prefetched: Option<Audio>) -> Result<Loaded, StreamError> {
    let (track, audio) = match prefetched {
        Some(audio) => {
            let track = app
                .state::<PlayerState>()
                .store
                .get(id)
                .ok_or(StreamError::Unavailable)?;
            (track, audio)
        }
        None => fetch_audio(app, id).await?,
    };
    let preview = audio.preview;
    let bytes = match audio.payload {
        Payload::Plain(bytes) => bytes,
        Payload::Drm(media) => {
            return Ok(Loaded {
                track,
                source: Source::Drm(media),
                preview,
            });
        }
    };
    let cache = app
        .path()
        .app_cache_dir()
        .ok()
        .map(|dir| (dir.join("audio-normalization"), track.urn.clone()));
    let duration = Some(Duration::from_millis(track.duration));
    let decoder = app.clone();
    let prepared = tokio::task::spawn_blocking(move || {
        engine::prepare(&decoder.state::<AudioState>(), bytes, cache, duration)
    })
    .await
    .map_err(|e| StreamError::Network(e.to_string()))?
    .map_err(|error| {
        diagnostics::warn(format!("[player] decode track {id}: {error}"));
        StreamError::Unavailable
    })?;
    Ok(Loaded {
        track,
        source: Source::Engine(prepared),
        preview,
    })
}
