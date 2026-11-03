//! How much recorded audio is waiting to be transcribed (spec F-QA-5).
//!
//! Ask is refused while the backlog is over a limit, so a question never slows transcription
//! down further. The dispatcher adds each chunk's length; a worker holds a `Taken` guard while it
//! works on a chunk, and the length is removed when the guard drops, whatever path the worker took.

use std::sync::atomic::{AtomicU64, Ordering};

static PENDING_MS: AtomicU64 = AtomicU64::new(0);

/// Called when a chunk is queued for transcription.
pub fn queued(milliseconds: u64) {
    PENDING_MS.fetch_add(milliseconds, Ordering::SeqCst);
}

/// Held while a worker processes a chunk; the chunk leaves the backlog when this drops.
pub struct Taken(u64);

impl Taken {
    pub fn new(milliseconds: u64) -> Self {
        Taken(milliseconds)
    }
}

impl Drop for Taken {
    fn drop(&mut self) {
        let taken = self.0;
        let _ = PENDING_MS.try_update(Ordering::SeqCst, Ordering::SeqCst, |v| Some(v.saturating_sub(taken)));
    }
}

/// Seconds of audio queued or being transcribed.
pub fn pending_seconds() -> f64 {
    PENDING_MS.load(Ordering::SeqCst) as f64 / 1000.0
}

/// Start of a recording: nothing is waiting.
pub fn reset() {
    PENDING_MS.store(0, Ordering::SeqCst);
}

/// A chunk's length in milliseconds.
pub fn chunk_milliseconds(samples: usize, sample_rate: u32) -> u64 {
    if sample_rate == 0 {
        return 0;
    }
    samples as u64 * 1000 / sample_rate as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    // One test, because the counter is global and tests run in parallel
    #[test]
    fn the_backlog_grows_when_queued_and_shrinks_when_done() {
        reset();
        assert_eq!(chunk_milliseconds(48_000, 16_000), 3000);
        assert_eq!(chunk_milliseconds(100, 0), 0, "no sample rate means no length");
        queued(3000);
        queued(2000);
        assert_eq!(pending_seconds(), 5.0);
        {
            let _taken = Taken::new(3000);
            assert_eq!(pending_seconds(), 5.0, "still counted while being transcribed");
        }
        assert_eq!(pending_seconds(), 2.0);
        drop(Taken::new(5000));
        assert_eq!(pending_seconds(), 0.0, "never below zero");
        queued(1000);
        reset();
        assert_eq!(pending_seconds(), 0.0);
    }
}
