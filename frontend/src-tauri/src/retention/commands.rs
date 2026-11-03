//! Retention commands, the finalizer the window calls after a save, and the background sweep
//! runner (spec F-RET-1 to F-RET-4, F-RET-8, F-RET-13, F-RET-14).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use log::{info, warn};
use once_cell::sync::Lazy;
use serde::Serialize;
use sqlx::SqlitePool;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::policy::Retention;
use super::sweep::{self, ApplyPreview, FinalizeOutcome, MeetingAudio, RetentionSettings, SweepReport};
use crate::state::AppState;

const SWEEP_EVERY: Duration = Duration::from_secs(60 * 60);
/// A window focus sweeps at most this often.
const FOCUS_GAP: Duration = Duration::from_secs(5 * 60);

static SWEEPING: AtomicBool = AtomicBool::new(false);
static LAST_FOCUS_SWEEP: Lazy<std::sync::Mutex<Option<Instant>>> = Lazy::new(|| std::sync::Mutex::new(None));
static RUNNER_STARTED: AtomicBool = AtomicBool::new(false);
/// The meetings the last sweep held back for confirmation: a "yes" covers exactly these.
static HELD: Lazy<std::sync::Mutex<Vec<String>>> = Lazy::new(|| std::sync::Mutex::new(Vec::new()));

fn db_error(context: &str, e: sqlx::Error) -> String {
    warn!("Retention: {} failed: {}", context, e);
    "Could not read or update the retention settings".to_string()
}

/// The folders automatic deletion may act in (plan PB3).
pub async fn roots<R: Runtime>(app: &AppHandle<R>) -> Vec<PathBuf> {
    let setting = crate::audio::recording_preferences::load_recording_preferences(app)
        .await
        .map(|p| p.save_folder)
        .unwrap_or_else(|_| crate::audio::recording_preferences::fallback_recordings_folder());
    let earlier = crate::audio::recording_preferences::is_adopted(app).then(crate::audio::recording_preferences::earlier_default_folder);
    sweep::with_earlier_root(sweep::known_roots(&setting, app.path().app_data_dir().ok().as_deref()), earlier)
}

/// Finalizes a saved or imported recording, running speaker detection first when detect-on-save
/// is on (F-RET-14), so Off can still use detection before the audio goes.
pub async fn finalize_meeting<R: Runtime>(app: &AppHandle<R>, pool: &SqlitePool, meeting_id: &str) -> Result<FinalizeOutcome, String> {
    let settings = sweep::settings(pool).await.map_err(|e| db_error("finalize", e))?;
    let audio = sweep::meeting_audio(pool, meeting_id).await.map_err(|e| db_error("finalize", e))?;
    let needs_finalizing = audio.as_ref().map_or(false, |a| a.folder.is_some() && a.retention.is_none());
    // Thoughts have one speaker: no detection (PN4)
    let detection = if needs_finalizing && settings.detect_on_save && audio.as_ref().map_or(false, |a| a.mode != "transcript_only") && !crate::thoughts::is_thoughts(pool, meeting_id).await {
        match crate::diarization::commands::detect_speakers(app, pool, meeting_id.to_string(), None, None).await {
            Ok(_) => Some(true),
            Err(e) => {
                warn!("Detect on save for meeting {} did not finish: {}", meeting_id, e);
                Some(false)
            }
        }
    } else {
        None
    };
    let roots = roots(app).await;
    let outcome = sweep::finalize(pool, meeting_id, &roots, chrono::Utc::now(), detection).await.map_err(|e| db_error("finalize", e))?;
    // The transcript is final now: name the meeting if it still has its default name (PL1)
    crate::summary::auto_name::name_in_background(app, meeting_id);
    Ok(outcome)
}

/// A sweep with a yes for exactly these meetings.
async fn run_sweep_confirmed<R: Runtime>(app: &AppHandle<R>, ids: std::collections::HashSet<String>) -> Option<SweepReport> {
    if crate::diarization::commands::is_detecting() || crate::audio::retranscription::is_retranscription_in_progress() || !super::scheduled::app_may_sweep() {
        return None;
    }
    if SWEEPING.swap(true, Ordering::SeqCst) {
        return None;
    }
    let result = async {
        let state = app.try_state::<AppState>()?;
        let pool = state.db_manager.pool().clone();
        let roots = roots(app).await;
        sweep::sweep_with(&pool, &roots, chrono::Utc::now(), sweep::Confirmation::Meetings(ids))
            .await
            .map(|(report, held)| {
                *HELD.lock().unwrap_or_else(|e| e.into_inner()) = held;
                report
            })
            .map_err(|e| warn!("Retention sweep failed: {}", e))
            .ok()
    }
    .await;
    SWEEPING.store(false, Ordering::SeqCst);
    if let Some(report) = &result {
        if report.deleted + report.deletes_finished > 0 {
            let _ = app.emit("retention-swept", report.clone());
        }
    }
    result
}

