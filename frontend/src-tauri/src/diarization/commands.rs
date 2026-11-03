//! Tauri commands for automatic speaker detection.

use super::embedder::SpeakerEmbedder;
use super::fbank::SAMPLE_RATE;
use super::labeling::{assign_speakers, LineInput, Options};
use super::model;
use crate::audio::decoder::decode_audio_file;
use crate::audio::retranscription::{find_audio_file, is_retranscription_in_progress};
use crate::database::repositories::{meeting::MeetingsRepository, people::PeopleRepository};
use crate::people::models::{MeetingSpeaker, SpeakerSuggestion};
use crate::state::AppState;
use log::{error as log_error, info as log_info};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// A line longer than this is measured on its first stretch only (keeps very long lines cheap).
const MAX_LINE_SECS: f64 = 20.0;

static DETECTING: AtomicBool = AtomicBool::new(false);
static DOWNLOADING: AtomicBool = AtomicBool::new(false);

/// Clears a busy flag when dropped, including on early returns and panics.
struct Busy(&'static AtomicBool);
impl Busy {
    fn acquire(flag: &'static AtomicBool) -> Option<Self> {
        (!flag.swap(true, Ordering::SeqCst)).then(|| Busy(flag))
    }
}
impl Drop for Busy {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn models_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("models").join("speaker"))
        .map_err(|e| format!("Could not find the app data folder: {e}"))
}

#[derive(Serialize)]
pub struct SpeakerModelStatus {
    pub downloaded: bool,
    pub size_bytes: u64,
}

#[derive(Serialize)]
pub struct DetectSpeakersResult {
    pub speakers: Vec<MeetingSpeaker>,
    pub lines_labeled: usize,
    pub lines_total: usize,
    /// Unnamed speakers whose voice matches someone named in an earlier meeting
    pub suggestions: Vec<SpeakerSuggestion>,
}

#[derive(Clone, Serialize)]
struct DetectionProgress<'a> {
    meeting_id: &'a str,
    /// "decoding", "analyzing" or "clustering"
    phase: &'static str,
    done: usize,
    total: usize,
}

#[derive(Clone, Serialize)]
struct DownloadProgress {
    downloaded: u64,
    total: u64,
}

/// Is the on-device voice model already downloaded?
#[tauri::command]
pub async fn speaker_model_status<R: Runtime>(app: AppHandle<R>) -> Result<SpeakerModelStatus, String> {
    let dir = models_dir(&app)?;
    // Full checksum, not just presence: a wrong file counts as "not downloaded" so the UI offers a fresh one
    let downloaded = tokio::task::spawn_blocking(move || model::is_verified(&dir)).await.unwrap_or(false);
    Ok(SpeakerModelStatus { downloaded, size_bytes: model::MODEL_SIZE_BYTES })
}

/// One-time download of the voice model (about 26 MB). Emits `speaker-model-download-progress`.
#[tauri::command]
pub async fn speaker_model_download<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let dir = models_dir(&app)?;
    let verified_dir = dir.clone();
    if tokio::task::spawn_blocking(move || model::is_verified(&verified_dir)).await.unwrap_or(false) {
        return Ok(());
    }
    let Some(_busy) = Busy::acquire(&DOWNLOADING) else {
        return Err("The speaker model is already downloading".to_string());
    };

    // log-ok: an app data, model or tool path; no meeting data
    log_info!("Downloading speaker model to {}", dir.display());
    let mut last_emitted = 0u64;
    model::download(&dir, |downloaded, total| {
        // About one event per 256 KB, and always the final one
        if downloaded - last_emitted >= 256 * 1024 || downloaded == total {
            last_emitted = downloaded;
            let _ = app.emit("speaker-model-download-progress", DownloadProgress { downloaded, total });
        }
    })
    .await
    .map(|_| ())
    .map_err(|e| {
        log_error!("Speaker model download failed: {e:#}");
        e.to_string()
    })
}

/// Work out who spoke each line of a saved meeting from the voices in its recording.
///
/// `num_speakers` is how many people spoke, if known (more reliable than guessing); `None`
/// estimates it. Replaces the meeting's current speaker labels but keeps names the user gave
/// wherever a voice still matches. Emits `speaker-detection-progress`.
///
/// Each speaker's average voice is kept on this computer so later meetings can suggest names;
/// pass `remember_voices: false` to keep nothing.
#[tauri::command]
pub async fn api_detect_speakers<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    num_speakers: Option<u32>,
    remember_voices: Option<bool>,
) -> Result<DetectSpeakersResult, String> {
    detect_speakers(&app, state.db_manager.pool(), meeting_id, num_speakers, remember_voices).await
}

/// True while speaker detection runs, so the retention sweep leaves audio alone meanwhile.
pub(crate) fn is_detecting() -> bool {
    DETECTING.load(Ordering::SeqCst)
}

