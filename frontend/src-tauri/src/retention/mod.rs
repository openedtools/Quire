//! Audio retention, safe audio deletion and the two-phase meeting delete (spec 2.3, plan Phase B).
pub mod commands;
pub mod files;
pub mod policy;
pub mod scheduled;
pub mod sweep;

use log::{info, warn};
use sqlx::SqlitePool;

use crate::api::recording_files;

/// What happened when a meeting was deleted.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MeetingDeletion {
    /// The meeting row is gone.
    pub removed: bool,
    /// Recording files that could not be removed yet; the meeting stays listed as deleting and the
    /// next retry tries again.
    pub files_pending: usize,
    /// Files removed this time.
    pub files_removed: usize,
}


/// Deletes a meeting in two phases (F-RET-12): mark it as deleting, remove its recording files,
/// and only then remove the row. A file that cannot be removed (a lock on Windows) keeps the
/// meeting listed as deleting until `retry_pending_deletes` succeeds. A folder another meeting
/// still uses is left in place.
pub async fn delete_meeting(pool: &SqlitePool, meeting_id: &str) -> Result<MeetingDeletion, sqlx::Error> {
    let folder: Option<(Option<String>,)> = sqlx::query_as("SELECT folder_path FROM meetings WHERE id = ?")
        .bind(meeting_id)
        .fetch_optional(pool)
        .await?;
    let Some((folder,)) = folder else {
        return Ok(MeetingDeletion { removed: false, files_pending: 0, files_removed: 0 });
    };
    sqlx::query("UPDATE meetings SET delete_requested_at = COALESCE(delete_requested_at, ?) WHERE id = ?")
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(meeting_id)
        .execute(pool)
        .await?;

    let mut files_removed = 0;
    if let Some(folder) = folder.filter(|f| !f.trim().is_empty()) {
        let path = std::path::PathBuf::from(&folder);
        let reach = tokio::task::spawn_blocking(move || files::reach(&path)).await.unwrap_or(files::Reach::Unreachable);
        if reach == files::Reach::Unreachable {
            // A drive that is not attached: keep the meeting as deleting rather than orphan its files
            warn!("Meeting {}: its recording folder cannot be reached; will retry", meeting_id);
            return Ok(MeetingDeletion { removed: false, files_pending: 1, files_removed: 0 });
        }
        if !sweep::folder_shared(pool, meeting_id, &folder).await? {
            let path = std::path::PathBuf::from(&folder);
            let report = tokio::task::spawn_blocking(move || recording_files::remove_recording(&path))
                .await
                .unwrap_or_default();
            files_removed = report.removed;
            if report.failed > 0 {
                warn!("Meeting {}: {} recording file(s) could not be removed yet; will retry", meeting_id, report.failed);
                return Ok(MeetingDeletion { removed: false, files_pending: report.failed, files_removed });
            }
        }
    }

    let removed = crate::database::repositories::meeting::MeetingsRepository::delete_meeting(pool, meeting_id).await?;
    crate::markdown_export::schedule(pool, meeting_id, crate::markdown_export::CHANGE_DELAY);
    info!("Meeting {} deleted ({} recording file(s) removed)", meeting_id, files_removed);
    Ok(MeetingDeletion { removed, files_pending: 0, files_removed })
}

