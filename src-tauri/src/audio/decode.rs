//! Decoding and loudness normalization. Input is the fragmented MP4 (AAC)
//! that `player::hls` assembles; symphonia decodes and seeks it in memory.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rodio::mixer::Mixer;
use rodio::source::SeekError;
use rodio::{Decoder, Player, Source};
use sha2::{Digest, Sha256};

use crate::audio::declick::DeclickSource;
use crate::audio::types::{
    ChannelCount, NORMALIZATION_ANALYSIS_SAMPLES, NORMALIZATION_BLOCK_SAMPLES,
    NORMALIZATION_MAX_ATTENUATION_DB, NORMALIZATION_MAX_BOOST_DB,
    NORMALIZATION_SEEK_PREROLL_SAMPLES, NORMALIZATION_SPREAD_MIN_SECS, NORMALIZATION_TARGET_PEAK,
    NORMALIZATION_TARGET_RMS, NORMALIZATION_WINDOWS, SampleRate,
};

const NORMALIZATION_CACHE_VERSION: u8 = 3;

fn decode_bytes(bytes: &[u8]) -> Result<Decoder<Cursor<Vec<u8>>>, String> {
    Decoder::builder()
        .with_data(Cursor::new(bytes.to_vec()))
        .with_byte_len(bytes.len() as u64)
        .with_seekable(true)
        .build()
        .map_err(|e| format!("Failed to decode: {e}"))
}

pub fn analysis_cache_file(cache_dir: &Path, cache_key: &str, extension: &str) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(cache_key.as_bytes());
    let hash = hex::encode(hasher.finalize());
    cache_dir.join(format!("{hash}.{extension}"))
}

fn read_cached_normalization_gain(
    cache_dir: Option<&Path>,
    cache_key: Option<&str>,
) -> Option<f32> {
    let path = analysis_cache_file(cache_dir?, cache_key?, "gain");
    let raw = std::fs::read_to_string(path).ok()?;
    let (version, value) = raw.trim().split_once(':')?;
    if version != NORMALIZATION_CACHE_VERSION.to_string() {
        return None;
    }
    value.parse::<f32>().ok()
}

fn write_cached_normalization_gain(cache_dir: Option<&Path>, cache_key: Option<&str>, gain: f32) {
    let Some(cache_dir) = cache_dir else {
        return;
    };
    let Some(cache_key) = cache_key else {
        return;
    };

    if std::fs::create_dir_all(cache_dir).is_err() {
        return;
    }

    let path = analysis_cache_file(cache_dir, cache_key, "gain");
    let _ = std::fs::write(path, format!("{NORMALIZATION_CACHE_VERSION}:{gain:.6}"));
}

fn spread_samples<S: Source<Item = f32>>(
    mut source: S,
    fallback_duration: Option<Duration>,
) -> impl Iterator<Item = f32> {
    let total = source
        .total_duration()
        .or(fallback_duration)
        .unwrap_or_default();
    let spread = total.as_secs() >= NORMALIZATION_SPREAD_MIN_SECS;
    let window = NORMALIZATION_ANALYSIS_SAMPLES / NORMALIZATION_WINDOWS;
    let mut index = 0;
    let mut left = 0;
    std::iter::from_fn(move || {
        if left == 0 {
            if index == NORMALIZATION_WINDOWS {
                return None;
            }
            let at = total.mul_f64((index as f64 + 0.5) / NORMALIZATION_WINDOWS as f64);
            if spread && source.try_seek(at).is_ok() {
                source
                    .by_ref()
                    .take(NORMALIZATION_SEEK_PREROLL_SAMPLES)
                    .for_each(drop);
            }
            index += 1;
            left = window;
        }
        left -= 1;
        source.next()
    })
}

fn normalization_gain_from_samples<S: Source<Item = f32>>(
    source: S,
    fallback_duration: Option<Duration>,
) -> f32 {
    let mut peak = 0.0f64;
    let mut count = 0usize;
    let mut block_sum_sq = 0.0f64;
    let mut block_count = 0usize;
    let mut block_powers = Vec::new();

    for sample in spread_samples(source, fallback_duration) {
        let value = sample as f64;
        let abs = value.abs();
        peak = peak.max(abs);
        block_sum_sq += value * value;
        block_count += 1;
        count += 1;

        if block_count >= NORMALIZATION_BLOCK_SAMPLES {
            block_powers.push(block_sum_sq / block_count as f64);
            block_sum_sq = 0.0;
            block_count = 0;
        }
    }

    if block_count > 0 {
        block_powers.push(block_sum_sq / block_count as f64);
    }

    if count == 0 || block_powers.is_empty() {
        return 1.0;
    }

    block_powers.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let keep_from = ((block_powers.len() as f64) * 0.4).floor() as usize;
    let kept = &block_powers[keep_from.min(block_powers.len().saturating_sub(1))..];
    let gated_power = kept.iter().copied().sum::<f64>() / kept.len() as f64;
    let rms = gated_power.sqrt().max(1e-6);
    let target_gain = NORMALIZATION_TARGET_RMS / rms;
    let peak_safe_gain = if peak > 0.0 {
        NORMALIZATION_TARGET_PEAK / peak
    } else {
        target_gain
    };

    let max_boost = 10f64.powf(NORMALIZATION_MAX_BOOST_DB / 20.0);
    let max_attenuation = 10f64.powf(NORMALIZATION_MAX_ATTENUATION_DB / 20.0);
    let gain = target_gain
        .min(peak_safe_gain)
        .clamp(max_attenuation, max_boost);

    if (gain - 1.0).abs() < 0.05 {
        1.0
    } else {
        gain as f32
    }
}

/// Loudness gain for `bytes`. fMP4 carries no total duration, so
/// `duration` (from the API) places the analysis windows.
pub fn resolve_normalization_gain(
    bytes: &[u8],
    cache_dir: Option<&Path>,
    cache_key: Option<&str>,
    duration: Option<Duration>,
) -> Result<f32, String> {
    if let Some(gain) = read_cached_normalization_gain(cache_dir, cache_key) {
        return Ok(gain);
    }
    let gain = normalization_gain_from_samples(decode_bytes(bytes)?, duration);
    write_cached_normalization_gain(cache_dir, cache_key, gain);
    Ok(gain)
}

/// Applies the loudness-normalization gain, clamped to avoid clipping.
struct GainSource<S: Source<Item = f32>> {
    source: S,
    gain: f32,
}

impl<S: Source<Item = f32>> Iterator for GainSource<S> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        self.source
            .next()
            .map(|sample| (sample * self.gain).clamp(-1.0, 1.0))
    }
}

impl<S: Source<Item = f32>> Source for GainSource<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.source.current_span_len()
    }

    fn channels(&self) -> ChannelCount {
        self.source.channels()
    }

    fn sample_rate(&self) -> SampleRate {
        self.source.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.source.total_duration()
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.source.try_seek(pos)
    }
}

pub fn create_player_from_bytes(
    bytes: &[u8],
    mixer: &Mixer,
    volume: f32,
    normalization_gain: f32,
    start_paused: bool,
) -> Result<Player, String> {
    let player = Player::connect_new(mixer);
    player.set_volume(volume);
    if start_paused {
        player.pause();
    }
    player.append(DeclickSource::new(GainSource {
        source: decode_bytes(bytes)?,
        gain: normalization_gain,
    }));
    Ok(player)
}
