//! One-way Markdown export for reading meetings in Obsidian (spec F-MD). Separate from the
//! shared vault in `sync-format.md`: files are named for people, and nothing is read back.
//!
//! Commands that change a meeting call `schedule`; a single background worker exports after a
//! short pause, so a burst of changes (typing notes) writes the file once. Every export, from
//! the worker or a command, runs under one lock, so two never touch the same file at once.
pub mod commands;
pub mod render;
pub mod store;
pub mod writer;

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use log::{info, warn};
use once_cell::sync::Lazy;
use serde::Serialize;
use sqlx::SqlitePool;

use writer::Outcome;

/// Pause after notes stop changing (F-MD-5).
pub const NOTES_DELAY: Duration = Duration::from_secs(5);
/// Pause after any other change, so several changes from one action write once.
pub const CHANGE_DELAY: Duration = Duration::from_secs(1);

static RUN_LOCK: Lazy<tokio::sync::Mutex<()>> = Lazy::new(|| tokio::sync::Mutex::new(()));

/// Held while the settings change, so a run never straddles turning the export off or moving
/// it to another folder.
pub async fn settings_lock() -> tokio::sync::MutexGuard<'static, ()> {
    RUN_LOCK.lock().await
}

#[derive(Default)]
struct Queue {
    /// Meetings waiting to be exported: when, and the database they belong to.
    due: HashMap<String, (Instant, SqlitePool)>,
    /// The running worker, on Tauri's long-lived runtime. A finished one is replaced.
    worker: Option<tauri::async_runtime::JoinHandle<()>>,
}

static QUEUE: Lazy<Mutex<Queue>> = Lazy::new(|| Mutex::new(Queue::default()));

/// Counts from one export run. Never carries titles or paths, so it is safe to log.
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct ExportReport {
    pub written: usize,
    pub unchanged: usize,
    pub changed_outside: usize,
    pub removed: usize,
    pub failed: usize,
}

/// Exports these meetings now, if the export is on. Returns None when it is off.
pub async fn run(pool: &SqlitePool, meeting_ids: &[String], replace: bool) -> Result<Option<ExportReport>, String> {
    let _guard = RUN_LOCK.lock().await;
    let settings = store::settings(pool).await.map_err(|e| {
        warn!("Markdown export: could not read settings: {}", e);
        "Could not read the export settings".to_string()
    })?;
    let Some(folder) = settings.folder.filter(|_| settings.enabled) else {
        return Ok(None);
    };
    let root = writer::open_root(&folder).map_err(|e| {
        warn!("Markdown export: the export folder is not usable");
        e.to_string()
    })?;
    let mut report = ExportReport::default();
    for id in meeting_ids {
        // Stop as soon as the export is turned off or moved, even mid-run
        match store::settings(pool).await {
            Ok(now) if now.enabled && now.folder.as_deref() == Some(folder.as_str()) => {}
            _ => break,
        }
        match writer::export_meeting(pool, &root, id, replace).await {
            Ok(Outcome::Written) => report.written += 1,
            Ok(Outcome::Unchanged) | Ok(Outcome::Skipped) => report.unchanged += 1,
            Ok(Outcome::ChangedOutside) | Ok(Outcome::Kept) => report.changed_outside += 1,
            Ok(Outcome::Removed) => report.removed += 1,
            Err(e) => {
                report.failed += 1;
                // The error text can hold a path, which holds a title: log only its kind
                let reason = match &e {
                    writer::ExportError::Io(io) => format!("file error ({:?})", io.kind()),
                    writer::ExportError::Db(_) => "database error".to_string(),
                    writer::ExportError::Refused(r) => r.clone(),
                };
                warn!("Markdown export of meeting {} failed: {}", id, reason);
            }
        }
    }
    if report.written + report.removed > 0 {
        let _ = store::mark_exported(pool).await;
    }
    Ok(Some(report))
}

/// Asks for this meeting to be exported after `delay`. A later call for the same meeting
/// pushes the export back.
pub fn schedule(pool: &SqlitePool, meeting_id: &str, delay: Duration) {
    schedule_many(pool, std::slice::from_ref(&meeting_id.to_string()), delay);
}

