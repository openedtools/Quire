//! Finalizing recordings and the retention sweep (spec F-RET-2 to F-RET-6, F-RET-8, F-RET-13).

use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use super::files::{self, Refusal};
use super::policy::{self, AudioPolicy, Decision, Retention};
use crate::markdown_export::store::parse_stored_time;

/// A recording that was saved but never finalized (the window closed in between) is finalized by
/// the sweep once it is this old, so the window always gets the first chance.
const STALE_UNFINALIZED: i64 = 10;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetentionSettings {
    pub default_retention: Retention,
    pub detect_on_save: bool,
}

pub async fn settings(pool: &SqlitePool) -> Result<RetentionSettings, sqlx::Error> {
    let row: Option<(Option<String>, i64)> =
        sqlx::query_as("SELECT default_retention, detect_on_save FROM retention_state WHERE id = 1").fetch_optional(pool).await?;
    let (retention, detect) = row.unwrap_or((None, 0));
    Ok(RetentionSettings {
        default_retention: retention.as_deref().and_then(Retention::parse).unwrap_or(Retention::D7),
        detect_on_save: detect != 0,
    })
}

pub async fn set_settings(pool: &SqlitePool, s: &RetentionSettings) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO retention_state (id, default_retention, detect_on_save) VALUES (1, ?1, ?2)
         ON CONFLICT(id) DO UPDATE SET default_retention = ?1, detect_on_save = ?2",
    )
    .bind(s.default_retention.as_str())
    .bind(s.detect_on_save as i64)
    .execute(pool)
    .await?;
    Ok(())
}

/// One meeting's audio as the sweep and the interface see it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MeetingAudio {
    pub meeting_id: String,
    #[serde(skip)]
    pub folder: Option<String>,
    pub retention: Option<Retention>,
    pub expires_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub finalized_at: Option<DateTime<Utc>>,
    pub state: String,
    pub mode: String,
}

impl MeetingAudio {
    fn policy(&self) -> AudioPolicy {
        AudioPolicy {
            retention: self.retention,
            expires_at: self.expires_at,
            finalized_at: self.finalized_at,
            state: self.state.clone(),
        }
    }
}

type AudioRow = (String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, String, String);

fn to_audio(r: AudioRow) -> MeetingAudio {
    let time = |t: Option<String>| t.as_deref().and_then(parse_stored_time);
    MeetingAudio {
        meeting_id: r.0,
        folder: r.1.filter(|f| !f.trim().is_empty()),
        retention: r.2.as_deref().and_then(Retention::parse),
        expires_at: time(r.3),
        deleted_at: time(r.4),
        finalized_at: time(r.5),
        state: r.6,
        mode: r.7,
    }
}

const AUDIO_COLUMNS: &str =
    "id, folder_path, audio_retention, audio_expires_at, audio_deleted_at, finalized_at, audio_state, mode";

pub async fn meeting_audio(pool: &SqlitePool, meeting_id: &str) -> Result<Option<MeetingAudio>, sqlx::Error> {
    let row: Option<AudioRow> = sqlx::query_as(&format!("SELECT {AUDIO_COLUMNS} FROM meetings WHERE id = ?"))
        .bind(meeting_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(to_audio))
}

