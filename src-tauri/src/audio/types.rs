use std::num::NonZero;
use std::sync::mpsc::Sender;
use std::time::Duration;

use rodio::mixer::Mixer;

pub const NORMALIZATION_ANALYSIS_SAMPLES: usize = 48_000 * 2 * 30;
pub const NORMALIZATION_BLOCK_SAMPLES: usize = 48_000 * 2 / 2;
pub const NORMALIZATION_WINDOWS: usize = 10;
pub const NORMALIZATION_SPREAD_MIN_SECS: u64 = 60;
pub const NORMALIZATION_SEEK_PREROLL_SAMPLES: usize = 48_000 * 2 / 10;
pub const NORMALIZATION_TARGET_RMS: f64 = 0.14;
pub const NORMALIZATION_TARGET_PEAK: f64 = 0.95;
pub const NORMALIZATION_MAX_BOOST_DB: f64 = 9.0;
pub const NORMALIZATION_MAX_ATTENUATION_DB: f64 = -8.0;
pub const TICK_INTERVAL_MS: u64 = 100;
pub const HIDDEN_TICK_INTERVAL: Duration = Duration::from_millis(500);

pub type ChannelCount = NonZero<u16>;
pub type SampleRate = NonZero<u32>;

pub fn frames_in(duration: Duration, sample_rate: SampleRate) -> usize {
    (duration.as_secs_f64() * sample_rate.get() as f64) as usize
}

// Every variant sets state on the system media controller (SMTC); the shared
// `Set` prefix reflects that purpose.
#[allow(clippy::enum_variant_names)]
pub enum MediaCmd {
    SetMetadata {
        title: String,
        artist: String,
        cover_url: Option<String>,
        duration_secs: f64,
    },
    SetPlaying(bool),
    SetPosition(f64),
}

pub enum AudioThreadCmd {
    SwitchDevice {
        name: Option<String>,
        reply: Sender<Result<Mixer, String>>,
    },
    Reconnect,
}

#[derive(serde::Serialize, Clone)]
pub struct AudioSink {
    pub name: String,
    pub description: String,
    pub is_default: bool,
    pub is_active: bool,
}

pub const STALL_THRESHOLD_MS: u64 = 2_000;
pub const STALL_COOLDOWN_MS: u64 = 10_000;
/// After a device switch/reconnect the new output can take a moment to start
/// pulling samples (Bluetooth especially); suppress stall-detection for this long
/// so the settling gap isn't mistaken for a dead stream.
pub const STALL_SUPPRESS_MS: u64 = 4_000;
