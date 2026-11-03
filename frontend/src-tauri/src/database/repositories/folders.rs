use crate::folders::models::{Folder, FolderSummary, MeetingListItem};
use chrono::Utc;
use sqlx::{Error as SqlxError, SqliteConnection, SqlitePool};
use uuid::Uuid;

const MAX_NAME_LEN: usize = 80;

#[derive(Debug, thiserror::Error)]
pub enum FolderError {
    /// A problem with what the caller asked for; the message is safe to show in the UI.
    #[error("{0}")]
    Invalid(String),
    #[error("Database error: {0}")]
    Db(#[from] SqlxError),
}

fn invalid<T>(msg: impl Into<String>) -> Result<T, FolderError> {
    Err(FolderError::Invalid(msg.into()))
}

/// Collapse runs of whitespace and reject empty or oversized names.
fn normalize_name(name: &str) -> Result<String, FolderError> {
    let cleaned = super::people::clean_single_line(name);
    if cleaned.is_empty() {
        return invalid("Folder name cannot be empty");
    }
    if cleaned.chars().count() > MAX_NAME_LEN {
        return invalid(format!("Folder name is too long (max {} characters)", MAX_NAME_LEN));
    }
    Ok(cleaned)
}

pub struct FoldersRepository;

impl FoldersRepository {
    /// All folders (alphabetical) with their meeting counts.
    pub async fn list_folders(pool: &SqlitePool) -> Result<Vec<FolderSummary>, FolderError> {
        let folders = sqlx::query_as::<_, FolderSummary>(
            r#"SELECT f.id, f.name, COUNT(m.id) AS meeting_count
               FROM folders f
               LEFT JOIN meetings m ON m.folder_id = f.id
               GROUP BY f.id, f.name
               ORDER BY f.name COLLATE NOCASE"#,
        )
        .fetch_all(pool)
        .await?;
        Ok(folders)
    }

    async fn find_by_name(conn: &mut SqliteConnection, name: &str) -> Result<Option<Folder>, SqlxError> {
        sqlx::query_as::<_, Folder>("SELECT * FROM folders WHERE name = ? COLLATE NOCASE")
            .bind(name)
            .fetch_optional(conn)
            .await
    }

    async fn find_by_id(conn: &mut SqliteConnection, id: &str) -> Result<Option<Folder>, SqlxError> {
        sqlx::query_as::<_, Folder>("SELECT * FROM folders WHERE id = ?")
            .bind(id)
            .fetch_optional(conn)
            .await
    }

    pub async fn create_folder(pool: &SqlitePool, name: &str) -> Result<Folder, FolderError> {
        let name = normalize_name(name)?;
        let mut conn = pool.acquire().await?;

        if let Some(existing) = Self::find_by_name(&mut conn, &name).await? {
            return invalid(format!("A folder named \"{}\" already exists", existing.name));
        }

        let now = Utc::now().to_rfc3339();
        let folder = Folder {
            id: format!("folder-{}", Uuid::new_v4()),
            name,
            created_at: now.clone(),
            updated_at: now,
        };
        sqlx::query("INSERT INTO folders (id, name, created_at, updated_at) VALUES (?, ?, ?, ?)")
            .bind(&folder.id)
            .bind(&folder.name)
            .bind(&folder.created_at)
            .bind(&folder.updated_at)
            .execute(&mut *conn)
            .await?;
        Ok(folder)
    }

    pub async fn rename_folder(
        pool: &SqlitePool,
        folder_id: &str,
        new_name: &str,
    ) -> Result<Folder, FolderError> {
        let name = normalize_name(new_name)?;
        let mut conn = pool.acquire().await?;

        if let Some(clash) = Self::find_by_name(&mut conn, &name).await? {
            if clash.id != folder_id {
                return invalid(format!("A folder named \"{}\" already exists", clash.name));
            }
        }

        let updated = sqlx::query("UPDATE folders SET name = ?, updated_at = ? WHERE id = ?")
            .bind(&name)
            .bind(Utc::now().to_rfc3339())
            .bind(folder_id)
            .execute(&mut *conn)
            .await?;
        if updated.rows_affected() == 0 {
            return invalid("Folder not found");
        }

        match Self::find_by_id(&mut conn, folder_id).await? {
            Some(folder) => Ok(folder),
            None => invalid("Folder not found"),
        }
    }

