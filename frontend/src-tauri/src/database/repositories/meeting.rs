use crate::api::{MeetingDetails, MeetingTranscript};
use crate::database::models::{MeetingModel, Transcript};
use chrono::Utc;
use sqlx::{Connection, Error as SqlxError, SqliteConnection, SqlitePool};
use tracing::{error, info};
use uuid::Uuid;

pub struct MeetingsRepository;

impl MeetingsRepository {
    pub async fn get_meetings(pool: &SqlitePool) -> Result<Vec<MeetingModel>, sqlx::Error> {
        let meetings =
            sqlx::query_as::<_, MeetingModel>("SELECT * FROM meetings ORDER BY created_at DESC")
                .fetch_all(pool)
                .await?;
        Ok(meetings)
    }

    /// A meeting with no recording yet, for notes-first meetings (spec F-NM-1).
    pub async fn create_meeting(pool: &SqlitePool, title: &str) -> Result<String, SqlxError> {
        let meeting_id = format!("meeting-{}", Uuid::new_v4());
        let now = Utc::now();
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES (?, ?, ?, ?)")
            .bind(&meeting_id)
            .bind(title)
            .bind(now)
            .bind(now)
            .execute(pool)
            .await?;
        info!("Created meeting {} with no recording", meeting_id);
        Ok(meeting_id)
    }

