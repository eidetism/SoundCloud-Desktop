use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};

use rodio::Player;
use rodio::mixer::Mixer;

use crate::audio::output::{OutputHandles, spawn_output_thread};
use crate::audio::types::{AudioThreadCmd, MediaCmd};
use crate::rt::AppHandle;

pub struct AudioState {
    pub player: Mutex<Option<Player>>,
    pub mixer: Arc<Mutex<Mixer>>,
    pub normalization_enabled: AtomicBool,
    pub normalization_gain: Mutex<f32>,
    pub volume: Mutex<f32>,
    pub has_track: AtomicBool,
    pub ended_notified: AtomicBool,
    pub suppress_ended_until_ms: AtomicU64,
    /// Wall-clock ms until which the tick thread skips stall-detection — set around
    /// device switch/reconnect so a settling output isn't read as a dead stream.
    pub suppress_stall_until_ms: AtomicU64,
    pub device_error: Arc<AtomicBool>,
    pub device_reconnected: Arc<AtomicBool>,
    pub load_gen: AtomicU64,
    pub media_tx: Mutex<Option<std::sync::mpsc::Sender<MediaCmd>>>,
    pub audio_tx: std::sync::mpsc::Sender<AudioThreadCmd>,
    pub source_bytes: Mutex<Option<Vec<u8>>>,
    pub follow_default_output: Arc<AtomicBool>,
    pub active_output: Arc<Mutex<Option<String>>>,
    pub last_known_default_output: Mutex<Option<String>>,
}

pub fn init(app: &AppHandle) -> AudioState {
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<AudioThreadCmd>();
    let device_error_flag = Arc::new(AtomicBool::new(false));
    let reconnected_flag = Arc::new(AtomicBool::new(false));
    let follow_default_output = Arc::new(AtomicBool::new(true));
    let active_output = Arc::new(Mutex::new(None));

    let shared_mixer = spawn_output_thread(
        OutputHandles {
            app: app.clone(),
            cmd_tx: cmd_tx.clone(),
            error_flag: device_error_flag.clone(),
            reconnected: reconnected_flag.clone(),
            follow_default_output: follow_default_output.clone(),
            active_output: active_output.clone(),
        },
        cmd_rx,
    );

    AudioState {
        player: Mutex::new(None),
        mixer: shared_mixer,
        normalization_enabled: AtomicBool::new(true),
        normalization_gain: Mutex::new(1.0),
        volume: Mutex::new(0.25),
        has_track: AtomicBool::new(false),
        ended_notified: AtomicBool::new(false),
        suppress_ended_until_ms: AtomicU64::new(0),
        suppress_stall_until_ms: AtomicU64::new(0),
        device_error: device_error_flag,
        device_reconnected: reconnected_flag,
        load_gen: AtomicU64::new(0),
        media_tx: Mutex::new(None),
        audio_tx: cmd_tx,
        source_bytes: Mutex::new(None),
        follow_default_output,
        active_output,
        last_known_default_output: Mutex::new(None),
    }
}