/// Runs one sweep unless one is already running or speakers are being detected or a recording is
/// being retranscribed (they read the audio). Tells the window about anything it must show.
pub async fn run_sweep<R: Runtime>(app: &AppHandle<R>, confirmed: usize) -> Option<SweepReport> {
    if crate::diarization::commands::is_detecting() || crate::audio::retranscription::is_retranscription_in_progress() {
        return None;
    }
    // A scheduled check that was running when the app opened finishes first
    if !super::scheduled::app_may_sweep() {
        return None;
    }
    if SWEEPING.swap(true, Ordering::SeqCst) {
        return None;
    }
    let result = async {
        let state = app.try_state::<AppState>()?;
        let pool = state.db_manager.pool().clone();
        let roots = roots(app).await;
        // A confirmation applies to the meetings the person was asked about, never to a count
        let confirmation = if confirmed > 0 {
            let held = HELD.lock().unwrap_or_else(|e| e.into_inner()).clone();
            sweep::Confirmation::Meetings(if held.len() == confirmed { held.into_iter().collect() } else { Default::default() })
        } else {
            sweep::Confirmation::Count(0)
        };
        let result = sweep::sweep_with(&pool, &roots, chrono::Utc::now(), confirmation).await;
        result
            .map(|(report, held)| {
                *HELD.lock().unwrap_or_else(|e| e.into_inner()) = held;
                report
            })
            .map_err(|e| warn!("Retention sweep failed: {}", e))
            .ok()
    }
    .await;
    SWEEPING.store(false, Ordering::SeqCst);
    if let Some(report) = &result {
        if report.deleted + report.pending + report.finalized_late + report.deletes_finished > 0 {
            info!("Retention sweep: {:?}", report);
        }
        if report.needs_confirmation > 0 {
            // The window shows the count and sends back these exact meetings with a yes
            let held = HELD.lock().unwrap_or_else(|e| e.into_inner()).clone();
            let _ = app.emit("retention-needs-confirmation", serde_json::json!({ "count": report.needs_confirmation, "ids": held }));
        }
        if report.clock_went_back {
            let _ = app.emit("retention-clock-paused", ());
        }
        if report.deleted + report.deletes_finished > 0 {
            let _ = app.emit("retention-swept", report.clone());
        }
    }
    result
}

/// Starts the sweep: once now (after the database is ready), then every hour (F-RET-4).
pub fn start_runner<R: Runtime>(app: AppHandle<R>) {
    if RUNNER_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        // Let a scheduled check that is still running finish first (up to about two minutes)
        for _ in 0..75 {
            if super::scheduled::app_may_sweep() {
                break;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        // Keep the daily check in line with the settings this install has
        if let Some(state) = app.try_state::<AppState>() {
            super::scheduled::sync(state.db_manager.pool()).await;
        }
        loop {
            run_sweep(&app, 0).await;
            tokio::time::sleep(SWEEP_EVERY).await;
        }
    });
}

/// Called when the main window regains focus: sweeps, at most every five minutes.
pub fn on_focus<R: Runtime>(app: &AppHandle<R>) {
    let mut last = LAST_FOCUS_SWEEP.lock().unwrap_or_else(|e| e.into_inner());
    if last.map_or(false, |t| t.elapsed() < FOCUS_GAP) {
        return;
    }
    *last = Some(Instant::now());
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        run_sweep(&app, 0).await;
    });
}

#[tauri::command]
pub async fn api_finalize_recording<R: Runtime>(app: AppHandle<R>, state: tauri::State<'_, AppState>, meeting_id: String) -> Result<FinalizeOutcome, String> {
    finalize_meeting(&app, state.db_manager.pool(), &meeting_id).await
}

#[tauri::command]
pub async fn api_retention_get_settings(state: tauri::State<'_, AppState>) -> Result<RetentionSettings, String> {
    sweep::settings(state.db_manager.pool()).await.map_err(|e| db_error("settings", e))
}