    /// Removes a notes-first meeting the person never used (spec F-NM-5).
    /// Only the database row goes; a meeting with notes, a recording folder, transcript lines,
    /// a new title or a place in a folder is kept, so a recorded or adopted meeting can never match.
    pub async fn discard_if_empty(pool: &SqlitePool, meeting_id: &str, default_title: &str) -> Result<bool, SqlxError> {
        let mut transaction = pool.begin().await?;
        let result = sqlx::query(
            "DELETE FROM meetings
             WHERE id = ?1 AND title = ?2 AND folder_path IS NULL AND folder_id IS NULL
               AND NOT EXISTS (SELECT 1 FROM transcripts WHERE meeting_id = ?1)
               AND NOT EXISTS (SELECT 1 FROM transcript_chunks WHERE meeting_id = ?1)
               AND NOT EXISTS (SELECT 1 FROM summary_processes WHERE meeting_id = ?1)
               AND NOT EXISTS (SELECT 1 FROM meeting_speakers WHERE meeting_id = ?1)
               AND NOT EXISTS (SELECT 1 FROM meeting_user_notes WHERE meeting_id = ?1 AND trim(body_markdown, ' ' || char(9) || char(10) || char(13)) <> '')",
        )
        .bind(meeting_id)
        .bind(default_title)
        .execute(&mut *transaction)
        .await?;
        if result.rows_affected() == 0 {
            transaction.rollback().await?;
            return Ok(false);
        }
        sqlx::query("DELETE FROM meeting_user_notes WHERE meeting_id = ?")
            .bind(meeting_id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        info!("Discarded unused meeting {}", meeting_id);
        Ok(true)
    }

    pub async fn delete_meeting(pool: &SqlitePool, meeting_id: &str) -> Result<bool, SqlxError> {
        if meeting_id.trim().is_empty() {
            return Err(SqlxError::Protocol(
                "meeting_id cannot be empty".to_string(),
            ));
        }

        let mut conn = pool.acquire().await?;
        let mut transaction = conn.begin().await?;

        match delete_meeting_with_transaction(&mut transaction, meeting_id).await {
            Ok(success) => {
                if success {
                    transaction.commit().await?;
                    info!(
                        "Successfully deleted meeting {} and all associated data",
                        meeting_id
                    );
                    Ok(true)
                } else {
                    transaction.rollback().await?;
                    Ok(false)
                }
            }
            Err(e) => {
                let _ = transaction.rollback().await;
                error!("Failed to delete meeting {}: {}", meeting_id, e);
                Err(e)
            }
        }
    }

    pub async fn get_meeting(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Option<MeetingDetails>, SqlxError> {
        if meeting_id.trim().is_empty() {
            return Err(SqlxError::Protocol(
                "meeting_id cannot be empty".to_string(),
            ));
        }

        let mut conn = pool.acquire().await?;
        let mut transaction = conn.begin().await?;

        // Get meeting details
        let meeting: Option<MeetingModel> =
            sqlx::query_as("SELECT id, title, created_at, updated_at, folder_path FROM meetings WHERE id = ?")
                .bind(meeting_id)
                .fetch_optional(&mut *transaction)
                .await?;

        if meeting.is_none() {
            transaction.rollback().await?;
            return Err(SqlxError::RowNotFound);
        }

        if let Some(meeting) = meeting {
            // Get all transcripts for this meeting
            let transcripts =
                sqlx::query_as::<_, Transcript>("SELECT * FROM transcripts WHERE meeting_id = ?")
                    .bind(meeting_id)
                    .fetch_all(&mut *transaction)
                    .await?;

            transaction.commit().await?;

            // Convert Transcript to MeetingTranscript
            let meeting_transcripts = transcripts
                .into_iter()
                .map(|t| MeetingTranscript {
                    id: t.id,
                    text: t.transcript,
                    timestamp: t.timestamp,
                    audio_start_time: t.audio_start_time,
                    audio_end_time: t.audio_end_time,
                    duration: t.duration,
                    speaker: t.speaker,
                })
                .collect::<Vec<_>>();

            Ok(Some(MeetingDetails {
                id: meeting.id,
                title: meeting.title,
                created_at: meeting.created_at.0.to_rfc3339(),
                updated_at: meeting.updated_at.0.to_rfc3339(),
                transcripts: meeting_transcripts,
            }))
        } else {
            transaction.rollback().await?;
            Ok(None)
        }
    }

    /// Get meeting metadata without transcripts (for pagination)
    pub async fn get_meeting_metadata(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Option<MeetingModel>, SqlxError> {
        if meeting_id.trim().is_empty() {
            return Err(SqlxError::Protocol(
                "meeting_id cannot be empty".to_string(),
            ));
        }

        let meeting: Option<MeetingModel> =
            sqlx::query_as("SELECT id, title, created_at, updated_at, folder_path FROM meetings WHERE id = ?")
                .bind(meeting_id)
                .fetch_optional(pool)
                .await?;

        Ok(meeting)
    }

    /// Get meeting transcripts with pagination support
    pub async fn get_meeting_transcripts_paginated(
        pool: &SqlitePool,
        meeting_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<Transcript>, i64), SqlxError> {
        if meeting_id.trim().is_empty() {
            return Err(SqlxError::Protocol(
                "meeting_id cannot be empty".to_string(),
            ));
        }

        // Get total count of transcripts for this meeting
        let total: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM transcripts WHERE meeting_id = ?"
        )
        .bind(meeting_id)
        .fetch_one(pool)
        .await?;

        // Get paginated transcripts ordered by audio_start_time
        let transcripts = sqlx::query_as::<_, Transcript>(
            "SELECT * FROM transcripts
             WHERE meeting_id = ?
             ORDER BY audio_start_time ASC
             LIMIT ? OFFSET ?"
        )
        .bind(meeting_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        Ok((transcripts, total.0))
    }

    pub async fn update_meeting_title(
        pool: &SqlitePool,
        meeting_id: &str,
        new_title: &str,
    ) -> Result<bool, SqlxError> {
        if meeting_id.trim().is_empty() {
            return Err(SqlxError::Protocol(
                "meeting_id cannot be empty".to_string(),
            ));
        }

        let mut conn = pool.acquire().await?;
        let mut transaction = conn.begin().await?;

        let now = Utc::now().naive_utc();

        let rows_affected =
            sqlx::query("UPDATE meetings SET title = ?, updated_at = ? WHERE id = ?")
                .bind(new_title)
                .bind(now)
                .bind(meeting_id)
                .execute(&mut *transaction)
                .await?;
        if rows_affected.rows_affected() == 0 {
            transaction.rollback().await?;
            return Ok(false);
        }
        transaction.commit().await?;
        Ok(true)
    }

    pub async fn update_meeting_name(
        pool: &SqlitePool,
        meeting_id: &str,
        new_title: &str,
    ) -> Result<bool, SqlxError> {
        let mut transaction = pool.begin().await?;
        let now = Utc::now();

        // Update meetings table
        let meeting_update =
            sqlx::query("UPDATE meetings SET title = ?, updated_at = ? WHERE id = ?")
                .bind(new_title)
                .bind(now)
                .bind(meeting_id)
                .execute(&mut *transaction)
                .await?;

        if meeting_update.rows_affected() == 0 {
            transaction.rollback().await?;
            return Ok(false); // Meeting not found
        }

        // Update transcript_chunks table
        sqlx::query("UPDATE transcript_chunks SET meeting_name = ? WHERE meeting_id = ?")
            .bind(new_title)
            .bind(meeting_id)
            .execute(&mut *transaction)
            .await?;

        transaction.commit().await?;
        Ok(true)
    }
}

async fn delete_meeting_with_transaction(
    transaction: &mut SqliteConnection,
    meeting_id: &str,
) -> Result<bool, SqlxError> {
    // Check if meeting exists
    let meeting_exists: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM meetings WHERE id = ?")
        .bind(meeting_id)
        .fetch_optional(&mut *transaction)
        .await?;

    if meeting_exists.is_none() {
        error!("Meeting {} not found for deletion", meeting_id);
        return Ok(false);
    }

    // Delete from related tables in proper order
    // 1. Delete from transcript_chunks
    sqlx::query("DELETE FROM transcript_chunks WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    // 2. Delete from summary_processes
    sqlx::query("DELETE FROM summary_processes WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    // 3. Delete from transcripts
    sqlx::query("DELETE FROM transcripts WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    // 4. Delete speaker slots (people themselves are kept in the directory)
    sqlx::query("DELETE FROM meeting_speakers WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    // 5. The person's own notes for this meeting
    sqlx::query("DELETE FROM meeting_user_notes WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    // 6. Finally, delete the meeting
    let result = sqlx::query("DELETE FROM meetings WHERE id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    Ok(result.rows_affected() > 0)
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

    async fn count(pool: &SqlitePool, sql: &str) -> i64 {
        sqlx::query_as::<_, (i64,)>(sql).fetch_one(pool).await.unwrap().0
    }

    #[tokio::test]
    async fn a_new_meeting_has_its_title_and_no_recording() {
        let pool = pool().await;
        let id = MeetingsRepository::create_meeting(&pool, "Meeting, 2 Nov 2026 14:05").await.unwrap();
        assert!(id.starts_with("meeting-"));
        let row = MeetingsRepository::get_meeting_metadata(&pool, &id).await.unwrap().unwrap();
        assert_eq!(row.title, "Meeting, 2 Nov 2026 14:05");
        assert!(row.folder_path.is_none());
        assert_eq!(count(&pool, "SELECT COUNT(*) FROM transcripts").await, 0);
    }

    #[tokio::test]
    async fn an_untouched_new_meeting_is_discarded() {
        let pool = pool().await;
        let id = MeetingsRepository::create_meeting(&pool, "Meeting A").await.unwrap();
        assert!(MeetingsRepository::discard_if_empty(&pool, &id, "Meeting A").await.unwrap());
        assert_eq!(count(&pool, "SELECT COUNT(*) FROM meetings").await, 0);
    }

    #[tokio::test]
    async fn blank_notes_still_count_as_untouched() {
        let pool = pool().await;
        let id = MeetingsRepository::create_meeting(&pool, "Meeting A").await.unwrap();
        sqlx::query("INSERT INTO meeting_user_notes (meeting_id, body_blocks, body_markdown, updated_at) VALUES (?, '[]', '  \n', 'x')")
            .bind(&id).execute(&pool).await.unwrap();
        assert!(MeetingsRepository::discard_if_empty(&pool, &id, "Meeting A").await.unwrap());
        assert_eq!(count(&pool, "SELECT COUNT(*) FROM meeting_user_notes").await, 0);
    }

    #[tokio::test]
    async fn a_used_meeting_is_never_discarded() {
        let pool = pool().await;
        // Notes
        let notes = MeetingsRepository::create_meeting(&pool, "Meeting A").await.unwrap();
        sqlx::query("INSERT INTO meeting_user_notes (meeting_id, body_blocks, body_markdown, updated_at) VALUES (?, '[]', 'agenda', 'x')")
            .bind(&notes).execute(&pool).await.unwrap();
        // Renamed
        let renamed = MeetingsRepository::create_meeting(&pool, "Meeting A").await.unwrap();
        MeetingsRepository::update_meeting_title(&pool, &renamed, "Budget").await.unwrap();
        // A transcript
        let transcript = MeetingsRepository::create_meeting(&pool, "Meeting A").await.unwrap();
        sqlx::query("INSERT INTO transcripts (id, meeting_id, transcript, timestamp) VALUES ('t1', ?, 'hello', '10:00')")
            .bind(&transcript).execute(&pool).await.unwrap();
        // A recording folder (an adopted or recorded meeting)
        let folder = MeetingsRepository::create_meeting(&pool, "Meeting A").await.unwrap();
        sqlx::query("UPDATE meetings SET folder_path = 'C:/rec' WHERE id = ?").bind(&folder).execute(&pool).await.unwrap();

        for id in [&notes, &renamed, &transcript, &folder] {
            assert!(!MeetingsRepository::discard_if_empty(&pool, id, "Meeting A").await.unwrap(), "{id} was discarded");
        }
        assert_eq!(count(&pool, "SELECT COUNT(*) FROM meetings").await, 4);
        assert_eq!(count(&pool, "SELECT COUNT(*) FROM meeting_user_notes").await, 1);
    }

    #[tokio::test]
    async fn discarding_a_missing_meeting_is_a_no_op() {
        let pool = pool().await;
        assert!(!MeetingsRepository::discard_if_empty(&pool, "meeting-missing", "Meeting A").await.unwrap());
    }
}

#[cfg(test)]
mod review_tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn a_new_meeting_filed_in_a_folder_is_kept() {
        let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO folders (id, name, created_at, updated_at) VALUES ('f1', 'Planning', 'x', 'x')")
            .execute(&pool).await.unwrap();
        let id = MeetingsRepository::create_meeting(&pool, "Meeting A").await.unwrap();
        sqlx::query("UPDATE meetings SET folder_id = 'f1' WHERE id = ?").bind(&id).execute(&pool).await.unwrap();
        assert!(!MeetingsRepository::discard_if_empty(&pool, &id, "Meeting A").await.unwrap());
        let left: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM meetings").fetch_one(&pool).await.unwrap();
        assert_eq!(left.0, 1);
    }
}
