//! The loudness of the recording in progress, for the window's waveform (plan PL6). Only a
//! number between 0 and 1 is kept; no audio.

use std::sync::atomic::{AtomicU32, Ordering};

static LEVEL: AtomicU32 = AtomicU32::new(0);

/// Loudness of `samples` from 0 to 1 (RMS, scaled so ordinary speech reaches about the middle).
pub fn loudness(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
    (rms * 6.0).sqrt().min(1.0)
}

/// Called for each captured chunk; the loudest recent chunk wins until it is read.
pub fn record(samples: &[f32]) {
    let level = loudness(samples);
    let _ = LEVEL.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| (level > f32::from_bits(old)).then_some(level.to_bits()));
}

/// The level since the last read, then starts again from silence.
pub fn take() -> f32 {
    f32::from_bits(LEVEL.swap(0, Ordering::Relaxed))
}

#[tauri::command]
pub fn api_recording_level() -> f32 {
    take()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_is_zero_and_loud_is_capped() {
        assert_eq!(loudness(&[]), 0.0);
        assert_eq!(loudness(&[0.0; 480]), 0.0);
        assert_eq!(loudness(&[1.0; 480]), 1.0);
        let speech = loudness(&vec![0.05; 480]);
        assert!((0.3..0.8).contains(&speech), "{speech}");
    }

    #[test]
    fn the_loudest_chunk_wins_until_read() {
        take();
        record(&[0.01; 480]);
        record(&[0.2; 480]);
        record(&[0.01; 480]);
        assert!((take() - loudness(&[0.2; 480])).abs() < 1e-6);
        assert_eq!(take(), 0.0);
    }
}
