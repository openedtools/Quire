//! The database side of the vault: settings, the stable vault IDs, what was last written, and
//! the rows a vault file is made from (spec F-SYNC-4, `sync-format.md` section 16).

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::SqlitePool;

use super::format::{self, Segment};
use crate::markdown_export::store::parse_stored_time;

#[derive(Debug, Clone, Default, Serialize)]
pub struct VaultSettings {
    pub enabled: bool,
    pub folder: Option<String>,
    pub device_id: Option<String>,
    pub vault_id: Option<String>,
    pub disclosed_at: Option<String>,
    pub last_export_at: Option<String>,
    pub last_report: Option<String>,
}

pub async fn settings(pool: &SqlitePool) -> Result<VaultSettings, sqlx::Error> {
    let row: Option<(i64, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT enabled, folder, device_id, vault_id, disclosed_at, last_export_at, last_report FROM vault_settings WHERE id = 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map_or_else(VaultSettings::default, |(enabled, folder, device_id, vault_id, disclosed_at, last_export_at, last_report)| VaultSettings {
        enabled: enabled != 0,
        folder,
        device_id,
        vault_id,
        disclosed_at,
        last_export_at,
        last_report,
    }))
}

/// This install's device ID, made once (`sync-format.md` section 3).
pub async fn device_id(pool: &SqlitePool) -> Result<String, sqlx::Error> {
    if let Some(id) = settings(pool).await?.device_id {
        return Ok(id);
    }
    let id = format!("dev-{:08x}", rand::random::<u32>());
    sqlx::query("INSERT INTO vault_settings (id, device_id) VALUES (1, ?) ON CONFLICT(id) DO UPDATE SET device_id = COALESCE(vault_settings.device_id, excluded.device_id)")
        .bind(&id)
        .execute(pool)
        .await?;
    Ok(settings(pool).await?.device_id.unwrap_or(id))
}

/// Turns the vault on for a folder the person chose and confirmed (F-SYNC-1, F-SYNC-2). A new
/// folder starts with no record of what was written.
pub async fn enable(pool: &SqlitePool, folder: &str) -> Result<(), sqlx::Error> {
    let previous = settings(pool).await?.folder;
    let mut tx = pool.begin().await?;
    if previous.as_deref() != Some(folder) {
        sqlx::query("DELETE FROM sync_state").execute(&mut *tx).await?;
        sqlx::query("DELETE FROM sync_pending").execute(&mut *tx).await?;
    }
    sqlx::query(
        "INSERT INTO vault_settings (id, enabled, folder, disclosed_at) VALUES (1, 1, ?, ?)
         ON CONFLICT(id) DO UPDATE SET enabled = 1, folder = excluded.folder,
           vault_id = CASE WHEN vault_settings.folder = excluded.folder THEN vault_settings.vault_id ELSE NULL END,
           disclosed_at = COALESCE(vault_settings.disclosed_at, excluded.disclosed_at)",
    )
    .bind(folder)
    .bind(format::timestamp(Utc::now()))
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

/// Stops writing; the files stay where they are (F-SYNC-9).
pub async fn disable(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE vault_settings SET enabled = 0 WHERE id = 1").execute(pool).await?;
    Ok(())
}

pub async fn set_vault_id(pool: &SqlitePool, vault_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE vault_settings SET vault_id = ? WHERE id = 1").bind(vault_id).execute(pool).await?;
    Ok(())
}

pub async fn record_run(pool: &SqlitePool, report_json: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE vault_settings SET last_export_at = ?, last_report = ? WHERE id = 1")
        .bind(format::timestamp(Utc::now()))
        .bind(report_json)
        .execute(pool)
        .await?;
    Ok(())
}

pub fn stored_time(text: &str) -> Option<DateTime<Utc>> {
    parse_stored_time(text)
}

fn millis(text: &str) -> u64 {
    stored_time(text).map_or(0, |t| t.timestamp_millis().max(0) as u64)
}

/// Gives every meeting, person and folder its vault ID the first time, from its creation time,
/// and never changes it after that (`sync-format.md` 16.2).
pub async fn assign_sync_ids(pool: &SqlitePool) -> Result<usize, sqlx::Error> {
    let mut assigned = 0;
    for table in ["meetings", "people", "folders"] {
        let rows: Vec<(String, Option<String>)> =
            sqlx::query_as(&format!("SELECT id, CAST(created_at AS TEXT) FROM {table} WHERE sync_id IS NULL")).fetch_all(pool).await?;
        for (id, created) in rows {
            let ulid = format::new_ulid(millis(created.as_deref().unwrap_or("")));
            assigned += sqlx::query(&format!("UPDATE {table} SET sync_id = ? WHERE id = ? AND sync_id IS NULL"))
                .bind(ulid)
                .bind(&id)
                .execute(pool)
                .await?
                .rows_affected() as usize;
        }
    }
    Ok(assigned)
}

fn iso(text: Option<&str>) -> Option<String> {
    text.and_then(stored_time).map(format::timestamp)
}

#[derive(Debug, Clone)]
pub struct FolderRow {
    pub sync_id: String,
    pub name: String,
    pub created: String,
    pub updated: String,
}

pub async fn folders(pool: &SqlitePool) -> Result<Vec<FolderRow>, sqlx::Error> {
    let rows: Vec<(String, String, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT sync_id, name, CAST(created_at AS TEXT), CAST(updated_at AS TEXT) FROM folders WHERE sync_id IS NOT NULL")
            .fetch_all(pool)
            .await?;
    Ok(rows
        .into_iter()
        .map(|(sync_id, name, created, updated)| {
            let created = iso(created.as_deref()).unwrap_or_else(|| format::timestamp(DateTime::<Utc>::UNIX_EPOCH));
            FolderRow { sync_id, name, updated: iso(updated.as_deref()).unwrap_or_else(|| created.clone()), created }
        })
        .collect())
}

#[derive(Debug, Clone)]
pub struct PersonRow {
    pub sync_id: String,
    pub name: String,
    pub notes: Option<String>,
    pub created: String,
    pub updated: String,
}

pub async fn people(pool: &SqlitePool) -> Result<Vec<PersonRow>, sqlx::Error> {
    let rows: Vec<(String, String, Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT sync_id, name, notes, CAST(created_at AS TEXT), CAST(updated_at AS TEXT) FROM people WHERE sync_id IS NOT NULL",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(sync_id, name, notes, created, updated)| {
            let created = iso(created.as_deref()).unwrap_or_else(|| format::timestamp(DateTime::<Utc>::UNIX_EPOCH));
            PersonRow { sync_id, name, notes, updated: iso(updated.as_deref()).unwrap_or_else(|| created.clone()), created }
        })
        .collect())
}