pub fn schedule_many(pool: &SqlitePool, meeting_ids: &[String], delay: Duration) {
    // The vault follows the same changes, including people and folders no meeting uses yet
    // (plan PE2); it is a no-op while the vault is off
    crate::vault::schedule(pool, crate::vault::DELAY);
    if meeting_ids.is_empty() {
        return;
    }
    let mut queue = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    let when = Instant::now() + delay;
    for id in meeting_ids {
        queue.due.insert(id.clone(), (when, pool.clone()));
    }
    let running = matches!(&queue.worker, Some(tauri::async_runtime::JoinHandle::Tokio(w)) if !w.is_finished());
    if !running {
        queue.worker = Some(tauri::async_runtime::spawn(worker()));
    }
}

async fn worker() {
    loop {
        let (ready, wait) = {
            let mut queue = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
            if queue.due.is_empty() {
                queue.worker = None;
                return;
            }
            let now = Instant::now();
            let ready_ids: Vec<String> = queue.due.iter().filter(|(_, (t, _))| *t <= now).map(|(id, _)| id.clone()).collect();
            let ready: Vec<(String, SqlitePool)> =
                ready_ids.into_iter().filter_map(|id| queue.due.remove(&id).map(|(_, pool)| (id, pool))).collect();
            let wait = queue.due.values().map(|(t, _)| *t).min().map(|t| t.saturating_duration_since(now)).unwrap_or_default();
            (ready, wait)
        };
        if ready.is_empty() {
            tokio::time::sleep(wait.max(Duration::from_millis(50))).await;
            continue;
        }
        // Each meeting runs against its own database (one in the app; tests use several)
        for (id, pool) in ready {
            match run(&pool, std::slice::from_ref(&id), false).await {
                Ok(Some(report)) if report.written + report.removed + report.failed > 0 => {
                    info!("Markdown export: {:?}", report)
                }
                Ok(_) => {}
                Err(e) => warn!("Markdown export did not run: {}", e),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool_with_meeting(id: &str) -> SqlitePool {
        let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES (?, 'Plan', '2026-11-02T14:05:00Z', 'x')")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO meeting_user_notes (meeting_id, body_blocks, body_markdown, updated_at) VALUES (?, '[]', 'n', 'x')")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn nothing_is_written_until_a_folder_is_confirmed() {
        let pool = pool_with_meeting("m1").await;
        assert_eq!(run(&pool, &["m1".to_string()], false).await.unwrap(), None);

        let tmp = tempfile::tempdir().unwrap();
        store::enable(&pool, &tmp.path().to_string_lossy()).await.unwrap();
        let report = run(&pool, &["m1".to_string()], false).await.unwrap().unwrap();
        assert_eq!(report.written, 1);
        assert!(store::settings(&pool).await.unwrap().last_export_at.is_some());

        // Turned off: no more writes, and the file stays
        store::disable(&pool).await.unwrap();
        sqlx::query("UPDATE meeting_user_notes SET body_markdown = 'changed'").execute(&pool).await.unwrap();
        assert_eq!(run(&pool, &["m1".to_string()], false).await.unwrap(), None);
        assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 1);

        // A different folder forgets the old files, so they are never touched again
        let other = tempfile::tempdir().unwrap();
        store::enable(&pool, &other.path().to_string_lossy()).await.unwrap();
        assert!(store::rows(&pool).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn scheduled_changes_are_exported_once_after_the_pause() {
        let pool = pool_with_meeting("m9").await;
        let tmp = tempfile::tempdir().unwrap();
        store::enable(&pool, &tmp.path().to_string_lossy()).await.unwrap();

        for _ in 0..5 {
            schedule(&pool, "m9", Duration::from_millis(300));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(store::row(&pool, "m9").await.unwrap().is_none(), "not before the pause");
        let mut exported = false;
        for _ in 0..60 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            if store::row(&pool, "m9").await.unwrap().is_some() {
                exported = true;
                break;
            }
        }
        assert!(exported);
        assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 1);
    }
}
