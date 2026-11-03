//! Opt-in accuracy check: runs the real pipeline (the app's own audio decoder, the speaker model,
//! clustering) on recordings that come with the true speaker for every line.
//!
//! Ignored by default because it needs the downloaded model and some audio. Each recording is
//! `<name>.<m4a|mp4|wav>` plus `<name>.json`: `[{"start": 1.2, "end": 4.0, "speaker": "spk1"}, ...]`.
//!
//! ```text
//! SPEAKER_TEST_MODEL=/path/to/wespeaker_en_voxceleb_resnet34.onnx \
//! SPEAKER_TEST_DIR=/path/to/recordings \
//!   cargo test --lib speaker_accuracy -- --ignored --nocapture
//! ```

use super::embedder::SpeakerEmbedder;
use super::fbank::SAMPLE_RATE;
use super::labeling::{assign_speakers, LineInput, Options};
use crate::audio::decoder::decode_audio_file;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(serde::Deserialize)]
struct Turn {
    start: f64,
    end: f64,
    speaker: String,
}

/// Share of lines that agree under the best one-to-one matching of detected to true speakers.
fn accuracy(truth: &[usize], predicted: &[usize]) -> f64 {
    let kt = truth.iter().max().map_or(0, |m| m + 1);
    let kp = predicted.iter().max().map_or(0, |m| m + 1);
    let mut confusion = vec![vec![0usize; kt]; kp];
    for (t, p) in truth.iter().zip(predicted) {
        confusion[*p][*t] += 1;
    }
    fn best(confusion: &[Vec<usize>], p: usize, used: &mut [bool]) -> usize {
        if p == confusion.len() {
            return 0;
        }
        let mut top = best(confusion, p + 1, used); // leave this detected speaker unmatched
        for t in 0..used.len() {
            if !used[t] {
                used[t] = true;
                top = top.max(confusion[p][t] + best(confusion, p + 1, used));
                used[t] = false;
            }
        }
        top
    }
    best(&confusion, 0, &mut vec![false; kt]) as f64 / truth.len().max(1) as f64
}

fn audio_for(dir: &Path, name: &str) -> Option<PathBuf> {
    ["m4a", "mp4", "wav"].iter().map(|ext| dir.join(format!("{name}.{ext}"))).find(|p| p.exists())
}

#[test]
fn accuracy_helper_matches_labels_up_to_renaming() {
    assert_eq!(accuracy(&[0, 0, 1, 1], &[1, 1, 0, 0]), 1.0);
    assert_eq!(accuracy(&[0, 0, 1, 1], &[0, 0, 0, 0]), 0.5);
    assert_eq!(accuracy(&[0, 1, 2, 0], &[0, 1, 1, 0]), 0.75);
}

#[test]
#[ignore = "needs SPEAKER_TEST_MODEL and SPEAKER_TEST_DIR (see module docs)"]
fn speaker_accuracy_on_real_recordings() {
    let model = std::env::var("SPEAKER_TEST_MODEL").expect("set SPEAKER_TEST_MODEL");
    let dir = PathBuf::from(std::env::var("SPEAKER_TEST_DIR").expect("set SPEAKER_TEST_DIR"));
    let min_accuracy: f64 = std::env::var("SPEAKER_TEST_MIN_ACCURACY").ok().and_then(|v| v.parse().ok()).unwrap_or(0.85);
    let mut embedder = SpeakerEmbedder::load(Path::new(&model)).expect("load model");

    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_str().and_then(|f| f.strip_suffix(".json")).map(str::to_string))
        .collect();
    names.sort();

    println!("{:<20} {:>5} {:>5} | {:>11} | {:>16}", "recording", "lines", "true", "known-count", "estimated: k/acc");
    let mut scored = Vec::new();
    for name in names {
        let Some(audio) = audio_for(&dir, &name) else { continue };
        let turns: Vec<Turn> = serde_json::from_str(&std::fs::read_to_string(dir.join(format!("{name}.json"))).unwrap()).unwrap();
        // Exactly what api_detect_speakers does: decode with the app's decoder, then 16 kHz mono
        let samples = decode_audio_file(&audio).expect("decode").to_whisper_format();

        let lines: Vec<LineInput> = turns
            .iter()
            .map(|t| {
                let from = ((t.start * SAMPLE_RATE as f64) as usize).min(samples.len());
                let to = ((t.end * SAMPLE_RATE as f64) as usize).min(samples.len());
                LineInput { duration: t.end - t.start, embedding: embedder.embed(&samples[from..to]).unwrap() }
            })
            .collect();
        if turns.iter().all(|t| t.speaker == "?") {
            // No answer key (speaker "?"): nothing to score
            println!("{:<20} {:>5}  (no ground truth, skipped)", name, turns.len());
            continue;
        }
        let mut ids: BTreeMap<&str, usize> = BTreeMap::new();
        let truth: Vec<usize> = turns.iter().map(|t| { let n = ids.len(); *ids.entry(&t.speaker).or_insert(n) }).collect();

        let known = assign_speakers(&lines, &Options { num_speakers: Some(ids.len()), ..Options::default() });
        let auto = assign_speakers(&lines, &Options::default());
        let known_acc = accuracy(&truth, &known.labels);
        println!("{:<20} {:>5} {:>5} | {:>11.2} | {:>7} / {:.2}", name, turns.len(), ids.len(), known_acc, auto.num_speakers, accuracy(&truth, &auto.labels));
        // Recordings built from near-identical synthetic voices are expected to fail; report only
        if !name.contains("hard") {
            scored.push((name, known_acc));
        }
    }

    assert!(!scored.is_empty(), "no recordings found in {}", dir.display());
    let mean = scored.iter().map(|(_, a)| a).sum::<f64>() / scored.len() as f64;
    println!("mean accuracy with the speaker count given: {mean:.3} over {} recordings", scored.len());
    assert!(mean >= min_accuracy, "mean accuracy {mean:.3} below {min_accuracy}");
}

