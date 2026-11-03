use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{migrate::MigrateDatabase, Result, Sqlite, SqlitePool, Transaction};
use std::str::FromStr;
use std::fs;
use std::path::Path;
use tauri::Manager;

#[derive(Clone)]
pub struct DatabaseManager {
    pool: SqlitePool,
}

impl DatabaseManager {
    pub async fn new(tauri_db_path: &str, backend_db_path: &str) -> Result<Self> {
        if let Some(parent_dir) = Path::new(tauri_db_path).parent() {
            if !parent_dir.exists() {
                fs::create_dir_all(parent_dir).map_err(|e| sqlx::Error::Io(e))?;
            }
        }

        if !Path::new(tauri_db_path).exists() {
            if Path::new(backend_db_path).exists() {
                // log-ok: an app data, model or tool path; no meeting data
                log::info!(
                    "Copying database from {} to {}",
                    backend_db_path,
                    tauri_db_path
                );
                fs::copy(backend_db_path, tauri_db_path).map_err(|e| sqlx::Error::Io(e))?;
            } else {
                // log-ok: an app data, model or tool path; no meeting data
                log::info!("Creating database at {}", tauri_db_path);
                Sqlite::create_database(tauri_db_path).await?;
            }
        }

        // secure_delete makes SQLite overwrite deleted content instead of leaving the old text in
        // unused pages of the file, so a deleted meeting's transcript is not recoverable from the database
        let options = SqliteConnectOptions::from_str(tauri_db_path)?.pragma("secure_delete", "ON");
        let pool = SqlitePoolOptions::new().connect_with(options).await?;

        let migrator = sqlx::migrate!("./migrations");
        let newest_version = migrator.migrations.last().map(|m| m.version).unwrap_or(0);
        // Migrations only go forward, so take a dated backup first. If the data cannot be
        // protected, do not migrate: the error says why.
        super::backup::backup_before_migration(
            &pool,
            Path::new(tauri_db_path),
            newest_version,
            chrono::Utc::now(),
        )
        .await?;
        migrator.run(&pool).await?;

        Ok(DatabaseManager { pool })
    }

    // NOTE: So for the first time users they needs to start the application
    // after they can just delete the existing .sqlite file and then copy the existing .db file to
    // the current app dir, So the system detects legacy db and copy it and starts with that data
    // (Newly created .sqlite with the copied content from .db)
    pub async fn new_from_app_handle(app_handle: &tauri::AppHandle) -> Result<Self> {
        // Resolve the app's data directory
        let app_data_dir = app_handle
            .path()
            .app_data_dir()
            .expect("failed to get app data dir");
        if !app_data_dir.exists() {
            fs::create_dir_all(&app_data_dir).map_err(|e| sqlx::Error::Io(e))?;
        }

        // Define database paths
        let tauri_db_path = app_data_dir
            .join("meeting_minutes.sqlite")
            .to_string_lossy()
            .to_string();
        // Legacy backend DB path (for auto-migration if exists)
        let backend_db_path = app_data_dir
            .join("meeting_minutes.db")
            .to_string_lossy()
            .to_string();

        // WAL file paths for defensive cleanup
        let wal_path = app_data_dir.join("meeting_minutes.sqlite-wal");
        let shm_path = app_data_dir.join("meeting_minutes.sqlite-shm");

        // log-ok: an app data, model or tool path; no meeting data
        log::info!("Tauri DB path: {}", tauri_db_path);
        // log-ok: an app data, model or tool path; no meeting data
        log::info!("Legacy backend DB path: {}", backend_db_path);

        // Try to open database with defensive WAL handling
        match Self::new(&tauri_db_path, &backend_db_path).await {
            Ok(db_manager) => {
                log::info!("Database opened successfully");
                Ok(db_manager)
            }
            Err(e) => {
                // Check if error is due to corrupted WAL file
                let error_msg = e.to_string();
                if error_msg.contains("malformed") || error_msg.contains("corrupt") {
                    log::warn!("Database appears corrupted, likely due to orphaned WAL file. Attempting recovery...");
                    log::warn!("Error details: {}", error_msg);

                    // Delete potentially corrupted WAL/SHM files
                    if wal_path.exists() {
                        match fs::remove_file(&wal_path) {
                            // log-ok: an app data, model or tool path; no meeting data
                            Ok(_) => log::info!("Removed orphaned WAL file: {:?}", wal_path),
                            Err(e) => log::warn!("Failed to remove WAL file: {}", e),
                        }
                    }
                    if shm_path.exists() {
                        match fs::remove_file(&shm_path) {
                            // log-ok: an app data, model or tool path; no meeting data
                            Ok(_) => log::info!("Removed orphaned SHM file: {:?}", shm_path),
                            Err(e) => log::warn!("Failed to remove SHM file: {}", e),
                        }
                    }

                    // Retry connection without WAL files
                    log::info!("Retrying database connection after WAL cleanup...");
                    match Self::new(&tauri_db_path, &backend_db_path).await {
                        Ok(db_manager) => {
                            log::info!("Database opened successfully after WAL recovery");
                            Ok(db_manager)
                        }
                        Err(retry_err) => {
                            log::error!("Database connection failed even after WAL cleanup: {}", retry_err);
                            Err(retry_err)
                        }
                    }
                } else {
                    // Not a WAL-related error, propagate original error
                    log::error!("Database connection failed: {}", error_msg);
                    Err(e)
                }
            }
        }
    }

