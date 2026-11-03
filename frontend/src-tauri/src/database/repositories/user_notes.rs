use crate::notes::models::UserNotes;
use chrono::Utc;
use sqlx::{Error as SqlxError, SqlitePool};

/// Notes larger than this are refused: far beyond what a person types in a meeting.
pub const MAX_NOTES_BYTES: usize = 1_000_000;

#[derive(Debug, thiserror::Error)]
pub enum UserNotesError {
    /// A problem with what the caller asked for; the message is safe to show in the UI.
    #[error("{0}")]
    Invalid(String),
    #[error("Database error: {0}")]
    Db(#[from] SqlxError),
}

pub struct UserNotesRepository;

impl UserNotesRepository {
    pub async fn get(pool: &SqlitePool, meeting_id: &str) -> Result<Option<UserNotes>, UserNotesError> {
        let notes = sqlx::query_as::<_, UserNotes>("SELECT * FROM meeting_user_notes WHERE meeting_id = ?")
            .bind(meeting_id)
            .fetch_optional(pool)
            .await?;
        Ok(notes)
    }

    /// Store the notes for a meeting, replacing what was there. Notes with no text remove the
    /// row, so "has notes" always means there is something to use in a summary.
    pub async fn save(
        pool: &SqlitePool,
        meeting_id: &str,
        body_blocks: &str,
        body_markdown: &str,
    ) -> Result<Option<UserNotes>, UserNotesError> {
        if body_blocks.len() + body_markdown.len() > MAX_NOTES_BYTES {
            return Err(UserNotesError::Invalid("These notes are too long to save".to_string()));
        }
        let exists: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM meetings WHERE id = ?")
            .bind(meeting_id)
            .fetch_optional(pool)
            .await?;
        if exists.is_none() {
            return Err(UserNotesError::Invalid("That meeting no longer exists".to_string()));
        }

        if body_markdown.trim().is_empty() {
            sqlx::query("DELETE FROM meeting_user_notes WHERE meeting_id = ?")
                .bind(meeting_id)
                .execute(pool)
                .await?;
            return Ok(None);
        }

        let notes = UserNotes {
            meeting_id: meeting_id.to_string(),
            body_blocks: body_blocks.to_string(),
            body_markdown: body_markdown.to_string(),
            updated_at: Utc::now().to_rfc3339(),
        };
        sqlx::query(
            r#"INSERT INTO meeting_user_notes (meeting_id, body_blocks, body_markdown, updated_at)
               VALUES (?, ?, ?, ?)
               ON CONFLICT(meeting_id) DO UPDATE SET
                 body_blocks = excluded.body_blocks,
                 body_markdown = excluded.body_markdown,
                 updated_at = excluded.updated_at"#,
        )
        .bind(&notes.meeting_id)
        .bind(&notes.body_blocks)
        .bind(&notes.body_markdown)
        .bind(&notes.updated_at)
        .execute(pool)
        .await?;
        Ok(Some(notes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::repositories::meeting::MeetingsRepository;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    async fn add_meeting(pool: &SqlitePool, id: &str) {
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES (?, 'M', 'x', 'x')")
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn notes_are_saved_and_read_back() {
        let pool = test_pool().await;
        add_meeting(&pool, "meeting-1").await;
        let saved = UserNotesRepository::save(&pool, "meeting-1", "[{\"type\":\"paragraph\"}]", "Budget is fixed")
            .await
            .unwrap()
            .expect("stored");
        assert_eq!(saved.body_markdown, "Budget is fixed");
        assert!(!saved.updated_at.is_empty());
        let read = UserNotesRepository::get(&pool, "meeting-1").await.unwrap().expect("found");
        assert_eq!(read, saved);
    }

    #[tokio::test]
    async fn saving_again_replaces_the_notes() {
        let pool = test_pool().await;
        add_meeting(&pool, "meeting-1").await;
        UserNotesRepository::save(&pool, "meeting-1", "[]", "first").await.unwrap();
        UserNotesRepository::save(&pool, "meeting-1", "[]", "second").await.unwrap();
        let read = UserNotesRepository::get(&pool, "meeting-1").await.unwrap().unwrap();
        assert_eq!(read.body_markdown, "second");
    }

    #[tokio::test]
    async fn a_meeting_without_notes_has_none() {
        let pool = test_pool().await;
        add_meeting(&pool, "meeting-1").await;
        assert_eq!(UserNotesRepository::get(&pool, "meeting-1").await.unwrap(), None);
    }

    #[tokio::test]
    async fn empty_notes_remove_the_row() {
        let pool = test_pool().await;
        add_meeting(&pool, "meeting-1").await;
        UserNotesRepository::save(&pool, "meeting-1", "[]", "something").await.unwrap();
        let after = UserNotesRepository::save(&pool, "meeting-1", "[]", "  \n ").await.unwrap();
        assert_eq!(after, None);
        assert_eq!(UserNotesRepository::get(&pool, "meeting-1").await.unwrap(), None);
    }

    #[tokio::test]
    async fn notes_for_an_unknown_meeting_are_refused() {
        let pool = test_pool().await;
        let err = UserNotesRepository::save(&pool, "meeting-missing", "[]", "text").await.unwrap_err();
        assert!(matches!(err, UserNotesError::Invalid(_)), "{err}");
    }

    #[tokio::test]
    async fn oversized_notes_are_refused() {
        let pool = test_pool().await;
        add_meeting(&pool, "meeting-1").await;
        let big = "x".repeat(MAX_NOTES_BYTES + 1);
        let err = UserNotesRepository::save(&pool, "meeting-1", "[]", &big).await.unwrap_err();
        assert!(matches!(err, UserNotesError::Invalid(_)), "{err}");
    }

    #[tokio::test]
    async fn deleting_the_meeting_deletes_its_notes() {
        let pool = test_pool().await;
        add_meeting(&pool, "meeting-1").await;
        UserNotesRepository::save(&pool, "meeting-1", "[]", "keep me?").await.unwrap();
        assert!(MeetingsRepository::delete_meeting(&pool, "meeting-1").await.unwrap());
        let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM meeting_user_notes").fetch_one(&pool).await.unwrap();
        assert_eq!(left, 0);
    }
}
