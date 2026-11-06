//! Track waveforms (`waveform_url` → `wave.sndcdn.com/<id>_m.json`), reduced
//! to a few bars for the mini-player. Kept in memory for recent tracks.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use serde::Deserialize;
use tauri::{Manager, State};

use crate::net::NetState;
use crate::rt::AppHandle;
use crate::shared::hosts;

const KEEP: usize = 32;
const MAX_BARS: usize = 400;

#[derive(Deserialize)]
struct Waveform {
    #[serde(default)]
    height: u32,
    samples: Vec<u32>,
}

#[derive(Default)]
pub struct WaveformCache(Mutex<VecDeque<(String, Vec<u32>, u32)>>);

/// Peaks per bar, scaled to 0–1 against the waveform's height.
fn bars(samples: &[u32], height: u32, count: usize) -> Vec<f32> {
    if samples.is_empty() || count == 0 {
        return Vec::new();
    }
    let height = height.max(*samples.iter().max().unwrap_or(&1)).max(1) as f32;
    (0..count)
        .map(|i| {
            let start = i * samples.len() / count;
            let end = ((i + 1) * samples.len() / count)
                .max(start + 1)
                .min(samples.len());
            samples[start..end].iter().copied().max().unwrap_or(0) as f32 / height
        })
        .collect()
}

/// `count` bars for the waveform at `url` (a SoundCloud CDN JSON).
#[tauri::command]
pub async fn player_waveform(
    app: AppHandle,
    cache: State<'_, WaveformCache>,
    url: String,
    count: usize,
) -> Result<Vec<f32>, String> {
    let count = count.min(MAX_BARS);
    let known = cache
        .0
        .lock()
        .unwrap()
        .iter()
        .find(|(u, _, _)| *u == url)
        .map(|(_, samples, height)| bars(samples, *height, count));
    if let Some(bars) = known {
        return Ok(bars);
    }
    let target = hosts::media_url(&url)?;
    let waveform: Waveform = app
        .state::<NetState>()
        .client()
        .get(target)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| e.without_url().to_string())?
        .json()
        .await
        .map_err(|e| e.without_url().to_string())?;
    let result = bars(&waveform.samples, waveform.height, count);
    let mut cached = cache.0.lock().unwrap();
    cached.push_front((url, waveform.samples, waveform.height));
    cached.truncate(KEEP);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bars_take_the_peak_of_each_slice() {
        let samples = [10, 20, 140, 0, 70, 70];
        assert_eq!(bars(&samples, 140, 3), [20.0 / 140.0, 1.0, 0.5]);
        assert_eq!(bars(&samples, 140, 6).len(), 6);
        assert!(bars(&[], 140, 10).is_empty());
    }

    #[test]
    fn more_bars_than_samples_repeats_samples() {
        assert_eq!(bars(&[140, 70], 140, 4), [1.0, 1.0, 0.5, 0.5]);
    }
}
