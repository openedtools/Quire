//! How long the live path takes to show the first line (plan Phase L, PL3). Opt in on a Mac:
//! `QUIRE_PARAKEET_DIR=<app data>/models cargo test --lib first_line -- --ignored --nocapture`.
//! The speech is made up and made on the spot by macOS `say`; nothing is recorded or kept.

#![cfg(test)]

use super::vad::tests::feed_live;
use super::vad::{ContinuousVadProcessor, SpeechSegment};
use std::path::Path;
use std::time::Instant;

/// Made-up speech: one long sentence with no real pause, then a short one after a pause.
const SCRIPT: &str = "The garden club agreed to repaint the benches near the pond before the spring fair, \
and the volunteers will meet on Saturday morning to sand them down, buy the paint and decide on a colour \
that matches the new gate by the library. [[slnc 1500]] Thanks, everyone.";

/// Made-up speech with no pause: words run on with no punctuation.
const RUN_ON: &str = "the garden club will repaint the benches near the pond and the volunteers will sand them \
down and buy the paint and pick a colour that matches the new gate by the library and then they will plant \
bulbs along the path and fix the tap by the shed and order more compost for the raised beds and the children \
will paint signs for every bed and the library will lend the club its long tables for the spring fair";

/// `text` spoken by macOS `say` at 16 kHz, or None where there is no `say`.
fn speech_16k(dir: &Path, text: &str) -> Option<Vec<f32>> {
    let wav = dir.join("speech.wav");
    let ok = std::process::Command::new("say")
        .args(["-o", wav.to_str().unwrap(), "--data-format=LEI16@16000", text])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        return None;
    }
    let bytes = std::fs::read(&wav).unwrap();
    // The PCM after the "data" chunk header
    let at = bytes.windows(4).position(|w| w == b"data").expect("a data chunk") + 8;
    Some(bytes[at..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect())
}

fn with_lead(audio: &[f32]) -> Vec<f32> {
    std::iter::repeat(0.0f32).take(16000).chain(audio.iter().copied()).collect()
}

#[test]
fn continuous_speech_is_cut_into_lines_of_at_most_14_seconds() {
    let dir = tempfile::tempdir().unwrap();
    let Some(speech) = speech_16k(dir.path(), RUN_ON) else {
        eprintln!("no macOS `say`: skipped");
        return;
    };
    let audio = with_lead(&speech);
    assert!(speech.len() > 20 * 16000, "the run-on speech is {:.1} s", speech.len() as f64 / 16000.0);

    // Without the cut the live path waits for a pause
    let (_, plain, _) = feed_live(&mut ContinuousVadProcessor::new(16000, 500).unwrap(), &audio);
    let longest = plain.iter().map(|s| s.samples.len()).max().unwrap_or(0);
    assert!(plain.is_empty() || longest > 14 * 16000, "the test speech must have no pause to bridge: {} lines", plain.len());

    let mut vad = ContinuousVadProcessor::new(16000, 500).unwrap().with_max_segment_ms(14_000);
    let (times, live, flushed) = feed_live(&mut vad, &audio);
    assert!(!live.is_empty(), "a line during the speech, not only at the end");
    assert!(times[0] <= 1.0 + 14.0 + 0.7, "first line after {:.1} s", times[0]);
    let all: Vec<&SpeechSegment> = live.iter().chain(flushed.iter()).collect();
    for s in &all {
        assert!(s.samples.len() <= 14 * 16000 + 9600, "a line of {} samples", s.samples.len());
    }
    // Nothing handed over twice, nothing lost, and each piece starts where the last ended
    let total: usize = all.iter().map(|s| s.samples.len()).sum();
    assert!(total <= audio.len(), "{} samples handed over from {}", total, audio.len());
    assert!(total as f64 >= speech.len() as f64 * 0.9, "speech lost: {} of {} samples", total, speech.len());
    for pair in live.windows(2) {
        assert!((pair[1].start_timestamp_ms - pair[0].end_timestamp_ms).abs() < 50.0, "{} then {}", pair[0].end_timestamp_ms, pair[1].start_timestamp_ms);
    }
}

/// Seconds of audio fed before the VAD hands over its first segment, feeding `step_ms` at a time
/// (the mixer's window), with 1 s of silence before the speech.
fn first_segment_after(audio: &[f32], step_ms: usize, max_segment_ms: Option<u32>) -> Option<(f64, f64)> {
    let mut vad = ContinuousVadProcessor::new(16000, 500).unwrap();
    if let Some(ms) = max_segment_ms {
        vad = vad.with_max_segment_ms(ms);
    }
    let lead = vec![0.0f32; 16000];
    let all: Vec<f32> = lead.iter().chain(audio.iter()).copied().collect();
    let step = 16 * step_ms;
    for (i, chunk) in all.chunks(step).enumerate() {
        if let Some(first) = vad.process_audio(chunk).unwrap().into_iter().next() {
            let fed = ((i + 1) * step).min(all.len()) as f64 / 16000.0;
            return Some((fed - 1.0, first.samples.len() as f64 / 16000.0));
        }
    }
    None
}

#[tokio::test]
#[ignore = "needs macOS `say`, the VAD model and optionally QUIRE_PARAKEET_DIR"]
async fn first_line_timing() {
    let dir = tempfile::tempdir().unwrap();
    let audio = speech_16k(dir.path(), SCRIPT).expect("macOS `say` is needed for this measurement");
    let run_on = speech_16k(dir.path(), RUN_ON).unwrap();
    for (name, audio) in [("with pauses", &audio), ("no pause", &run_on)] {
        println!("made-up speech, {name}: {:.1} s", audio.len() as f64 / 16000.0);
        for (label, step, max) in [("before: 600 ms steps, no cap", 600, None), ("600 ms steps, 14 s cap", 600, Some(14_000)), ("200 ms steps, 14 s cap", 200, Some(14_000))] {
            match first_segment_after(audio, step, max) {
                Some((at, len)) => println!("  {label}: first segment after {at:.1} s of speech, {len:.1} s long"),
                None => println!("  {label}: no segment before the audio ended"),
            }
        }
    }

    let Some(models) = std::env::var_os("QUIRE_PARAKEET_DIR") else {
        println!("QUIRE_PARAKEET_DIR not set: model timings skipped");
        return;
    };
    let engine = crate::parakeet_engine::ParakeetEngine::new_with_models_dir(Some(models.into())).unwrap();
    let name = engine.discover_models().await.unwrap().into_iter().find(|m| m.name.contains("v3")).expect("a Parakeet v3 model").name;
    let started = Instant::now();
    engine.load_model(&name).await.unwrap();
    println!("model load: {} ms", started.elapsed().as_millis());
    let (_, len) = first_segment_after(&audio, 200, Some(14_000)).unwrap();
    let segment = audio[..((len * 16000.0) as usize).min(audio.len())].to_vec();
    let started = Instant::now();
    let text = engine.transcribe_audio(segment).await.unwrap();
    // log-ok: a word count of made-up speech
    println!("first segment transcribed in {} ms ({} words)", started.elapsed().as_millis(), text.split_whitespace().count());
}
