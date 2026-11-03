//! Automatic speaker detection ("who spoke when") for saved meetings.
//!
//! The recording is cut into the transcript's own lines, each line is turned into a voice
//! embedding by a small on-device model (WeSpeaker ResNet34, ~26 MB, downloaded on first use), and
//! lines whose voices are alike are grouped into speakers. Nothing leaves the machine except that
//! one-time model download.
//!
//! - `fbank`: Kaldi-compatible audio features (checked against Kaldi's own output)
//! - `embedder`: runs the ONNX model
//! - `cluster`: average-linkage clustering and speaker-count estimate (checked against SciPy)
//! - `labeling`: embeddings -> a speaker for every line
//! - `model`: where the model lives and how it is downloaded and verified
//! - `commands`: the Tauri commands the UI calls

pub mod cluster;
pub mod commands;
pub mod embedder;
pub mod fbank;
pub mod labeling;
pub mod model;

#[cfg(test)]
mod accuracy_check;