/// Changing the setting affects future recordings only (F-RET-8).
#[tauri::command]
pub async fn api_retention_set_settings(state: tauri::State<'_, AppState>, settings: RetentionSettings) -> Result<RetentionSettings, String> {
    let pool = state.db_manager.pool();
    sweep::set_settings(pool, &settings).await.map_err(|e| db_error("save settings", e))?;
    // The daily check follows the setting
    super::scheduled::sync(pool).await;
    sweep::settings(pool).await.map_err(|e| db_error("settings", e))
}

/// "Apply to existing recordings": `apply` false returns the counts for the confirmation.
#[tauri::command]
pub async fn api_retention_apply_to_existing<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    retention: Retention,
    apply: bool,
) -> Result<ApplyPreview, String> {
    let preview = sweep::apply_to_existing(state.db_manager.pool(), retention, chrono::Utc::now(), apply)
        .await
        .map_err(|e| db_error("apply to existing", e))?;
    if apply {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            run_sweep(&app, 0).await;
        });
    }
    Ok(preview)
}

#[tauri::command]
pub async fn api_meeting_audio(state: tauri::State<'_, AppState>, meeting_id: String) -> Result<Option<MeetingAudio>, String> {
    sweep::meeting_audio(state.db_manager.pool(), &meeting_id).await.map_err(|e| db_error("meeting audio", e))
}

#[tauri::command]
pub async fn api_meeting_keep_audio(state: tauri::State<'_, AppState>, meeting_id: String) -> Result<bool, String> {
    sweep::keep_audio(state.db_manager.pool(), &meeting_id).await.map_err(|e| db_error("keep audio", e))
}

/// "Delete audio now". Returns `deleted`, `pending` (a file in use), `shared` or `outside`.
#[tauri::command]
pub async fn api_meeting_delete_audio_now<R: Runtime>(app: AppHandle<R>, state: tauri::State<'_, AppState>, meeting_id: String) -> Result<String, String> {
    if crate::diarization::commands::is_detecting() || crate::audio::retranscription::is_retranscription_in_progress() {
        return Err("Wait for speaker detection or retranscription to finish".to_string());
    }
    let roots = roots(&app).await;
    sweep::delete_audio_now(state.db_manager.pool(), &meeting_id, &roots, chrono::Utc::now())
        .await
        .map_err(|e| db_error("delete audio", e))
}

#[derive(Debug, Clone, Serialize)]
pub struct RetentionStatus {
    pub last_sweep_at: Option<String>,
    /// Recording folders no meeting points at (names only).
    pub unmanaged: Vec<String>,
    /// Recordings past their deadline but kept because their folder is shared or outside
    pub kept: Vec<sweep::KeptRecording>,
    /// The daily audio check (None where there is none, the Mac for now)
    pub scheduled: Option<super::scheduled::Scheduled>,
}

#[tauri::command]
pub async fn api_retention_status<R: Runtime>(app: AppHandle<R>, state: tauri::State<'_, AppState>) -> Result<RetentionStatus, String> {
    let pool = state.db_manager.pool();
    let last: Option<(Option<String>,)> =
        sqlx::query_as("SELECT last_sweep_at FROM retention_state WHERE id = 1").fetch_optional(pool).await.map_err(|e| db_error("status", e))?;
    let roots = roots(&app).await;
    let unmanaged = sweep::unmanaged_folders(pool, &roots).await.map_err(|e| db_error("status", e))?;
    let kept = sweep::kept_by_rules(pool, &roots, chrono::Utc::now()).await.map_err(|e| db_error("status", e))?;
    let scheduled = super::scheduled::status(pool).await;
    Ok(RetentionStatus { last_sweep_at: last.and_then(|(t,)| t), unmanaged, kept, scheduled })
}

/// The person confirmed today's date and time after the sweep paused for a clock that went back
/// (F-RET-13): deletion resumes from now.
#[tauri::command]
pub async fn api_retention_resume_after_clock(state: tauri::State<'_, AppState>) -> Result<(), String> {
    sqlx::query("UPDATE retention_state SET latest_clock_seen = ? WHERE id = 1")
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(state.db_manager.pool())
        .await
        .map_err(|e| db_error("resume", e))?;
    Ok(())
}

/// The person agreed to delete the recordings the mass-expiry guard held back (F-RET-13).
#[tauri::command]
pub async fn api_retention_confirm_due<R: Runtime>(app: AppHandle<R>, count: usize, ids: Option<Vec<String>>) -> Result<Option<SweepReport>, String> {
    match ids {
        // The meetings the person was shown, exactly (review finding)
        Some(ids) => Ok(run_sweep_confirmed(&app, ids.into_iter().collect()).await),
        None => Ok(run_sweep(&app, count).await),
    }
}