/// Meetings to export: every one with a vault ID, except those being deleted.
pub async fn meeting_ids(pool: &SqlitePool) -> Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT id FROM meetings WHERE sync_id IS NOT NULL AND delete_requested_at IS NULL ORDER BY created_at, id")
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

#[derive(Debug, Clone)]
pub struct SpeakerRow {
    pub key: String,
    pub name: String,
    pub person_sync_id: Option<String>,
}

/// Everything one note and its sidecar are made from.
#[derive(Debug, Clone)]
pub struct NoteSource {
    pub sync_id: String,
    pub title: String,
    pub created: String,
    pub updated: String,
    pub folder_sync_id: Option<String>,
    pub mode: String,
    pub retention: Option<String>,
    pub expires: Option<String>,
    pub audio_state: String,
    pub audio_deleted_at: Option<String>,
    /// Each line with its speaker key
    pub lines: Vec<(Segment, Option<String>)>,
    pub speakers: Vec<SpeakerRow>,
    pub summary_status: Option<String>,
    pub summary_at: Option<String>,
    pub summary_model: Option<String>,
    pub summary_markdown: Option<String>,
    pub notes_markdown: Option<String>,
    /// `thoughts` for a recording of thoughts (Phase N, sync-format section 6)
    pub kind: Option<String>,
}

type MeetingRow = (String, String, Option<String>, Option<String>, Option<String>, String, Option<String>, Option<String>, String, Option<String>);

