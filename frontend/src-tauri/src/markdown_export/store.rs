//! Database side of the export: the settings row, one row per exported file, and loading a
//! meeting in the shape the renderer needs.

use chrono::{DateTime, Local, NaiveDateTime, Utc};
use serde::Serialize;
use sqlx::SqlitePool;

use super::render::{ExportLine, ExportMeeting};

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ExportSettings {
    pub enabled: bool,
    pub folder: Option<String>,
    pub confirmed_at: Option<String>,
    pub last_export_at: Option<String>,
}

/// What the app last wrote for a meeting: the file's path inside the export folder and the
/// SHA-256 of its bytes, used to tell an untouched file from one edited in another app.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct ExportRow {
    pub meeting_id: String,
    pub rel_path: String,
    pub sha256: String,
    pub exported_at: String,
}

pub async fn settings(pool: &SqlitePool) -> Result<ExportSettings, sqlx::Error> {
    let row: Option<(i64, Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT enabled, folder, confirmed_at, last_export_at FROM markdown_export_settings WHERE id = 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(row
        .map(|(enabled, folder, confirmed_at, last_export_at)| ExportSettings {
            enabled: enabled != 0,
            folder,
            confirmed_at,
            last_export_at,
        })
        .unwrap_or_default())
}

/// Turns the export on for `folder`. A different folder than before forgets the old rows, so
/// files in the old folder are never touched again.
pub async fn enable(pool: &SqlitePool, folder: &str) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    let previous: Option<(Option<String>,)> =
        sqlx::query_as("SELECT folder FROM markdown_export_settings WHERE id = 1").fetch_optional(&mut *tx).await?;
    if previous.and_then(|(f,)| f).as_deref() != Some(folder) {
        sqlx::query("DELETE FROM markdown_exports").execute(&mut *tx).await?;
    }
    sqlx::query(
        "INSERT INTO markdown_export_settings (id, enabled, folder, confirmed_at) VALUES (1, 1, ?1, ?2)
         ON CONFLICT(id) DO UPDATE SET enabled = 1, folder = ?1, confirmed_at = ?2",
    )
    .bind(folder)
    .bind(Utc::now().to_rfc3339())
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

pub async fn disable(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE markdown_export_settings SET enabled = 0 WHERE id = 1").execute(pool).await?;
    Ok(())
}

pub async fn mark_exported(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE markdown_export_settings SET last_export_at = ? WHERE id = 1")
        .bind(Utc::now().to_rfc3339())
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn row(pool: &SqlitePool, meeting_id: &str) -> Result<Option<ExportRow>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM markdown_exports WHERE meeting_id = ?").bind(meeting_id).fetch_optional(pool).await
}

pub async fn rows(pool: &SqlitePool) -> Result<Vec<ExportRow>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM markdown_exports ORDER BY rel_path").fetch_all(pool).await
}

pub async fn put_row(pool: &SqlitePool, meeting_id: &str, rel_path: &str, sha256: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO markdown_exports (meeting_id, rel_path, sha256, exported_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(meeting_id) DO UPDATE SET rel_path = ?2, sha256 = ?3, exported_at = ?4",
    )
    .bind(meeting_id)
    .bind(rel_path)
    .bind(sha256)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_row(pool: &SqlitePool, meeting_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM markdown_exports WHERE meeting_id = ?").bind(meeting_id).execute(pool).await?;
    Ok(())
}

/// Every meeting, plus every meeting that still has a row after being deleted.
pub async fn all_meeting_ids(pool: &SqlitePool) -> Result<Vec<String>, sqlx::Error> {
    let ids: Vec<(String,)> = sqlx::query_as(
        "SELECT id FROM meetings UNION SELECT meeting_id FROM markdown_exports ORDER BY 1",
    )
    .fetch_all(pool)
    .await?;
    Ok(ids.into_iter().map(|(id,)| id).collect())
}

/// Meetings filed in a folder, for a folder rename or delete.
pub async fn meeting_ids_in_folder(pool: &SqlitePool, folder_id: &str) -> Result<Vec<String>, sqlx::Error> {
    let ids: Vec<(String,)> =
        sqlx::query_as("SELECT id FROM meetings WHERE folder_id = ?").bind(folder_id).fetch_all(pool).await?;
    Ok(ids.into_iter().map(|(id,)| id).collect())
}

/// Meetings where this person is named as a speaker, for a rename, merge or delete.
pub async fn meeting_ids_for_person(pool: &SqlitePool, person_id: &str) -> Result<Vec<String>, sqlx::Error> {
    let ids: Vec<(String,)> = sqlx::query_as("SELECT DISTINCT meeting_id FROM meeting_speakers WHERE person_id = ?")
        .bind(person_id)
        .fetch_all(pool)
        .await?;
    Ok(ids.into_iter().map(|(id,)| id).collect())
}

/// The meeting a transcript line belongs to.
pub async fn meeting_id_for_transcript(pool: &SqlitePool, transcript_id: &str) -> Result<Option<String>, sqlx::Error> {
    let id: Option<(String,)> = sqlx::query_as("SELECT meeting_id FROM transcripts WHERE id = ?")
        .bind(transcript_id)
        .fetch_optional(pool)
        .await?;
    Ok(id.map(|(id,)| id))
}

/// Reads a stored timestamp in any of the forms the app has written, as UTC.
pub fn parse_stored_time(text: &str) -> Option<DateTime<Utc>> {
    let text = text.trim();
    if let Ok(t) = DateTime::parse_from_rfc3339(text) {
        return Some(t.with_timezone(&Utc));
    }
    for format in ["%Y-%m-%d %H:%M:%S%.f%:z", "%Y-%m-%d %H:%M:%S%.f%z"] {
        if let Ok(t) = DateTime::parse_from_str(text, format) {
            return Some(t.with_timezone(&Utc));
        }
    }
    for format in ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S%.f"] {
        if let Ok(t) = NaiveDateTime::parse_from_str(text, format) {
            return Some(t.and_utc());
        }
    }
    None
}