/// Finishes deletes that were waiting on a locked file. Returns how many finished.
pub async fn retry_pending_deletes(pool: &SqlitePool) -> usize {
    let pending: Vec<(String,)> = match sqlx::query_as("SELECT id FROM meetings WHERE delete_requested_at IS NOT NULL")
        .fetch_all(pool)
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            warn!("Could not list meetings waiting to be deleted: {}", e);
            return 0;
        }
    };
    let mut finished = 0;
    for (id,) in pending {
        match delete_meeting(pool, &id).await {
            Ok(d) if d.removed => finished += 1,
            Ok(_) => {}
            Err(e) => warn!("Retrying the delete of meeting {} failed: {}", id, e),
        }
    }
    finished
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::path::Path;

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    async fn meeting(pool: &SqlitePool, id: &str, folder: Option<&Path>) {
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at, folder_path) VALUES (?, 'M', 'x', 'x', ?)")
            .bind(id)
            .bind(folder.map(|f| f.to_string_lossy().to_string()))
            .execute(pool)
            .await
            .unwrap();
    }

    fn recording(root: &Path, name: &str) -> std::path::PathBuf {
        let folder = root.join(name);
        std::fs::create_dir_all(&folder).unwrap();
        for f in ["audio.mp4", "metadata.json", "transcripts.json"] {
            std::fs::write(folder.join(f), b"x").unwrap();
        }
        folder
    }

    async fn exists(pool: &SqlitePool, id: &str) -> bool {
        sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM meetings WHERE id = ?").bind(id).fetch_one(pool).await.unwrap().0 > 0
    }

    #[tokio::test]
    async fn files_go_first_then_the_row() {
        let tmp = tempfile::tempdir().unwrap();
        let folder = recording(tmp.path(), "A");
        let pool = pool().await;
        meeting(&pool, "m1", Some(&folder)).await;
        let d = delete_meeting(&pool, "m1").await.unwrap();
        assert!(d.removed && d.files_pending == 0);
        assert!(!folder.exists());
        assert!(!exists(&pool, "m1").await);
        assert!(!delete_meeting(&pool, "m1").await.unwrap().removed, "a second delete is a no-op");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_locked_file_keeps_the_meeting_as_deleting_until_a_retry_succeeds() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let folder = recording(tmp.path(), "A");
        let pool = pool().await;
        meeting(&pool, "m1", Some(&folder)).await;
        // A read-only folder stands in for a Windows lock: its files cannot be removed
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o500)).unwrap();
        let d = delete_meeting(&pool, "m1").await.unwrap();
        assert!(!d.removed && d.files_pending > 0);
        assert!(exists(&pool, "m1").await, "the meeting stays, listed as deleting");
        let marked: (Option<String>,) = sqlx::query_as("SELECT delete_requested_at FROM meetings WHERE id = 'm1'").fetch_one(&pool).await.unwrap();
        assert!(marked.0.is_some());

        assert_eq!(retry_pending_deletes(&pool).await, 0, "still locked");
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(retry_pending_deletes(&pool).await, 1);
        assert!(!exists(&pool, "m1").await);
        assert!(!folder.exists(), "no orphan folder");
    }

    #[tokio::test]
    async fn a_meeting_on_an_unreachable_drive_waits_instead_of_orphaning_its_files() {
        let tmp = tempfile::tempdir().unwrap();
        let pool = pool().await;
        let folder = tmp.path().join("ExternalDrive").join("Rec");
        meeting(&pool, "m1", Some(&folder)).await;
        let d = delete_meeting(&pool, "m1").await.unwrap();
        assert!(!d.removed && d.files_pending > 0, "{d:?}");
        assert!(exists(&pool, "m1").await, "kept as deleting until the drive is back");
    }

    #[tokio::test]
    async fn a_shared_folder_stays_for_the_other_meeting() {
        let tmp = tempfile::tempdir().unwrap();
        let folder = recording(tmp.path(), "Shared");
        let pool = pool().await;
        meeting(&pool, "m1", Some(&folder)).await;
        meeting(&pool, "m2", Some(&folder)).await;
        assert!(delete_meeting(&pool, "m1").await.unwrap().removed);
        assert!(folder.join("audio.mp4").exists(), "m2 still uses it");
        assert!(delete_meeting(&pool, "m2").await.unwrap().removed);
        assert!(!folder.exists(), "the last meeting takes the folder with it");
    }

    #[tokio::test]
    async fn meetings_without_a_recording_or_with_a_foreign_folder_are_removed() {
        let tmp = tempfile::tempdir().unwrap();
        let foreign = tmp.path().join("Music");
        std::fs::create_dir_all(&foreign).unwrap();
        std::fs::write(foreign.join("song.mp3"), b"x").unwrap();
        let pool = pool().await;
        meeting(&pool, "m1", None).await;
        meeting(&pool, "m2", Some(&foreign)).await;
        assert!(delete_meeting(&pool, "m1").await.unwrap().removed);
        assert!(delete_meeting(&pool, "m2").await.unwrap().removed);
        assert!(foreign.join("song.mp3").exists(), "a folder without the app's markers is never touched");
    }
}