    /// Check if this is the first launch (sqlite database doesn't exist yet)
    pub async fn is_first_launch(app_handle: &tauri::AppHandle) -> Result<bool> {
        let app_data_dir = app_handle
            .path()
            .app_data_dir()
            .expect("failed to get app data dir");

        let tauri_db_path = app_data_dir.join("meeting_minutes.sqlite");

        Ok(!tauri_db_path.exists())
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn with_transaction<T, F, Fut>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut Transaction<'_, Sqlite>) -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let mut tx = self.pool.begin().await?;
        let result = f(&mut tx).await;

        match result {
            Ok(val) => {
                tx.commit().await?;
                Ok(val)
            }
            Err(err) => {
                tx.rollback().await?;
                Err(err)
            }
        }
    }

    /// Cleanup database connection and checkpoint WAL
    /// This should be called on application shutdown to ensure:
    /// - All WAL changes are written to the main database file
    /// - The .wal and .shm files are deleted
    /// - Connection pool is gracefully closed
    pub async fn cleanup(&self) -> Result<()> {
        log::info!("Starting database cleanup...");

        // Force checkpoint of WAL to main database file and remove WAL file
        // TRUNCATE mode: checkpoints all pages AND deletes the WAL file
        match sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&self.pool)
            .await
        {
            Ok(_) => log::info!("WAL checkpoint completed successfully"),
            Err(e) => log::warn!("WAL checkpoint failed (non-fatal): {}", e),
        }

        // Close the connection pool gracefully
        self.pool.close().await;
        log::info!("Database connection pool closed");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqliteConnectOptions;
    use sqlx::Row;
    use std::borrow::Cow;

    /// A database as an earlier build left it: every migration except the newest is applied.
    async fn database_one_migration_behind(path: &Path) {
        let options = SqliteConnectOptions::new().filename(path).create_if_missing(true);
        let pool = SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap();
        let mut migrator = sqlx::migrate!("./migrations");
        let newest = migrator.migrations.last().unwrap().version;
        migrator.migrations =
            Cow::Owned(migrator.migrations.iter().filter(|m| m.version < newest).cloned().collect());
        migrator.run(&pool).await.unwrap();
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES ('keep-me', 'T', 'x', 'x')")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
    }

    #[tokio::test]
    async fn opening_an_older_database_backs_it_up_and_then_migrates_it() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("meeting_minutes.sqlite");
        database_one_migration_behind(&db).await;

        let manager = DatabaseManager::new(db.to_str().unwrap(), "does-not-exist.db").await.unwrap();

        // Migrated: the newest migration is now recorded and the data is still there
        let newest = sqlx::migrate!("./migrations").migrations.last().unwrap().version;
        let applied: i64 = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
            .fetch_one(manager.pool())
            .await
            .unwrap();
        assert_eq!(applied, newest);
        let n: i64 = sqlx::query("SELECT COUNT(*) AS n FROM meetings").fetch_one(manager.pool()).await.unwrap().get("n");
        assert_eq!(n, 1);

        // And a backup of the pre-migration state exists
        let backups: Vec<_> = std::fs::read_dir(super::super::backup::backups_dir(&db))
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.ends_with(".bak"))
            .collect();
        assert_eq!(backups.len(), 1, "{backups:?}");
    }

    #[tokio::test]
    async fn opening_a_current_database_again_makes_no_new_backup() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("meeting_minutes.sqlite");
        database_one_migration_behind(&db).await;
        let first = DatabaseManager::new(db.to_str().unwrap(), "none.db").await.unwrap();
        first.pool().close().await;
        let second = DatabaseManager::new(db.to_str().unwrap(), "none.db").await.unwrap();
        second.pool().close().await;

        let count = std::fs::read_dir(super::super::backup::backups_dir(&db))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".bak"))
            .count();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn the_unused_licensing_and_meeting_notes_tables_are_dropped_on_upgrade() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("meeting_minutes.sqlite");
        database_one_migration_behind(&db).await;

        let manager = DatabaseManager::new(db.to_str().unwrap(), "none.db").await.unwrap();
        let tables: Vec<String> = sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table'")
            .fetch_all(manager.pool())
            .await
            .unwrap();
        assert!(!tables.iter().any(|t| t == "licensing" || t == "meeting_notes"), "{tables:?}");
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM meetings").fetch_one(manager.pool()).await.unwrap();
        assert_eq!(n, 1, "meetings are kept");
    }

    #[tokio::test]
    async fn a_brand_new_database_is_created_without_any_backup() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("meeting_minutes.sqlite");
        let manager = DatabaseManager::new(db.to_str().unwrap(), "none.db").await.unwrap();
        manager.pool().close().await;
        assert!(!super::super::backup::backups_dir(&db).exists());
    }
}