/// Speaker detection for one meeting, also run by the finalizer when detect-on-save is on.
pub(crate) async fn detect_speakers<R: Runtime>(
    app: &AppHandle<R>,
    pool: &sqlx::SqlitePool,
    meeting_id: String,
    num_speakers: Option<u32>,
    remember_voices: Option<bool>,
) -> Result<DetectSpeakersResult, String> {
    let Some(_busy) = Busy::acquire(&DETECTING) else {
        return Err("Speaker detection is already running".to_string());
    };
    if is_retranscription_in_progress() {
        return Err("Wait for retranscription to finish before detecting speakers".to_string());
    }

    let meeting = MeetingsRepository::get_meeting_metadata(pool, &meeting_id)
        .await
        .map_err(|e| format!("Could not load the meeting: {e}"))?
        .ok_or_else(|| "Meeting not found".to_string())?;
    let folder = meeting
        .folder_path
        .filter(|p| !p.trim().is_empty())
        .ok_or_else(|| "This meeting has no saved recording, so there is no audio to analyse".to_string())?;
    let audio_path = find_audio_file(Path::new(&folder)).map_err(|e| e.to_string())?;

    let models = models_dir(app)?;
    let verified_dir = models.clone();
    if !tokio::task::spawn_blocking(move || model::is_verified(&verified_dir)).await.unwrap_or(false) {
        return Err("The speaker model is missing or is not the expected file. Open Detect speakers again to download it.".to_string());
    }
    let model_path = model::model_path(&models);

    // Lines we can place in the recording
    let rows: Vec<(String, Option<f64>, Option<f64>)> = sqlx::query_as(
        "SELECT id, audio_start_time, audio_end_time FROM transcripts WHERE meeting_id = ? ORDER BY audio_start_time ASC",
    )
    .bind(&meeting_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Could not load the transcript: {e}"))?;
    let lines_total = rows.len();
    let timed: Vec<(String, f64, f64)> = rows
        .into_iter()
        .filter_map(|(id, start, end)| match (start, end) {
            (Some(s), Some(e)) if e > s => Some((id, s, e)),
            _ => None,
        })
        .collect();
    if timed.is_empty() {
        return Err("This transcript has no timing information, so voices cannot be matched to lines".to_string());
    }

    log_info!("Detecting speakers for {} ({} lines, requested speakers: {:?})", meeting_id, timed.len(), num_speakers);
    let progress_app = app.clone();
    let progress_meeting = meeting_id.clone();
    let lines_for_blocking = timed.clone();
    let options = Options { num_speakers: num_speakers.map(|n| n as usize), ..Options::default() };

    // Decoding and the model are CPU-heavy; keep them off the async runtime
    let labeling = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let emit = |phase: &'static str, done: usize, total: usize| {
            let _ = progress_app.emit(
                "speaker-detection-progress",
                DetectionProgress { meeting_id: &progress_meeting, phase, done, total },
            );
        };
        emit("decoding", 0, 0);
        let samples = decode_audio_file(&audio_path)?.to_whisper_format();
        let mut embedder = SpeakerEmbedder::load(&model_path)?;

        let total = lines_for_blocking.len();
        let mut inputs = Vec::with_capacity(total);
        for (i, (_, start, end)) in lines_for_blocking.iter().enumerate() {
            let from = ((start * SAMPLE_RATE as f64) as usize).min(samples.len());
            let to = ((end.min(start + MAX_LINE_SECS) * SAMPLE_RATE as f64) as usize).min(samples.len());
            let embedding = if to > from { embedder.embed(&samples[from..to])? } else { None };
            inputs.push(LineInput { duration: end - start, embedding });
            if i % 5 == 0 || i + 1 == total {
                emit("analyzing", i + 1, total);
            }
        }
        emit("clustering", total, total);
        Ok(assign_speakers(&inputs, &options))
    })
    .await
    .map_err(|e| format!("Speaker detection stopped unexpectedly: {e}"))?
    .map_err(|e| {
        log_error!("Speaker detection failed: {e:#}");
        format!("Speaker detection failed: {e}")
    })?;

    let assignments: Vec<(String, Option<usize>)> = timed
        .iter()
        .zip(&labeling.labels)
        .map(|((id, _, _), label)| (id.clone(), Some(*label)))
        .collect();
    let voices = if remember_voices.unwrap_or(true) { labeling.voices.as_slice() } else { &[] };
    let speakers = PeopleRepository::apply_detected_speakers(pool, &meeting_id, &assignments, voices)
        .await
        .map_err(|e| e.to_string())?;
    crate::markdown_export::schedule(pool, &meeting_id, crate::markdown_export::CHANGE_DELAY);
    // A failure to guess names must never undo a successful detection
    let suggestions = PeopleRepository::suggest_people_for_meeting(pool, &meeting_id).await.unwrap_or_else(|e| {
        log_error!("Could not suggest names for {}: {}", meeting_id, e);
        Vec::new()
    });

    log_info!("Detected {} speakers in {}", labeling.num_speakers, meeting_id);
    Ok(DetectSpeakersResult { speakers, lines_labeled: timed.len(), lines_total, suggestions })
}