/// A meeting as the renderer needs it, or None if it no longer exists.
pub async fn load_meeting(pool: &SqlitePool, meeting_id: &str) -> Result<Option<ExportMeeting>, sqlx::Error> {
    let meeting: Option<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT m.id, m.title, CAST(m.created_at AS TEXT), f.name
         FROM meetings m LEFT JOIN folders f ON f.id = m.folder_id WHERE m.id = ?",
    )
    .bind(meeting_id)
    .fetch_optional(pool)
    .await?;
    let Some((id, title, created_at, folder)) = meeting else {
        return Ok(None);
    };
    let created_local = parse_stored_time(&created_at)
        .map(|t| t.with_timezone(&Local).naive_local())
        .unwrap_or_default();

    let notes: Option<(String,)> = sqlx::query_as("SELECT body_markdown FROM meeting_user_notes WHERE meeting_id = ?")
        .bind(meeting_id)
        .fetch_optional(pool)
        .await?;

    let summary: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT result FROM summary_processes WHERE meeting_id = ? AND LOWER(status) = 'completed'",
    )
    .bind(meeting_id)
    .fetch_optional(pool)
    .await?;
    let summary_markdown = summary
        .and_then(|(result,)| result)
        .and_then(|r| serde_json::from_str::<serde_json::Value>(&r).ok())
        .and_then(|v| v.get("markdown").and_then(|m| m.as_str()).map(str::to_string));

    let lines: Vec<(String, Option<f64>, Option<f64>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT t.transcript, t.audio_start_time, t.audio_end_time, t.speaker, p.name
         FROM transcripts t
         LEFT JOIN meeting_speakers ms ON ms.meeting_id = t.meeting_id AND ms.speaker_key = t.speaker
         LEFT JOIN people p ON p.id = ms.person_id
         WHERE t.meeting_id = ?
         ORDER BY COALESCE(t.audio_start_time, 0), t.timestamp, t.id",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await?;
    let lines = lines
        .into_iter()
        .map(|(text, start, end, key, person)| {
            let start = start.unwrap_or(0.0);
            ExportLine {
                start,
                end: end.unwrap_or(start),
                speaker: crate::ask::policy::speaker_name(key.as_deref(), person.as_deref()),
                text,
            }
        })
        .collect();

    Ok(Some(ExportMeeting {
        id,
        title,
        created_local,
        folder,
        notes_markdown: notes.map(|(n,)| n),
        summary_markdown,
        lines,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_times_in_every_written_form_are_read() {
        let want = "2026-11-02T14:05:07Z";
        for text in [
            "2026-11-02T14:05:07Z",
            "2026-11-02T15:05:07+01:00",
            "2026-11-02 14:05:07.000000+00:00",
            "2026-11-02 14:05:07",
            "2026-11-02T14:05:07.5",
        ] {
            let t = parse_stored_time(text).unwrap_or_else(|| panic!("{text}"));
            assert_eq!(t.format("%Y-%m-%dT%H:%M:%SZ").to_string(), want, "{text}");
        }
        assert!(parse_stored_time("x").is_none());
    }

    #[tokio::test]
    async fn people_and_lines_lead_back_to_their_meetings() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        for id in ["m1", "m2"] {
            sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES (?, 'M', 'x', 'x')").bind(id).execute(&pool).await.unwrap();
        }
        let person = crate::database::repositories::people::PeopleRepository::get_or_create_person(&pool, "Alex").await.unwrap();
        for id in ["m1", "m2"] {
            sqlx::query("INSERT INTO meeting_speakers (id, meeting_id, speaker_key, person_id, created_at) VALUES (?, ?, 'S1', ?, 'x')")
                .bind(format!("s-{id}"))
                .bind(id)
                .bind(&person.id)
                .execute(&pool)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO transcripts (id, meeting_id, transcript, timestamp) VALUES ('t1', 'm2', 'hi', '10:00')").execute(&pool).await.unwrap();
        assert_eq!(meeting_ids_for_person(&pool, &person.id).await.unwrap().len(), 2);
        assert_eq!(meeting_id_for_transcript(&pool, "t1").await.unwrap().as_deref(), Some("m2"));
        assert_eq!(meeting_id_for_transcript(&pool, "t9").await.unwrap(), None);
    }
}
