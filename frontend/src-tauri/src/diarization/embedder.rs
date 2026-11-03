//! Runs the speaker-embedding model (WeSpeaker ResNet34, ONNX) on a stretch of audio.

use super::cluster::normalized;
use super::fbank::{num_frames, Fbank, NUM_MELS, SAMPLE_RATE};
use anyhow::{anyhow, Context, Result};
use ndarray::Array3;
use ort::inputs;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::TensorRef;
use std::path::Path;

/// Audio shorter than this carries too little voice to measure reliably.
pub const MIN_EMBED_SECS: f64 = 0.4;

/// Audio is meant to be in [-1, 1]; anything else (NaN, infinity, wild amplitudes from a corrupt or
/// hostile file) would turn into NaN features and poison the clustering, so it is neutralised.
pub(crate) fn sanitize_samples(samples: &[f32]) -> std::borrow::Cow<'_, [f32]> {
    if samples.iter().all(|s| s.is_finite() && s.abs() <= 1.0) {
        return std::borrow::Cow::Borrowed(samples);
    }
    std::borrow::Cow::Owned(
        samples.iter().map(|&s| if s.is_finite() { s.clamp(-1.0, 1.0) } else { 0.0 }).collect(),
    )
}

pub struct SpeakerEmbedder {
    session: Session,
    fbank: Fbank,
    input_name: String,
    output_name: String,
}

impl SpeakerEmbedder {
    pub fn load(model_path: &Path) -> Result<Self> {
        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .commit_from_file(model_path)
            .with_context(|| format!("Failed to load speaker model {}", model_path.display()))?;

        let input_name = session.inputs.first().map(|i| i.name.clone()).ok_or_else(|| anyhow!("Speaker model has no inputs"))?;
        let output_name = session.outputs.first().map(|o| o.name.clone()).ok_or_else(|| anyhow!("Speaker model has no outputs"))?;
        Ok(Self { session, fbank: Fbank::new(), input_name, output_name })
    }

    /// Voice embedding (unit length) for 16 kHz mono samples in [-1, 1], or `None` if too short.
    pub fn embed(&mut self, samples: &[f32]) -> Result<Option<Vec<f32>>> {
        let samples = sanitize_samples(samples);
        let samples = samples.as_ref();
        let frames = num_frames(samples.len());
        if (samples.len() as f64) < MIN_EMBED_SECS * SAMPLE_RATE as f64 || frames == 0 {
            return Ok(None);
        }

        let mut features = Array3::from_shape_vec((1, frames, NUM_MELS), self.fbank.compute(samples))
            .context("Unexpected feature shape")?;
        // Cepstral mean normalisation: remove each bin's average over the clip (also removes gain)
        for bin in 0..NUM_MELS {
            let mean = (0..frames).map(|t| features[[0, t, bin]]).sum::<f32>() / frames as f32;
            for t in 0..frames {
                features[[0, t, bin]] -= mean;
            }
        }

        let outputs = self
            .session
            .run(inputs![self.input_name.as_str() => TensorRef::from_array_view(features.view())?])?;
        let embedding = outputs
            .get(self.output_name.as_str())
            .ok_or_else(|| anyhow!("Speaker model output '{}' not found", self.output_name))?
            .try_extract_array::<f32>()?;
        let embedding: Vec<f32> = embedding.iter().copied().collect();
        // A model that returns NaN/inf for some clip must not feed the clustering
        if embedding.is_empty() || embedding.iter().any(|v| !v.is_finite()) {
            return Ok(None);
        }
        Ok(Some(normalized(embedding)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_audio_is_passed_through_untouched() {
        let clean = [0.0, 0.5, -1.0, 1.0];
        assert!(matches!(sanitize_samples(&clean), std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn nan_infinity_and_out_of_range_samples_are_neutralised() {
        let hostile = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1e30, -1e30, 0.25];
        let safe = sanitize_samples(&hostile);
        assert_eq!(safe.as_ref(), &[0.0, 0.0, 0.0, 1.0, -1.0, 0.25]);
        assert!(safe.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
    }
}
