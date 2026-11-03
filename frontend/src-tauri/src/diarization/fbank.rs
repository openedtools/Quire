//! Kaldi-compatible log-mel filterbank features (80 bins, 25 ms window, 10 ms hop).
//!
//! The speaker model was trained on features from Kaldi/torchaudio `fbank`, so the settings and
//! arithmetic follow that implementation: hamming window, 0.97 pre-emphasis, per-frame DC removal,
//! 512-point FFT of the zero-padded frame, power spectrum, triangular mel filters from 20 Hz to
//! Nyquist, natural log floored at f32 epsilon, no dither, and "snip edges" framing (only complete
//! frames). Samples are scaled to the 16-bit range first, as the model's metadata asks.

use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};
use std::sync::Arc;

pub const SAMPLE_RATE: usize = 16_000;
pub const NUM_MELS: usize = 80;
const FRAME_LEN: usize = 400; // 25 ms
const FRAME_SHIFT: usize = 160; // 10 ms
const FFT_SIZE: usize = 512; // frame length rounded up to a power of two
const PREEMPHASIS: f32 = 0.97;
const LOW_FREQ: f64 = 20.0;
const LOG_FLOOR: f32 = f32::EPSILON;
/// Scale from [-1, 1] floats to the 16-bit integer range the model expects.
const INT16_SCALE: f32 = 32768.0;

fn mel(freq: f64) -> f64 {
    1127.0 * (1.0 + freq / 700.0).ln()
}

/// Number of complete frames in `num_samples` samples.
pub fn num_frames(num_samples: usize) -> usize {
    if num_samples < FRAME_LEN {
        0
    } else {
        1 + (num_samples - FRAME_LEN) / FRAME_SHIFT
    }
}

pub struct Fbank {
    window: Vec<f32>,
    /// For each mel bin: first FFT bin it covers and its weights from there.
    mel_filters: Vec<(usize, Vec<f32>)>,
    fft: Arc<dyn RealToComplex<f32>>,
}

impl Default for Fbank {
    fn default() -> Self {
        Self::new()
    }
}

impl Fbank {
    pub fn new() -> Self {
        let a = 2.0 * std::f64::consts::PI / (FRAME_LEN as f64 - 1.0);
        let window = (0..FRAME_LEN)
            .map(|i| (0.54 - 0.46 * (a * i as f64).cos()) as f32)
            .collect();

        // Kaldi uses only FFT bins below Nyquist for the mel filters.
        let num_fft_bins = FFT_SIZE / 2;
        let fft_bin_width = SAMPLE_RATE as f64 / FFT_SIZE as f64;
        let high_freq = SAMPLE_RATE as f64 / 2.0;
        let (mel_low, mel_high) = (mel(LOW_FREQ), mel(high_freq));
        let mel_delta = (mel_high - mel_low) / (NUM_MELS as f64 + 1.0);

        let mel_filters = (0..NUM_MELS)
            .map(|bin| {
                let left = mel_low + bin as f64 * mel_delta;
                let center = left + mel_delta;
                let right = center + mel_delta;
                let mut first = None;
                let mut weights = Vec::new();
                for i in 0..num_fft_bins {
                    let m = mel(fft_bin_width * i as f64);
                    if m > left && m < right {
                        let w = if m <= center {
                            (m - left) / (center - left)
                        } else {
                            (right - m) / (right - center)
                        };
                        first.get_or_insert(i);
                        weights.push(w as f32);
                    }
                }
                (first.unwrap_or(0), weights)
            })
            .collect();

        Self {
            window,
            mel_filters,
            fft: RealFftPlanner::<f32>::new().plan_fft_forward(FFT_SIZE),
        }
    }

    /// Features for 16 kHz mono samples in [-1, 1]; row-major `num_frames x 80`.
    pub fn compute(&self, samples: &[f32]) -> Vec<f32> {
        let frames = num_frames(samples.len());
        let mut features = Vec::with_capacity(frames * NUM_MELS);

        let mut input = vec![0.0f32; FFT_SIZE];
        let mut spectrum = vec![Complex::new(0.0f32, 0.0f32); FFT_SIZE / 2 + 1];
        let mut power = vec![0.0f32; FFT_SIZE / 2 + 1];
        let mut frame = [0.0f32; FRAME_LEN];

        for f in 0..frames {
            let start = f * FRAME_SHIFT;
            for (dst, src) in frame.iter_mut().zip(&samples[start..start + FRAME_LEN]) {
                *dst = src * INT16_SCALE;
            }

            // Remove the frame's DC offset, then pre-emphasise (in place, back to front).
            let mean = frame.iter().sum::<f32>() / FRAME_LEN as f32;
            frame.iter_mut().for_each(|s| *s -= mean);
            for i in (1..FRAME_LEN).rev() {
                frame[i] -= PREEMPHASIS * frame[i - 1];
            }
            frame[0] -= PREEMPHASIS * frame[0];

            for ((dst, s), w) in input.iter_mut().zip(&frame).zip(&self.window) {
                *dst = s * w;
            }
            input[FRAME_LEN..].fill(0.0);

            self.fft
                .process(&mut input, &mut spectrum)
                .expect("FFT buffer sizes are fixed");
            for (p, c) in power.iter_mut().zip(&spectrum) {
                *p = c.re * c.re + c.im * c.im;
            }

            for (first, weights) in &self.mel_filters {
                let energy: f32 = weights.iter().zip(&power[*first..]).map(|(w, p)| w * p).sum();
                features.push(energy.max(LOG_FLOOR).ln());
            }
        }
        features
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_count_matches_kaldi_snip_edges() {
        assert_eq!(num_frames(399), 0);
        assert_eq!(num_frames(400), 1);
        assert_eq!(num_frames(559), 1);
        assert_eq!(num_frames(560), 2);
        assert_eq!(num_frames(16_000), 98);
    }

    #[test]
    fn silence_floors_to_the_log_epsilon() {
        let features = Fbank::new().compute(&vec![0.0; 1600]);
        assert_eq!(features.len(), num_frames(1600) * NUM_MELS);
        assert!(features.iter().all(|&v| (v - LOG_FLOOR.ln()).abs() < 1e-6));
    }

    #[test]
    fn a_pure_tone_lights_up_the_matching_mel_bin() {
        // 1 kHz sine: the strongest mel bin should sit where mel(1000 Hz) falls on the scale
        let tone: Vec<f32> = (0..8000)
            .map(|n| 0.5 * (2.0 * std::f32::consts::PI * 1000.0 * n as f32 / 16000.0).sin())
            .collect();
        let features = Fbank::new().compute(&tone);
        let frame = &features[10 * NUM_MELS..11 * NUM_MELS];
        let peak = frame.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap().0;
        let expected = ((mel(1000.0) - mel(LOW_FREQ)) / ((mel(8000.0) - mel(LOW_FREQ)) / (NUM_MELS as f64 + 1.0)) - 1.0).round() as usize;
        assert!(peak.abs_diff(expected) <= 1, "peak bin {peak}, expected about {expected}");
    }
}
