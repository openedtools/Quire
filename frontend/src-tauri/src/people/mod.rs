//! People directory and per-meeting speaker labels.
//!
//! Transcript lines store only a speaker slot key ("S1"); a slot optionally links to a
//! person. Naming a speaker is therefore a single-row update that every line reflects.

pub mod commands;
pub mod models;