/// The voice-memory path end to end: name the speakers of one recording, then check that the same
/// people are suggested in a *different* recording (other room, other codec) of the same voices.
///
/// `SPEAKER_TEST_MEMORY_PAIR=first,second` names two recordings in `SPEAKER_TEST_DIR` that share
/// speakers (default `en_flite4,en_flite4_room`).
#[tokio::test]
#[ignore = "needs SPEAKER_TEST_MODEL and SPEAKER_TEST_DIR (see module docs)"]
async fn voice_memory_recognises_people_across_recordings() {
    use crate::database::repositories::people::PeopleRepository;
    use sqlx::sqlite::SqlitePoolOptions;

    let model = std::env::var("SPEAKER_TEST_MODEL").expect("set SPEAKER_TEST_MODEL");
    let dir = PathBuf::from(std::env::var("SPEAKER_TEST_DIR").expect("set SPEAKER_TEST_DIR"));
    let pair = std::env::var("SPEAKER_TEST_MEMORY_PAIR").unwrap_or_else(|_| "en_flite4,en_flite4_room".to_string());
    let names: Vec<&str> = pair.split(',').collect();
    assert_eq!(names.len(), 2, "SPEAKER_TEST_MEMORY_PAIR must be two recording names");
    let mut embedder = SpeakerEmbedder::load(Path::new(&model)).expect("load model");

    let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    // For each recording: detect speakers (count known), store them with their voices, and note
    // which true speaker each detected speaker really is.
    let mut truth_of_slot: Vec<Vec<(String, String)>> = Vec::new();
    for (index, name) in names.iter().enumerate() {
        let meeting_id = format!("m{}", index + 1);
        let audio = audio_for(&dir, name).unwrap_or_else(|| panic!("no audio for {name}"));
        let turns: Vec<Turn> = serde_json::from_str(&std::fs::read_to_string(dir.join(format!("{name}.json"))).unwrap()).unwrap();
        let samples = decode_audio_file(&audio).expect("decode").to_whisper_format();
        let lines: Vec<LineInput> = turns
            .iter()
            .map(|t| {
                let from = ((t.start * SAMPLE_RATE as f64) as usize).min(samples.len());
                let to = ((t.end * SAMPLE_RATE as f64) as usize).min(samples.len());
                LineInput { duration: t.end - t.start, embedding: embedder.embed(&samples[from..to]).unwrap() }
            })
            .collect();
        let true_speakers: std::collections::BTreeSet<&str> = turns.iter().map(|t| t.speaker.as_str()).collect();
        let labeling = assign_speakers(&lines, &Options { num_speakers: Some(true_speakers.len()), ..Options::default() });

        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES (?, ?, 'x', 'x')")
            .bind(&meeting_id).bind(*name).execute(&pool).await.unwrap();
        let mut assignments = Vec::new();
        for (i, (turn, label)) in turns.iter().zip(&labeling.labels).enumerate() {
            let id = format!("{meeting_id}-{i}");
            sqlx::query("INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time) VALUES (?, ?, 'x', '10:00', ?, ?)")
                .bind(&id).bind(&meeting_id).bind(turn.start).bind(turn.end).execute(&pool).await.unwrap();
            assignments.push((id, Some(*label)));
        }
        PeopleRepository::apply_detected_speakers(&pool, &meeting_id, &assignments, &labeling.voices).await.unwrap();

        // Which true speaker does each detected speaker mostly correspond to?
        let mut slots = Vec::new();
        for speaker in 0..labeling.num_speakers {
            let mut tally: BTreeMap<&str, usize> = BTreeMap::new();
            for (turn, label) in turns.iter().zip(&labeling.labels) {
                if *label == speaker {
                    *tally.entry(turn.speaker.as_str()).or_default() += 1;
                }
            }
            let truth = tally.into_iter().max_by_key(|(_, n)| *n).map(|(t, _)| t.to_string()).unwrap();
            slots.push((format!("S{}", speaker + 1), truth));
        }
        truth_of_slot.push(slots);
    }

    // The first recording's speakers are named by the user...
    for (key, truth) in &truth_of_slot[0] {
        let person = PeopleRepository::get_or_create_person(&pool, truth).await.unwrap();
        PeopleRepository::set_speaker_person(&pool, "m1", key, Some(&person.id)).await.unwrap();
    }
    // ...and the second one should now be recognised
    let suggestions = PeopleRepository::suggest_people_for_meeting(&pool, "m2").await.unwrap();
    let mut correct = 0;
    for (key, truth) in &truth_of_slot[1] {
        let best = suggestions.iter().find(|s| &s.speaker_key == key && s.is_best);
        // log-ok: test-only accuracy tool run by hand on fictional fixtures
        println!(
            "{key}: really {truth:<6} suggested {:<8} similarity {}",
            best.map_or("(nothing)", |s| s.person_name.as_str()),
            best.map_or("-".to_string(), |s| format!("{:.2} ({})", s.similarity, s.strength)),
        );
        if best.map_or(false, |s| &s.person_name == truth) {
            correct += 1;
        }
    }
    println!("recognised {correct} of {} speakers across recordings", truth_of_slot[1].len());
    assert_eq!(correct, truth_of_slot[1].len(), "every speaker should be recognised");
}