async fn all_audio(pool: &SqlitePool) -> Result<Vec<MeetingAudio>, sqlx::Error> {
    let rows: Vec<AudioRow> = sqlx::query_as(&format!(
        "SELECT {AUDIO_COLUMNS} FROM meetings WHERE delete_requested_at IS NULL"
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(to_audio).collect())
}

/// Two spellings of the same folder: the same place on disk when both exist, otherwise the same
/// text ignoring a trailing separator and letter case (as Windows and macOS compare names).
pub(crate) fn same_folder(a: &str, b: &str) -> bool {
    if let (Ok(x), Ok(y)) = (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        return x == y;
    }
    let trim = |s: &str| s.trim_end_matches(['/', '\\']).to_lowercase();
    trim(a) == trim(b)
}

/// Other meetings that point at the same recording folder (older installs could share one).
pub(crate) async fn folder_shared(pool: &SqlitePool, meeting_id: &str, folder: &str) -> Result<bool, sqlx::Error> {
    let others: Vec<(String,)> =
        sqlx::query_as("SELECT folder_path FROM meetings WHERE id <> ? AND folder_path IS NOT NULL AND folder_path <> ''")
            .bind(meeting_id)
            .fetch_all(pool)
            .await?;
    Ok(others.iter().any(|(other,)| same_folder(other, folder)))
}

/// Deletes one meeting's audio and records the result. Returns true when no audio is left.
async fn delete_and_record(pool: &SqlitePool, audio: &MeetingAudio, roots: &[PathBuf], now: DateTime<Utc>) -> Result<DeleteResult, sqlx::Error> {
    let Some(folder) = audio.folder.clone() else {
        set_state(pool, &audio.meeting_id, "none", None).await?;
        return Ok(DeleteResult::Done);
    };
    let shared = folder_shared(pool, &audio.meeting_id, &folder).await?;
    let roots = roots.to_vec();
    let path = PathBuf::from(&folder);
    let deletion = tokio::task::spawn_blocking(move || files::delete_audio(&path, &roots, shared)).await.unwrap_or_default();
    match deletion.refused {
        Some(Refusal::Missing) => {
            set_state(pool, &audio.meeting_id, "none", None).await?;
            Ok(DeleteResult::Done)
        }
        // A drive that is not attached: nothing is decided, the next sweep tries again
        Some(Refusal::Unreachable) => Ok(DeleteResult::Pending),
        Some(refusal) => Ok(DeleteResult::Refused(refusal)),
        None if deletion.complete() => {
            set_state(pool, &audio.meeting_id, "deleted", Some(now)).await?;
            info!("Deleted the audio of meeting {} ({} file(s))", audio.meeting_id, deletion.results.len());
            Ok(DeleteResult::Done)
        }
        None => {
            warn!("Meeting {}: {} audio file(s) could not be deleted yet; will retry", audio.meeting_id, deletion.failed());
            Ok(DeleteResult::Pending)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeleteResult {
    Done,
    Pending,
    Refused(Refusal),
}

async fn set_state(pool: &SqlitePool, meeting_id: &str, state: &str, deleted_at: Option<DateTime<Utc>>) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE meetings SET audio_state = ?, audio_deleted_at = COALESCE(?, audio_deleted_at) WHERE id = ?")
        .bind(state)
        .bind(deleted_at.map(|t| t.to_rfc3339()))
        .bind(meeting_id)
        .execute(pool)
        .await?;
    Ok(())
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct FinalizeOutcome {
    /// The recording was finalized by this call (false: already finalized, or no recording).
    pub finalized: bool,
    pub retention: Option<Retention>,
    pub audio_state: String,
    /// Off: the audio was deleted now.
    pub deleted_now: bool,
    /// Off: why the audio was kept instead (shown to the person).
    pub kept_because: Option<String>,
}

/// Finalizes a saved recording (F-RET-2, F-RET-3): stamps the retention from the setting and its
/// deadline, and for Off deletes the audio, but only once the transcript is saved and, when
/// detect-on-save is on, detection succeeded (`detection` is that result; None when not needed).
/// Calling it again does nothing.
pub async fn finalize(
    pool: &SqlitePool,
    meeting_id: &str,
    roots: &[PathBuf],
    now: DateTime<Utc>,
    detection: Option<bool>,
) -> Result<FinalizeOutcome, sqlx::Error> {
    let Some(audio) = meeting_audio(pool, meeting_id).await? else {
        return Ok(FinalizeOutcome::default());
    };
    let Some(folder) = audio.folder.clone() else {
        return Ok(FinalizeOutcome { audio_state: audio.state, ..Default::default() });
    };
    if audio.retention.is_some() {
        return Ok(FinalizeOutcome { retention: audio.retention, audio_state: audio.state, ..Default::default() });
    }
    let setting = settings(pool).await?.default_retention;
    let (reach, has_audio, recorded_mode) = {
        let path = PathBuf::from(&folder);
        tokio::task::spawn_blocking(move || (files::reach(&path), files::has_audio(&path), files::recorded_mode(&path)))
            .await
            .unwrap_or((files::Reach::Unreachable, false, None))
    };
    if reach == files::Reach::Unreachable {
        // The recordings drive is not attached: wait, rather than record "no audio"
        return Ok(FinalizeOutcome { audio_state: audio.state, ..Default::default() });
    }
    let mode = if recorded_mode.as_deref() == Some("transcript_only") { "transcript_only" } else { audio.mode.as_str() };
    let state = if has_audio && mode != "transcript_only" { "present" } else { "none" };
    // Claim the meeting atomically: when the window and the sweep both finalize, only one does
    let claimed = sqlx::query(
        "UPDATE meetings SET mode = ?, audio_retention = ?, finalized_at = ?, audio_expires_at = ?, audio_state = ?
         WHERE id = ? AND audio_retention IS NULL",
    )
    .bind(mode)
    .bind(setting.as_str())
    .bind(now.to_rfc3339())
    .bind(setting.deadline(now).map(|t| t.to_rfc3339()))
    .bind(state)
    .bind(meeting_id)
    .execute(pool)
    .await?;
    if claimed.rows_affected() == 0 {
        let audio = meeting_audio(pool, meeting_id).await?.unwrap_or(audio);
        return Ok(FinalizeOutcome { retention: audio.retention, audio_state: audio.state, ..Default::default() });
    }

    let mut outcome = FinalizeOutcome { finalized: true, retention: Some(setting), audio_state: state.to_string(), ..Default::default() };
    if setting != Retention::Off || state != "present" {
        return Ok(outcome);
    }
    // Off: delete only after the save is confirmed (F-RET-3)
    let lines: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM transcripts WHERE meeting_id = ?").bind(meeting_id).fetch_one(pool).await?;
    if lines.0 == 0 {
        outcome.kept_because = Some("The transcript is empty, so the audio was kept".to_string());
    } else if detection == Some(false) {
        outcome.kept_because = Some("Speaker detection did not finish, so the audio was kept".to_string());
    } else {
        let audio = meeting_audio(pool, meeting_id).await?.expect("meeting exists");
        match delete_and_record(pool, &audio, roots, now).await? {
            DeleteResult::Done => {
                outcome.deleted_now = true;
                outcome.audio_state = "deleted".to_string();
            }
            DeleteResult::Pending => outcome.kept_because = Some("A file is in use; the audio is deleted when it is free".to_string()),
            DeleteResult::Refused(_) => {
                outcome.kept_because = Some("The recording is outside the recordings folder, so it was not deleted".to_string())
            }
        }
    }
    // Audio the person was told is kept really is kept: no deadline for the sweep to act on.
    // (A file in use keeps its deadline, so the sweep retries it.)
    let pending = outcome.kept_because.as_deref().map_or(false, |k| k.starts_with("A file is in use"));
    if outcome.kept_because.is_some() && !pending {
        sqlx::query("UPDATE meetings SET audio_retention = 'keep', audio_expires_at = NULL WHERE id = ?")
            .bind(meeting_id)
            .execute(pool)
            .await?;
        outcome.retention = Some(Retention::Keep);
    }
    Ok(outcome)
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct SweepReport {
    pub deleted: usize,
    pub pending: usize,
    /// Kept because the folder is shared or outside the recordings folders.
    pub kept_outside: usize,
    pub finalized_late: usize,
    pub deletes_finished: usize,
    /// Due recordings waiting for the person's confirmation (mass-expiry guard).
    pub needs_confirmation: usize,
    /// The clock is earlier than one already seen: nothing was deleted.
    pub clock_went_back: bool,
}

/// One sweep (F-RET-4). `confirmed` is the number of due recordings the person agreed to delete
/// after the mass-expiry guard stopped an earlier sweep.
pub async fn sweep(pool: &SqlitePool, roots: &[PathBuf], now: DateTime<Utc>, confirmed: usize) -> Result<SweepReport, sqlx::Error> {
    sweep_with(pool, roots, now, Confirmation::Count(confirmed)).await.map(|(report, _)| report)
}

/// What the person agreed to when many recordings were due at once (F-RET-13).
#[derive(Debug, Clone)]
pub enum Confirmation {
    /// A number of recordings (the guard's original form)
    Count(usize),
    /// Exactly these meetings, as they were listed when the person was asked (review finding)
    Meetings(std::collections::HashSet<String>),
}

/// One sweep, also returning the meetings it held back for confirmation, so a later "yes"
/// covers exactly those and nothing that became due since.
pub async fn sweep_with(pool: &SqlitePool, roots: &[PathBuf], now: DateTime<Utc>, confirmation: Confirmation) -> Result<(SweepReport, Vec<String>), sqlx::Error> {
    let confirmed = match &confirmation {
        Confirmation::Count(n) => *n,
        Confirmation::Meetings(_) => 0,
    };
    let mut report = SweepReport::default();
    let latest: Option<(Option<String>,)> =
        sqlx::query_as("SELECT latest_clock_seen FROM retention_state WHERE id = 1").fetch_optional(pool).await?;
    let latest = latest.and_then(|(t,)| t).as_deref().and_then(parse_stored_time);
    if policy::clock_went_back(now, latest) {
        warn!("Retention: the clock is earlier than one already seen; deletion paused");
        report.clock_went_back = true;
        return Ok((report, vec![]));
    }
    sqlx::query(
        "INSERT INTO retention_state (id, last_sweep_at, latest_clock_seen) VALUES (1, ?1, ?1)
         ON CONFLICT(id) DO UPDATE SET last_sweep_at = ?1,
           latest_clock_seen = CASE WHEN latest_clock_seen IS NULL OR latest_clock_seen < ?1 THEN ?1 ELSE latest_clock_seen END",
    )
    .bind(now.to_rfc3339())
    .execute(pool)
    .await?;

    report.deletes_finished = super::retry_pending_deletes(pool).await;

    // Recordings saved while the window could not finalize them. The wait counts from the save (the
    // later of created_at and updated_at), not from when a notes-first meeting was created. Detection never ran for them,
    // so with detect-on-save on, Off keeps their audio (F-RET-14).
    let detect_on_save = settings(pool).await?.detect_on_save;
    let stale: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id, CAST(created_at AS TEXT), CAST(updated_at AS TEXT) FROM meetings
         WHERE folder_path IS NOT NULL AND folder_path <> '' AND audio_retention IS NULL AND delete_requested_at IS NULL",
    )
    .fetch_all(pool)
    .await?;
    for (id, created, updated) in stale {
        let saved = [parse_stored_time(&created), parse_stored_time(&updated)].into_iter().flatten().max();
        let old_enough = saved.map_or(true, |c| now - c >= Duration::minutes(STALE_UNFINALIZED));
        let detection = detect_on_save.then_some(false);
        if old_enough && finalize(pool, &id, roots, now, detection).await?.finalized {
            report.finalized_late += 1;
        }
    }

    let meetings = all_audio(pool).await?;
    let with_audio = meetings.iter().filter(|m| m.state == "present").count();
    let mut due: Vec<&MeetingAudio> = meetings.iter().filter(|m| policy::due(&m.policy(), now) == Decision::Delete).collect();
    // Confirmed meetings go now; anything that became due since the question is asked about again
    let mut agreed: Vec<&MeetingAudio> = Vec::new();
    if let Confirmation::Meetings(ids) = &confirmation {
        let (yes, rest): (Vec<&MeetingAudio>, Vec<&MeetingAudio>) = due.into_iter().partition(|m| ids.contains(&m.meeting_id));
        agreed = yes;
        due = rest;
    }
    let mut held: Vec<String> = Vec::new();
    if policy::needs_confirmation(due.len(), with_audio) && confirmed < due.len() {
        report.needs_confirmation = due.len();
        warn!("Retention: {} recordings are due at once; waiting for confirmation", due.len());
        held = due.iter().map(|m| m.meeting_id.clone()).collect();
        due.clear();
    }
    for audio in agreed.into_iter().chain(due) {
        // Checked before every deletion, not only at the start: detection or retranscription may
        // have started meanwhile, and they read the audio (review finding). The rest waits.
        if crate::diarization::commands::is_detecting() || crate::audio::retranscription::is_retranscription_in_progress() {
            break;
        }
        match delete_and_record(pool, audio, roots, now).await? {
            DeleteResult::Done => report.deleted += 1,
            DeleteResult::Pending => report.pending += 1,
            DeleteResult::Refused(_) => report.kept_outside += 1,
        }
    }
    Ok((report, held))
}

/// A recording whose audio is due but kept by the safety rules (review finding): the window
/// lists it with the reason, and the person chooses.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct KeptRecording {
    pub meeting_id: String,
    pub title: String,
    /// `shared` (another meeting uses the folder) or `outside` (not in a recordings folder)
    pub reason: String,
}

pub async fn kept_by_rules(pool: &SqlitePool, roots: &[PathBuf], now: DateTime<Utc>) -> Result<Vec<KeptRecording>, sqlx::Error> {
    let mut out = Vec::new();
    for audio in all_audio(pool).await? {
        if audio.state != "present" || policy::due(&audio.policy(), now) != Decision::Delete {
            continue;
        }
        let Some(folder) = audio.folder.clone() else { continue };
        let reason = if folder_shared(pool, &audio.meeting_id, &folder).await? {
            "shared"
        } else {
            let roots = roots.to_vec();
            let path = PathBuf::from(&folder);
            match tokio::task::spawn_blocking(move || files::check_folder(&path, &roots, false)).await {
                Ok(Err(Refusal::OutsideRoots)) | Ok(Err(Refusal::NotOurs)) | Ok(Err(Refusal::Link)) => "outside",
                _ => continue,
            }
        };
        let title: Option<(String,)> = sqlx::query_as("SELECT title FROM meetings WHERE id = ?").bind(&audio.meeting_id).fetch_optional(pool).await?;
        out.push(KeptRecording { meeting_id: audio.meeting_id.clone(), title: title.map(|(t,)| t).unwrap_or_default(), reason: reason.into() });
    }
    Ok(out)
}

/// Deletes one meeting's audio now, at the person's request ("Delete audio now"). Automatic rules
/// still apply: a shared folder or one outside the recordings folders is not touched.
pub async fn delete_audio_now(pool: &SqlitePool, meeting_id: &str, roots: &[PathBuf], now: DateTime<Utc>) -> Result<String, sqlx::Error> {
    let Some(audio) = meeting_audio(pool, meeting_id).await? else {
        return Ok("missing".to_string());
    };
    if audio.finalized_at.is_none() {
        return Ok("not_finalized".to_string());
    }
    Ok(match delete_and_record(pool, &audio, roots, now).await? {
        DeleteResult::Done => {
            sqlx::query("UPDATE meetings SET audio_expires_at = ? WHERE id = ? AND audio_expires_at IS NULL")
                .bind(now.to_rfc3339())
                .bind(meeting_id)
                .execute(pool)
                .await?;
            "deleted".to_string()
        }
        DeleteResult::Pending => "pending".to_string(),
        DeleteResult::Refused(Refusal::Shared) => "shared".to_string(),
        DeleteResult::Refused(_) => "outside".to_string(),
    })
}

/// "Keep this one" (F-RET-8): the meeting's audio is never deleted by the sweep.
pub async fn keep_audio(pool: &SqlitePool, meeting_id: &str) -> Result<bool, sqlx::Error> {
    let r = sqlx::query("UPDATE meetings SET audio_retention = 'keep', audio_expires_at = NULL WHERE id = ? AND audio_state = 'present'")
        .bind(meeting_id)
        .execute(pool)
        .await?;
    Ok(r.rows_affected() > 0)
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ApplyPreview {
    pub recordings: usize,
    pub bytes: u64,
    /// Of those, already past the new deadline (deleted at the next sweep).
    pub due_now: usize,
}

/// "Apply to existing recordings" (F-RET-8): with `apply` false only counts what would change.
/// Extending is allowed only before expiry, so audio already deleted is never involved.
pub async fn apply_to_existing(pool: &SqlitePool, retention: Retention, now: DateTime<Utc>, apply: bool) -> Result<ApplyPreview, sqlx::Error> {
    let meetings: Vec<MeetingAudio> =
        all_audio(pool).await?.into_iter().filter(|m| m.state == "present" && m.finalized_at.is_some()).collect();
    let mut preview = ApplyPreview { recordings: meetings.len(), ..Default::default() };
    for m in &meetings {
        let deadline = m.finalized_at.and_then(|f| retention.deadline(f));
        if deadline.map_or(false, |d| d <= now) {
            preview.due_now += 1;
        }
        if let Some(folder) = &m.folder {
            let path = PathBuf::from(folder);
            preview.bytes += tokio::task::spawn_blocking(move || {
                files::audio_artifacts(&path).iter().filter_map(|(p, _)| std::fs::symlink_metadata(p).ok()).map(|m| m.len()).sum::<u64>()
            })
            .await
            .unwrap_or(0);
        }
        if apply {
            sqlx::query("UPDATE meetings SET audio_retention = ?, audio_expires_at = ? WHERE id = ?")
                .bind(retention.as_str())
                .bind(deadline.map(|d| d.to_rfc3339()))
                .bind(&m.meeting_id)
                .execute(pool)
                .await?;
        }
    }
    Ok(preview)
}

/// Recording folders under the recordings folders that no meeting points at (F-RET-2). Listed for
/// the person; never deleted automatically. Returns folder names only.
pub async fn unmanaged_folders(pool: &SqlitePool, roots: &[PathBuf]) -> Result<Vec<String>, sqlx::Error> {
    let known: Vec<(String,)> = sqlx::query_as("SELECT folder_path FROM meetings WHERE folder_path IS NOT NULL").fetch_all(pool).await?;
    let known: Vec<PathBuf> = known.into_iter().filter_map(|(f,)| std::fs::canonicalize(f).ok()).collect();
    let roots = roots.to_vec();
    let found = tokio::task::spawn_blocking(move || {
        let mut names = Vec::new();
        for root in roots {
            let Ok(entries) = std::fs::read_dir(&root) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if files::check_folder(&path, &[], false).is_err() {
                    continue;
                }
                let canonical = std::fs::canonicalize(&path).ok();
                if !canonical.map_or(false, |c| known.contains(&c)) {
                    names.push(entry.file_name().to_string_lossy().to_string());
                }
            }
        }
        names.sort();
        names.dedup();
        names
    })
    .await
    .unwrap_or_default();
    Ok(found)
}

/// The folders the sweep may delete in: the recordings folder setting and the app data folder's
/// `recordings` (plan PB3).
pub fn known_roots(setting: &Path, app_data_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut roots = vec![setting.to_path_buf()];
    if let Some(dir) = app_data_dir {
        roots.push(dir.join("recordings"));
    }
    roots.dedup();
    roots
}

/// Adds the earlier app's default recordings folder for an adopted install: its older meetings
/// still keep their audio there (plan PR3).
pub fn with_earlier_root(mut roots: Vec<PathBuf>, earlier: Option<PathBuf>) -> Vec<PathBuf> {
    if let Some(folder) = earlier {
        if !roots.contains(&folder) {
            roots.push(folder);
        }
    }
    roots
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_adopted_install_keeps_the_earlier_folder_as_a_root() {
        let roots = known_roots(Path::new("/r/chosen"), Some(Path::new("/data")));
        let with = with_earlier_root(roots.clone(), Some(PathBuf::from("/m/earlier")));
        assert!(with.contains(&PathBuf::from("/m/earlier")));
        assert!(with.contains(&PathBuf::from("/r/chosen")));
        assert_eq!(with_earlier_root(roots.clone(), None), roots, "a fresh install gets no extra root");
        assert_eq!(with_earlier_root(roots.clone(), Some(PathBuf::from("/r/chosen"))), roots, "no duplicate");
    }

    use super::*;
    use chrono::TimeZone;
    use sqlx::sqlite::SqlitePoolOptions;

    fn t(day: u32, hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, day, hour, 0, 0).unwrap()
    }

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    /// A saved recording: folder with audio, transcript and metadata, and one transcript line.
    async fn recording(pool: &SqlitePool, root: &Path, id: &str, lines: usize) -> PathBuf {
        let folder = root.join(format!("Rec_{id}"));
        std::fs::create_dir_all(folder.join(".checkpoints")).unwrap();
        for f in ["audio.mp4", "metadata.json", "transcripts.json"] {
            std::fs::write(folder.join(f), b"x").unwrap();
        }
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at, folder_path) VALUES (?, 'M', ?, ?, ?)")
            .bind(id)
            .bind(t(1, 0).to_rfc3339())
            .bind(t(1, 0).to_rfc3339())
            .bind(folder.to_string_lossy().to_string())
            .execute(pool)
            .await
            .unwrap();
        for i in 0..lines {
            sqlx::query("INSERT INTO transcripts (id, meeting_id, transcript, timestamp) VALUES (?, ?, 'text', '10:00')")
                .bind(format!("{id}-t{i}"))
                .bind(id)
                .execute(pool)
                .await
                .unwrap();
        }
        folder
    }

    async fn set(pool: &SqlitePool, r: Retention) {
        set_settings(pool, &RetentionSettings { default_retention: r, detect_on_save: false }).await.unwrap();
    }

    #[tokio::test]
    async fn a_new_database_defaults_to_seven_days() {
        let pool = pool().await;
        assert_eq!(settings(&pool).await.unwrap().default_retention, Retention::D7);
    }

    #[tokio::test]
    async fn finalize_stamps_the_deadline_once() {
        let tmp = tempfile::tempdir().unwrap();
        let pool = pool().await;
        set(&pool, Retention::H24).await;
        recording(&pool, tmp.path(), "m1", 2).await;
        let o = finalize(&pool, "m1", &[tmp.path().to_path_buf()], t(2, 14), None).await.unwrap();
        assert!(o.finalized && !o.deleted_now);
        let a = meeting_audio(&pool, "m1").await.unwrap().unwrap();
        assert_eq!(a.retention, Some(Retention::H24));
        assert_eq!(a.expires_at, Some(t(3, 14)));
        assert_eq!(a.state, "present");
        assert!(!finalize(&pool, "m1", &[tmp.path().to_path_buf()], t(5, 0), None).await.unwrap().finalized, "idempotent");
    }

    #[tokio::test]
    async fn off_deletes_only_after_a_confirmed_save() {
        let tmp = tempfile::tempdir().unwrap();
        let roots = [tmp.path().to_path_buf()];
        let pool = pool().await;
        set(&pool, Retention::Off).await;

        let saved = recording(&pool, tmp.path(), "saved", 3).await;
        let o = finalize(&pool, "saved", &roots, t(2, 14), None).await.unwrap();
        assert!(o.deleted_now, "{o:?}");
        assert!(!saved.join("audio.mp4").exists() && saved.join("transcripts.json").exists());
        assert_eq!(meeting_audio(&pool, "saved").await.unwrap().unwrap().state, "deleted");

        let empty = recording(&pool, tmp.path(), "empty", 0).await;
        let o = finalize(&pool, "empty", &roots, t(2, 14), None).await.unwrap();
        assert!(!o.deleted_now && o.kept_because.is_some());
        assert!(empty.join("audio.mp4").exists(), "no transcript: the audio is all there is");

        let undetected = recording(&pool, tmp.path(), "undetected", 3).await;
        let o = finalize(&pool, "undetected", &roots, t(2, 14), Some(false)).await.unwrap();
        assert!(!o.deleted_now && o.kept_because.is_some());
        assert!(undetected.join("audio.mp4").exists(), "detection failed: the audio is kept");

        let detected = recording(&pool, tmp.path(), "detected", 3).await;
        assert!(finalize(&pool, "detected", &roots, t(2, 14), Some(true)).await.unwrap().deleted_now);
        assert!(!detected.join("audio.mp4").exists());
    }

    #[tokio::test]
    async fn the_sweep_deletes_what_is_due_and_nothing_before_finalization() {
        let tmp = tempfile::tempdir().unwrap();
        let roots = [tmp.path().to_path_buf()];
        let pool = pool().await;
        set(&pool, Retention::H24).await;
        let a = recording(&pool, tmp.path(), "a", 1).await;
        finalize(&pool, "a", &roots, t(2, 14), None).await.unwrap();
        // b was saved just now and not finalized yet: the sweep leaves it to the window
        let b = recording(&pool, tmp.path(), "b", 1).await;
        sqlx::query("UPDATE meetings SET created_at = ? WHERE id = 'b'").bind(t(3, 14).to_rfc3339()).execute(&pool).await.unwrap();

        let r = sweep(&pool, &roots, t(3, 13), 0).await.unwrap();
        assert_eq!(r.deleted, 0, "not due yet");
        let r = sweep(&pool, &roots, t(3, 14), 0).await.unwrap();
        assert_eq!(r.deleted, 1);
        assert!(!a.join("audio.mp4").exists());
        assert!(b.join("audio.mp4").exists());
        assert_eq!(r.finalized_late, 0);

        // Ten minutes later the sweep finalizes b itself, starting its own 24 hours
        let r = sweep(&pool, &roots, t(3, 14) + Duration::minutes(10), 0).await.unwrap();
        assert_eq!(r.finalized_late, 1);
        assert!(b.join("audio.mp4").exists());
    }

    #[tokio::test]
    async fn a_transcript_only_recording_is_stored_with_its_mode_and_no_audio() {
        let tmp = tempfile::tempdir().unwrap();
        let pool = pool().await;
        set(&pool, Retention::D7).await;
        let folder = recording(&pool, tmp.path(), "t", 2).await;
        std::fs::remove_file(folder.join("audio.mp4")).unwrap();
        std::fs::write(folder.join("metadata.json"), br#"{"status":"completed","mode":"transcript_only"}"#).unwrap();
        let o = finalize(&pool, "t", &[tmp.path().to_path_buf()], t(2, 14), None).await.unwrap();
        assert_eq!(o.audio_state, "none");
        let a = meeting_audio(&pool, "t").await.unwrap().unwrap();
        assert_eq!((a.mode.as_str(), a.state.as_str()), ("transcript_only", "none"));
    }

    #[tokio::test]
    async fn audio_kept_under_off_survives_every_later_sweep() {
        let tmp = tempfile::tempdir().unwrap();
        let roots = [tmp.path().to_path_buf()];
        let pool = pool().await;
        set(&pool, Retention::Off).await;
        let empty = recording(&pool, tmp.path(), "empty", 0).await;
        let undetected = recording(&pool, tmp.path(), "undetected", 3).await;
        assert!(finalize(&pool, "empty", &roots, t(2, 14), None).await.unwrap().kept_because.is_some());
        assert!(finalize(&pool, "undetected", &roots, t(2, 14), Some(false)).await.unwrap().kept_because.is_some());
        for hours in [1, 24, 24 * 365] {
            sweep(&pool, &roots, t(2, 14) + Duration::hours(hours), 0).await.unwrap();
        }
        assert!(empty.join("audio.mp4").exists(), "told it was kept, so it stays");
        assert!(undetected.join("audio.mp4").exists(), "told it was kept, so it stays");
    }

    #[tokio::test]
    async fn a_late_finalize_never_skips_detect_on_save() {
        let tmp = tempfile::tempdir().unwrap();
        let roots = [tmp.path().to_path_buf()];
        let pool = pool().await;
        set_settings(&pool, &RetentionSettings { default_retention: Retention::Off, detect_on_save: true }).await.unwrap();
        let folder = recording(&pool, tmp.path(), "m", 3).await;
        // The window never finalized it; the sweep finds it later
        sweep(&pool, &roots, t(2, 14), 0).await.unwrap();
        sweep(&pool, &roots, t(3, 14), 0).await.unwrap();
        assert!(folder.join("audio.mp4").exists(), "detection never ran, so Off must not delete");
    }

    #[tokio::test]
    async fn the_grace_period_counts_from_the_save_not_from_creation() {
        let tmp = tempfile::tempdir().unwrap();
        let roots = [tmp.path().to_path_buf()];
        let pool = pool().await;
        set(&pool, Retention::Off).await;
        // A notes-first meeting made a day ago that a recording was just saved into
        let folder = recording(&pool, tmp.path(), "m", 3).await;
        sqlx::query("UPDATE meetings SET created_at = ?, updated_at = ? WHERE id = 'm'")
            .bind(t(1, 14).to_rfc3339())
            .bind(t(2, 14).to_rfc3339())
            .execute(&pool)
            .await
            .unwrap();
        let r = sweep(&pool, &roots, t(2, 14) + Duration::minutes(1), 0).await.unwrap();
        assert_eq!(r.finalized_late, 0, "the window gets its chance first");
        assert!(folder.join("audio.mp4").exists());
    }

    #[tokio::test]
    async fn an_unreachable_folder_is_never_recorded_as_no_audio_or_deleted() {
        let tmp = tempfile::tempdir().unwrap();
        let pool = pool().await;
        set(&pool, Retention::H24).await;
        // The recordings drive is not attached: the folder's parent is missing
        let gone_root = tmp.path().join("ExternalDrive");
        let folder = gone_root.join("Rec_m");
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at, folder_path) VALUES ('m', 'M', ?, ?, ?)")
            .bind(t(1, 0).to_rfc3339())
            .bind(t(1, 0).to_rfc3339())
            .bind(folder.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .unwrap();
        let o = finalize(&pool, "m", &[gone_root.clone()], t(2, 14), None).await.unwrap();
        assert!(!o.finalized, "wait until the drive is back");
        assert_eq!(meeting_audio(&pool, "m").await.unwrap().unwrap().retention, None);

        // Finalized while attached, then the drive goes away: never marked deleted or none
        recording(&pool, tmp.path(), "k", 1).await;
        finalize(&pool, "k", &[tmp.path().to_path_buf()], t(2, 14), None).await.unwrap();
        sqlx::query("UPDATE meetings SET folder_path = ? WHERE id = 'k'")
            .bind(gone_root.join("Rec_k").to_string_lossy().to_string())
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(delete_audio_now(&pool, "k", &[gone_root.clone()], t(2, 15)).await.unwrap(), "pending");
        sweep(&pool, &[gone_root], t(4, 0), 0).await.unwrap();
        assert_eq!(meeting_audio(&pool, "k").await.unwrap().unwrap().state, "present");
    }

    #[tokio::test]
    async fn delete_now_needs_a_finalized_recording() {
        let tmp = tempfile::tempdir().unwrap();
        let pool = pool().await;
        let folder = recording(&pool, tmp.path(), "m", 1).await;
        assert_eq!(delete_audio_now(&pool, "m", &[tmp.path().to_path_buf()], t(2, 14)).await.unwrap(), "not_finalized");
        assert!(folder.join("audio.mp4").exists());
    }

    #[tokio::test]
    async fn a_shared_folder_is_seen_however_its_path_is_written() {
        let tmp = tempfile::tempdir().unwrap();
        let roots = [tmp.path().to_path_buf()];
        let pool = pool().await;
        set(&pool, Retention::H24).await;
        let folder = recording(&pool, tmp.path(), "a", 1).await;
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at, folder_path) VALUES ('b', 'M', 'x', 'x', ?)")
            .bind(format!("{}/", folder.to_string_lossy()))
            .execute(&pool)
            .await
            .unwrap();
        finalize(&pool, "a", &roots, t(2, 14), None).await.unwrap();
        sweep(&pool, &roots, t(4, 0), 0).await.unwrap();
        assert!(folder.join("audio.mp4").exists(), "b still uses the folder");
    }

    #[tokio::test]
    async fn a_sleeping_computer_catches_up_at_the_next_sweep() {
        let tmp = tempfile::tempdir().unwrap();
        let roots = [tmp.path().to_path_buf()];
        let pool = pool().await;
        set(&pool, Retention::H24).await;
        let a = recording(&pool, tmp.path(), "a", 1).await;
        finalize(&pool, "a", &roots, t(2, 14), None).await.unwrap();
        assert_eq!(sweep(&pool, &roots, t(5, 9), 0).await.unwrap().deleted, 1, "three days asleep, deleted on wake");
        assert!(!a.join("audio.mp4").exists());
    }

    #[tokio::test]
    async fn many_at_once_waits_for_confirmation_and_a_backwards_clock_pauses() {
        let tmp = tempfile::tempdir().unwrap();
        let roots = [tmp.path().to_path_buf()];
        let pool = pool().await;
        set(&pool, Retention::H24).await;
        for i in 0..21 {
            recording(&pool, tmp.path(), &format!("m{i}"), 1).await;
            finalize(&pool, &format!("m{i}"), &roots, t(2, 14), None).await.unwrap();
        }
        let r = sweep(&pool, &roots, t(4, 0), 0).await.unwrap();
        assert_eq!((r.deleted, r.needs_confirmation), (0, 21));
        let r = sweep(&pool, &roots, t(4, 1), 21).await.unwrap();
        assert_eq!(r.deleted, 21);

        // Clock set back a day: nothing happens and the pause is reported
        let r = sweep(&pool, &roots, t(3, 1), 0).await.unwrap();
        assert!(r.clock_went_back);
    }

    #[tokio::test]
    async fn recordings_kept_by_the_safety_rules_are_listed_with_their_reason() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let roots = [tmp.path().to_path_buf()];
        let pool = pool().await;
        set(&pool, Retention::H24).await;
        recording(&pool, tmp.path(), "inside", 1).await;
        finalize(&pool, "inside", &roots, t(2, 14), None).await.unwrap();
        recording(&pool, outside.path(), "away", 1).await;
        sqlx::query("UPDATE meetings SET audio_retention = 'h24', audio_expires_at = ?, finalized_at = ?, audio_state = 'present' WHERE id = 'away'")
            .bind(t(3, 14).to_rfc3339())
            .bind(t(2, 14).to_rfc3339())
            .execute(&pool)
            .await
            .unwrap();
        let kept = kept_by_rules(&pool, &roots, t(4, 0)).await.unwrap();
        assert_eq!(kept.len(), 1, "{kept:?}");
        assert_eq!((kept[0].meeting_id.as_str(), kept[0].reason.as_str()), ("away", "outside"));
    }

    #[tokio::test]
    async fn a_yes_covers_exactly_the_meetings_asked_about() {
        let tmp = tempfile::tempdir().unwrap();
        let roots = [tmp.path().to_path_buf()];
        let pool = pool().await;
        set(&pool, Retention::H24).await;
        for i in 0..21 {
            recording(&pool, tmp.path(), &format!("m{i}"), 1).await;
            finalize(&pool, &format!("m{i}"), &roots, t(2, 14), None).await.unwrap();
        }
        let (r, held) = sweep_with(&pool, &roots, t(4, 0), Confirmation::Count(0)).await.unwrap();
        assert_eq!((r.deleted, held.len()), (0, 21));
        // 21 more become due before the person answers: they are not covered by the yes
        for i in 21..42 {
            recording(&pool, tmp.path(), &format!("m{i}"), 1).await;
            finalize(&pool, &format!("m{i}"), &roots, t(2, 15), None).await.unwrap();
        }
        let yes = Confirmation::Meetings(held.into_iter().collect());
        let (r, held_again) = sweep_with(&pool, &roots, t(4, 1), yes).await.unwrap();
        assert_eq!(r.deleted, 21, "exactly the confirmed ones");
        assert_eq!((r.needs_confirmation, held_again.len()), (21, 21), "the new ones are asked about again");
        let newer: std::collections::HashSet<String> = (21..42).map(|i| format!("m{i}")).collect();
        assert_eq!(held_again.into_iter().collect::<std::collections::HashSet<_>>(), newer);
    }

    #[tokio::test]
    async fn keep_this_one_delete_now_and_apply_to_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let roots = [tmp.path().to_path_buf()];
        let pool = pool().await;
        set(&pool, Retention::H24).await;
        let kept = recording(&pool, tmp.path(), "kept", 1).await;
        let now_gone = recording(&pool, tmp.path(), "now", 1).await;
        recording(&pool, tmp.path(), "later", 1).await;
        for id in ["kept", "now", "later"] {
            finalize(&pool, id, &roots, t(2, 14), None).await.unwrap();
        }
        assert!(keep_audio(&pool, "kept").await.unwrap());
        assert_eq!(delete_audio_now(&pool, "now", &roots, t(2, 15)).await.unwrap(), "deleted");
        assert!(!now_gone.join("audio.mp4").exists());

        let preview = apply_to_existing(&pool, Retention::D30, t(2, 16), false).await.unwrap();
        assert_eq!((preview.recordings, preview.due_now), (2, 0));
        assert!(preview.bytes > 0);
        assert_eq!(meeting_audio(&pool, "later").await.unwrap().unwrap().retention, Some(Retention::H24), "a preview changes nothing");
        apply_to_existing(&pool, Retention::D30, t(2, 16), true).await.unwrap();
        assert_eq!(sweep(&pool, &roots, t(4, 0), 0).await.unwrap().deleted, 0, "extended to 30 days");
        assert!(kept.join("audio.mp4").exists());
    }

    #[tokio::test]
    async fn shared_and_outside_folders_are_kept_and_unmanaged_folders_listed() {
        let tmp = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let roots = [tmp.path().to_path_buf()];
        let pool = pool().await;
        set(&pool, Retention::H24).await;
        let outside = recording(&pool, elsewhere.path(), "out", 1).await;
        finalize(&pool, "out", &roots, t(2, 14), None).await.unwrap();
        let shared = recording(&pool, tmp.path(), "s1", 1).await;
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at, folder_path) VALUES ('s2', 'M', 'x', 'x', ?)")
            .bind(shared.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .unwrap();
        finalize(&pool, "s1", &roots, t(2, 14), None).await.unwrap();
        let r = sweep(&pool, &roots, t(4, 0), 0).await.unwrap();
        assert_eq!((r.deleted, r.kept_outside), (0, 2));
        assert!(outside.join("audio.mp4").exists() && shared.join("audio.mp4").exists());

        let orphan = tmp.path().join("Orphan");
        std::fs::create_dir_all(&orphan).unwrap();
        std::fs::write(orphan.join("metadata.json"), b"{}").unwrap();
        assert_eq!(unmanaged_folders(&pool, &roots).await.unwrap(), vec!["Orphan".to_string()]);
        assert!(orphan.exists(), "listed, never deleted");
    }

    #[tokio::test]
    async fn upgrading_an_existing_database_schedules_nothing() {
        let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        let mut migrator = sqlx::migrate!("./migrations");
        let before: Vec<_> = migrator.migrations.iter().filter(|m| m.version < 20261105000000).cloned().collect();
        migrator.migrations = std::borrow::Cow::Owned(before);
        migrator.run(&pool).await.unwrap();
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at, folder_path) VALUES ('old', 'M', 'x', 'x', '/somewhere/Rec')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let a = meeting_audio(&pool, "old").await.unwrap().unwrap();
        assert_eq!((a.retention, a.state.as_str(), a.expires_at), (Some(Retention::Keep), "present", None));
        assert_eq!(settings(&pool).await.unwrap().default_retention, Retention::Keep, "an install with meetings starts at Keep");
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(sweep(&pool, &[tmp.path().to_path_buf()], Utc::now() + Duration::days(3650), 0).await.unwrap().deleted, 0);
    }
}