    /// Delete a folder. Its meetings are kept and become unfiled.
    pub async fn delete_folder(pool: &SqlitePool, folder_id: &str) -> Result<bool, FolderError> {
        let mut tx = pool.begin().await?;
        sqlx::query("UPDATE meetings SET folder_id = NULL WHERE folder_id = ?")
            .bind(folder_id)
            .execute(&mut *tx)
            .await?;
        let deleted = sqlx::query("DELETE FROM folders WHERE id = ?")
            .bind(folder_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(deleted.rows_affected() > 0)
    }

    /// Put a meeting in a folder, or pass `None` to make it unfiled.
    pub async fn move_meeting(
        pool: &SqlitePool,
        meeting_id: &str,
        folder_id: Option<&str>,
    ) -> Result<(), FolderError> {
        let mut tx = pool.begin().await?;

        if let Some(fid) = folder_id {
            if Self::find_by_id(&mut tx, fid).await?.is_none() {
                return invalid("Folder not found");
            }
        }

        // Deliberately leaves meetings.updated_at alone: filing a meeting is not editing it.
        let moved = sqlx::query("UPDATE meetings SET folder_id = ? WHERE id = ?")
            .bind(folder_id)
            .bind(meeting_id)
            .execute(&mut *tx)
            .await?;
        if moved.rows_affected() == 0 {
            return invalid("Meeting not found");
        }

        tx.commit().await?;
        Ok(())
    }

    /// Every meeting, newest first, with the folder it is filed in.
    pub async fn list_meetings(pool: &SqlitePool) -> Result<Vec<MeetingListItem>, FolderError> {
        let meetings = sqlx::query_as::<_, MeetingListItem>(
            "SELECT id, title, folder_id, delete_requested_at IS NOT NULL AS deleting FROM meetings ORDER BY created_at DESC",
        )
        .fetch_all(pool)
        .await?;
        Ok(meetings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    /// In-memory database with the real migration chain applied. A single connection
    /// keeps every query on the same in-memory database.
    async fn test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    async fn add_meeting(pool: &SqlitePool, id: &str, created: &str) {
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES (?, ?, ?, ?)")
            .bind(id)
            .bind(format!("Meeting {}", id))
            .bind(created)
            .bind(created)
            .execute(pool)
            .await
            .unwrap();
    }

    #[test]
    fn names_are_cleaned_and_validated() {
        assert_eq!(normalize_name("  Weekly   Syncs ").unwrap(), "Weekly Syncs");
        assert!(normalize_name("  ").is_err());
        assert!(normalize_name(&"x".repeat(MAX_NAME_LEN + 1)).is_err());
    }

    #[tokio::test]
    async fn folder_names_are_unique_ignoring_case() {
        let pool = test_pool().await;
        FoldersRepository::create_folder(&pool, "Weekly Syncs").await.unwrap();
        let err = FoldersRepository::create_folder(&pool, "  weekly   syncs ").await.unwrap_err();
        assert!(err.to_string().contains("already exists"), "{err}");
        assert_eq!(FoldersRepository::list_folders(&pool).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn moving_files_a_meeting_and_counts_follow() {
        let pool = test_pool().await;
        add_meeting(&pool, "m1", "2026-10-01T10:00:00Z").await;
        add_meeting(&pool, "m2", "2026-10-02T10:00:00Z").await;
        let folder = FoldersRepository::create_folder(&pool, "Project X").await.unwrap();

        FoldersRepository::move_meeting(&pool, "m1", Some(&folder.id)).await.unwrap();
        FoldersRepository::move_meeting(&pool, "m2", Some(&folder.id)).await.unwrap();
        assert_eq!(FoldersRepository::list_folders(&pool).await.unwrap()[0].meeting_count, 2);

        // Back to unfiled
        FoldersRepository::move_meeting(&pool, "m2", None).await.unwrap();
        assert_eq!(FoldersRepository::list_folders(&pool).await.unwrap()[0].meeting_count, 1);

        let list = FoldersRepository::list_meetings(&pool).await.unwrap();
        assert_eq!(list.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["m2", "m1"], "newest first");
        assert_eq!(list[0].folder_id, None);
        assert_eq!(list[1].folder_id.as_deref(), Some(folder.id.as_str()));
    }

    #[tokio::test]
    async fn moving_to_a_missing_folder_or_meeting_is_rejected() {
        let pool = test_pool().await;
        add_meeting(&pool, "m1", "2026-10-01T10:00:00Z").await;
        assert!(FoldersRepository::move_meeting(&pool, "m1", Some("folder-nope")).await.is_err());
        let folder = FoldersRepository::create_folder(&pool, "A").await.unwrap();
        assert!(FoldersRepository::move_meeting(&pool, "nope", Some(&folder.id)).await.is_err());
        assert_eq!(FoldersRepository::list_meetings(&pool).await.unwrap()[0].folder_id, None);
    }

    #[tokio::test]
    async fn filing_a_meeting_does_not_touch_its_updated_at() {
        let pool = test_pool().await;
        add_meeting(&pool, "m1", "2026-10-01T10:00:00Z").await;
        let folder = FoldersRepository::create_folder(&pool, "A").await.unwrap();
        FoldersRepository::move_meeting(&pool, "m1", Some(&folder.id)).await.unwrap();
        let (updated,): (String,) = sqlx::query_as("SELECT updated_at FROM meetings WHERE id = 'm1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(updated, "2026-10-01T10:00:00Z");
    }

    #[tokio::test]
    async fn deleting_a_folder_keeps_its_meetings_unfiled() {
        let pool = test_pool().await;
        add_meeting(&pool, "m1", "2026-10-01T10:00:00Z").await;
        let folder = FoldersRepository::create_folder(&pool, "Old stuff").await.unwrap();
        FoldersRepository::move_meeting(&pool, "m1", Some(&folder.id)).await.unwrap();

        assert!(FoldersRepository::delete_folder(&pool, &folder.id).await.unwrap());
        assert!(!FoldersRepository::delete_folder(&pool, &folder.id).await.unwrap(), "already gone");

        let list = FoldersRepository::list_meetings(&pool).await.unwrap();
        assert_eq!(list.len(), 1, "the meeting survives");
        assert_eq!(list[0].folder_id, None);
        assert!(FoldersRepository::list_folders(&pool).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn rename_rejects_another_folders_name_but_allows_changing_case() {
        let pool = test_pool().await;
        let a = FoldersRepository::create_folder(&pool, "Alpha").await.unwrap();
        FoldersRepository::create_folder(&pool, "Beta").await.unwrap();
        assert!(FoldersRepository::rename_folder(&pool, &a.id, "beta").await.is_err());
        assert_eq!(FoldersRepository::rename_folder(&pool, &a.id, "ALPHA").await.unwrap().name, "ALPHA");
        assert!(FoldersRepository::rename_folder(&pool, "folder-nope", "Gamma").await.is_err());
    }

    #[tokio::test]
    async fn deleting_a_meeting_updates_folder_counts() {
        let pool = test_pool().await;
        add_meeting(&pool, "m1", "2026-10-01T10:00:00Z").await;
        let folder = FoldersRepository::create_folder(&pool, "A").await.unwrap();
        FoldersRepository::move_meeting(&pool, "m1", Some(&folder.id)).await.unwrap();
        sqlx::query("DELETE FROM meetings WHERE id = 'm1'").execute(&pool).await.unwrap();
        assert_eq!(FoldersRepository::list_folders(&pool).await.unwrap()[0].meeting_count, 0);
    }

    #[tokio::test]
    async fn folder_names_cannot_hide_behind_invisible_characters_or_carry_sql() {
        let pool = test_pool().await;
        let real = FoldersRepository::create_folder(&pool, "Project X").await.unwrap();
        for sneaky in ["Project\u{200B} X", "\u{202E}Project X", "Project X\u{0000}"] {
            assert!(FoldersRepository::create_folder(&pool, sneaky).await.is_err(), "{sneaky:?} duplicated an existing folder");
        }
        assert!(FoldersRepository::create_folder(&pool, "\u{200B}\u{202E}").await.is_err());
        let nasty = FoldersRepository::create_folder(&pool, "x'); DROP TABLE folders; --").await.unwrap();
        assert_eq!(nasty.name, "x'); DROP TABLE folders; --");
        assert_eq!(FoldersRepository::list_folders(&pool).await.unwrap().len(), 2);
        assert_eq!(FoldersRepository::rename_folder(&pool, &real.id, "Proj\u{202E}ect Y").await.unwrap().name, "Project Y");
    }
}