pub async fn note_source(pool: &SqlitePool, meeting_id: &str) -> Result<Option<NoteSource>, sqlx::Error> {
    let row: Option<MeetingRow> = sqlx::query_as(
        "SELECT m.sync_id, m.title, CAST(m.created_at AS TEXT), CAST(m.updated_at AS TEXT), f.sync_id, m.mode,
                m.audio_retention, m.audio_expires_at, m.audio_state, m.audio_deleted_at
         FROM meetings m LEFT JOIN folders f ON f.id = m.folder_id WHERE m.id = ? AND m.sync_id IS NOT NULL",
    )
    .bind(meeting_id)
    .fetch_optional(pool)
    .await?;
    let Some((sync_id, title, created, updated, folder_sync_id, mode, retention, expires, audio_state, audio_deleted_at)) = row else {
        return Ok(None);
    };
    let created = iso(created.as_deref()).unwrap_or_else(|| format::timestamp(DateTime::<Utc>::UNIX_EPOCH));
    let updated = iso(updated.as_deref()).unwrap_or_else(|| created.clone());

    let lines: Vec<(String, String, Option<f64>, Option<f64>, Option<String>)> = sqlx::query_as(
        "SELECT id, transcript, audio_start_time, audio_end_time, speaker FROM transcripts WHERE meeting_id = ?
         ORDER BY COALESCE(audio_start_time, 0), timestamp, id",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await?;
    let lines = lines
        .into_iter()
        .map(|(id, text, start, end, key)| {
            let start = start.unwrap_or(0.0);
            (Segment { id: format!("seg-{id}"), start, end: end.unwrap_or(start), speaker: None, text }, key)
        })
        .collect();

    let speakers: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT ms.speaker_key, p.name, p.sync_id FROM meeting_speakers ms LEFT JOIN people p ON p.id = ms.person_id
         WHERE ms.meeting_id = ? ORDER BY ms.speaker_key",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await?;
    let speakers = speakers
        .into_iter()
        .map(|(key, name, person_sync_id)| SpeakerRow {
            name: crate::ask::policy::speaker_name(Some(&key), name.as_deref()).unwrap_or_else(|| key.clone()),
            key,
            person_sync_id,
        })
        .collect();

    let summary: Option<(String, Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT status, result, CAST(updated_at AS TEXT), metadata FROM summary_processes WHERE meeting_id = ?",
    )
    .bind(meeting_id)
    .fetch_optional(pool)
    .await?;
    let (summary_status, summary_at, summary_model, summary_markdown) = match summary {
        None => (None, None, None, None),
        Some((status, result, at, metadata)) => {
            let status = match status.to_ascii_lowercase().as_str() {
                "completed" => "done",
                "failed" | "error" | "cancelled" => "failed",
                "pending" | "processing" | "started" | "running" => "pending",
                _ => "none",
            };
            let markdown = (status == "done")
                .then(|| result.and_then(|r| serde_json::from_str::<serde_json::Value>(&r).ok()))
                .flatten()
                .and_then(|v| v.get("markdown").and_then(|m| m.as_str()).map(str::to_string));
            let model = metadata
                .and_then(|m| serde_json::from_str::<serde_json::Value>(&m).ok())
                .and_then(|v| v.get("model").or_else(|| v.get("model_name")).and_then(|m| m.as_str()).map(str::to_string));
            (Some(status.to_string()), (status == "done").then(|| iso(at.as_deref())).flatten(), model, markdown)
        }
    };

    let notes: Option<(String,)> = sqlx::query_as("SELECT body_markdown FROM meeting_user_notes WHERE meeting_id = ?")
        .bind(meeting_id)
        .fetch_optional(pool)
        .await?;

    let kind: Option<String> = sqlx::query_scalar("SELECT kind FROM meetings WHERE id = ?").bind(meeting_id).fetch_optional(pool).await?.flatten();
    Ok(Some(NoteSource {
        kind,
        sync_id,
        title,
        created,
        updated,
        folder_sync_id,
        mode,
        retention,
        expires: iso(expires.as_deref()),
        audio_state,
        audio_deleted_at: iso(audio_deleted_at.as_deref()),
        lines,
        speakers,
        summary_status,
        summary_at,
        summary_model,
        summary_markdown,
        notes_markdown: notes.map(|(n,)| n),
    }))
}

#[derive(Debug, Clone, PartialEq)]
pub struct StateRow {
    pub sync_id: String,
    pub kind: String,
    pub rel_path: String,
    pub sha256: String,
    pub content_key: Option<String>,
    pub rev: Option<i64>,
    pub updated: Option<String>,
    pub exported_at: String,
}

pub async fn state(pool: &SqlitePool) -> Result<Vec<StateRow>, sqlx::Error> {
    let rows: Vec<(String, String, String, String, Option<String>, Option<i64>, Option<String>, String)> =
        sqlx::query_as("SELECT sync_id, kind, rel_path, sha256, content_key, rev, updated, exported_at FROM sync_state")
            .fetch_all(pool)
            .await?;
    Ok(rows
        .into_iter()
        .map(|(sync_id, kind, rel_path, sha256, content_key, rev, updated, exported_at)| StateRow {
            sync_id,
            kind,
            rel_path,
            sha256,
            content_key,
            rev,
            updated,
            exported_at,
        })
        .collect())
}

/// Replaces the whole record in one transaction, after the vault has been verified.
pub async fn replace_state(pool: &SqlitePool, rows: &[StateRow]) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM sync_state").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM sync_pending").execute(&mut *tx).await?;
    for r in rows {
        sqlx::query(
            "INSERT INTO sync_state (sync_id, kind, rel_path, sha256, content_key, rev, updated, exported_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&r.sync_id)
        .bind(&r.kind)
        .bind(&r.rel_path)
        .bind(&r.sha256)
        .bind(&r.content_key)
        .bind(r.rev)
        .bind(&r.updated)
        .bind(&r.exported_at)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

pub async fn clear_state(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM sync_state").execute(pool).await?;
    sqlx::query("DELETE FROM sync_pending").execute(pool).await?;
    Ok(())
}

/// Records a file just before it is written (see `sync_pending`).
pub async fn put_pending(pool: &SqlitePool, r: &StateRow) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT OR REPLACE INTO sync_pending (sync_id, kind, rel_path, sha256, content_key, rev, updated, exported_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&r.sync_id)
    .bind(&r.kind)
    .bind(&r.rel_path)
    .bind(&r.sha256)
    .bind(&r.content_key)
    .bind(r.rev)
    .bind(&r.updated)
    .bind(&r.exported_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn pending(pool: &SqlitePool) -> Result<Vec<StateRow>, sqlx::Error> {
    let rows: Vec<(String, String, String, String, Option<String>, Option<i64>, Option<String>, String)> =
        sqlx::query_as("SELECT sync_id, kind, rel_path, sha256, content_key, rev, updated, exported_at FROM sync_pending")
            .fetch_all(pool)
            .await?;
    Ok(rows
        .into_iter()
        .map(|(sync_id, kind, rel_path, sha256, content_key, rev, updated, exported_at)| StateRow { sync_id, kind, rel_path, sha256, content_key, rev, updated, exported_at })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn vault_ids_take_the_creation_time_and_never_change() {
        let pool = pool().await;
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES ('meeting-1', 'Plan', '2026-10-03T18:00:00Z', '2026-10-03T18:00:00Z')")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(assign_sync_ids(&pool).await.unwrap(), 1);
        let first: String = sqlx::query_scalar("SELECT sync_id FROM meetings").fetch_one(&pool).await.unwrap();
        assert!(format::is_ulid(&first));
        let expected_prefix = format::ulid(1_791_050_400_000, [0; 10]);
        assert_eq!(&first[..10], &expected_prefix[..10], "the time part is the meeting's creation time");
        assert_eq!(assign_sync_ids(&pool).await.unwrap(), 0);
        let again: String = sqlx::query_scalar("SELECT sync_id FROM meetings").fetch_one(&pool).await.unwrap();
        assert_eq!(first, again);
        let old_id: String = sqlx::query_scalar("SELECT id FROM meetings").fetch_one(&pool).await.unwrap();
        assert_eq!(old_id, "meeting-1", "existing IDs are untouched");
    }

    #[tokio::test]
    async fn the_device_id_is_made_once() {
        let pool = pool().await;
        let a = device_id(&pool).await.unwrap();
        assert!(a.starts_with("dev-") && a.len() == 12);
        assert_eq!(device_id(&pool).await.unwrap(), a);
    }

    #[tokio::test]
    async fn choosing_another_folder_forgets_what_was_written_to_the_old_one() {
        let pool = pool().await;
        enable(&pool, "/vault/a").await.unwrap();
        let row = StateRow { sync_id: "x".into(), kind: "note".into(), rel_path: "x.md".into(), sha256: "s".into(), content_key: None, rev: Some(1), updated: None, exported_at: "t".into() };
        replace_state(&pool, &[row]).await.unwrap();
        enable(&pool, "/vault/a").await.unwrap();
        assert_eq!(state(&pool).await.unwrap().len(), 1, "same folder: kept");
        enable(&pool, "/vault/b").await.unwrap();
        assert!(state(&pool).await.unwrap().is_empty());
        disable(&pool).await.unwrap();
        assert!(!settings(&pool).await.unwrap().enabled);
    }
}
