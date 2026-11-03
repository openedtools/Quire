//! Dated backups of the database, taken just before a migration changes it.
//!
//! Migrations only go forward, and a build that does not know a newer migration refuses to open
//! the database. A backup is therefore the only way back from a bad update. The copy is made with
//! `VACUUM INTO`, which gives a consistent snapshot even when rows are still in the write-ahead
//! log, and is written to a temporary name first so a half-written file is never mistaken for a
//! backup.

use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// How many backups are kept; older ones are deleted after a new one is made.
pub const KEEP_BACKUPS: usize = 3;

const PREFIX: &str = "meeting_minutes.pre-";
const SUFFIX: &str = ".bak";

/// Folder next to the database that holds the backups.
pub fn backups_dir(db_path: &Path) -> PathBuf {
    db_path.parent().unwrap_or_else(|| Path::new(".")).join("backups")
}

/// `meeting_minutes.pre-<version the update moves to>.<UTC time>.bak`
pub fn backup_file_name(target_version: i64, now: DateTime<Utc>) -> String {
    format!("{PREFIX}{target_version}.{}{SUFFIX}", now.format("%Y%m%dT%H%M%SZ"))
}

/// Not worded with "corrupt" or "malformed": those words start the database recovery path.
fn backup_error(what: impl std::fmt::Display) -> sqlx::Error {
    sqlx::Error::Io(io::Error::new(
        io::ErrorKind::Other,
        format!("Could not back up the database before updating it: {what}"),
    ))
}

/// Highest migration version recorded as applied, or `None` when there is no record yet.
pub(crate) async fn applied_version(pool: &SqlitePool) -> sqlx::Result<Option<i64>> {
    let has_table: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations'",
    )
    .fetch_one(pool)
    .await?;
    if has_table == 0 {
        return Ok(None);
    }
    sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(version) FROM _sqlx_migrations").fetch_one(pool).await
}

/// Does the file hold any tables of its own (for example a database copied from the old backend)?
async fn has_user_tables(pool: &SqlitePool) -> sqlx::Result<bool> {
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name != '_sqlx_migrations'",
    )
    .fetch_one(pool)
    .await?;
    Ok(n > 0)
}

fn already_backed_up(dir: &Path, target_version: i64) -> bool {
    let start = format!("{PREFIX}{target_version}.");
    fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .any(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    name.starts_with(&start) && name.ends_with(SUFFIX)
                })
        })
        .unwrap_or(false)
}

fn remove_partial_files(dir: &Path) {
    if let Ok(rd) = fs::read_dir(dir) {
        for entry in rd.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(PREFIX) && name.ends_with(".part") {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}

/// Make a backup if the database holds data and is about to be migrated.
///
/// Returns the backup path, or `None` when no backup was needed or one already exists for this
/// target version (the open code can run twice through its retry path). An error means the data
/// could not be protected, so the caller must not migrate.
pub async fn backup_before_migration(
    pool: &SqlitePool,
    db_path: &Path,
    newest_version: i64,
    now: DateTime<Utc>,
) -> sqlx::Result<Option<PathBuf>> {
    let needs_backup = match applied_version(pool).await? {
        Some(applied) => applied < newest_version,
        None => has_user_tables(pool).await?,
    };
    if !needs_backup {
        return Ok(None);
    }

    let dir = backups_dir(db_path);
    fs::create_dir_all(&dir).map_err(backup_error)?;
    remove_partial_files(&dir);
    if already_backed_up(&dir, newest_version) {
        return Ok(None);
    }

    let name = backup_file_name(newest_version, now);
    let final_path = dir.join(&name);
    let part_path = dir.join(format!("{name}.part"));
    if let Err(e) = sqlx::query("VACUUM INTO ?1").bind(part_path.to_string_lossy().to_string()).execute(pool).await {
        let _ = fs::remove_file(&part_path);
        return Err(backup_error(e));
    }
    if let Err(e) = crate::fs_retry::rename(&part_path, &final_path) {
        let _ = fs::remove_file(&part_path);
        return Err(backup_error(e));
    }
    // log-ok: an app data, model or tool path; no meeting data
    log::info!("Backed up the database to {} before updating it", final_path.display());

    if let Err(e) = rotate(&dir, KEEP_BACKUPS) {
        log::warn!("Could not remove old database backups: {e}");
    }
    Ok(Some(final_path))
}

/// Delete all but the newest `keep` backups. Newest means latest time in the file name.
fn rotate(dir: &Path, keep: usize) -> io::Result<()> {
    let mut found: Vec<(String, PathBuf)> = Vec::new();
    for entry in fs::read_dir(dir)?.filter_map(|e| e.ok()) {
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(middle) = name.strip_prefix(PREFIX).and_then(|rest| rest.strip_suffix(SUFFIX)) {
            // "<target version>.<time>": order by the time
            let time = middle.split_once('.').map(|(_, t)| t.to_string()).unwrap_or_default();
            found.push((format!("{time}|{name}"), entry.path()));
        }
    }
    found.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, path) in found.into_iter().skip(keep) {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use sqlx::Row;

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 2, h, m, 0).unwrap()
    }

    async fn open(path: &Path) -> SqlitePool {
        let options = SqliteConnectOptions::new().filename(path).create_if_missing(true);
        SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap()
    }

    async fn migrated(path: &Path) -> SqlitePool {
        let pool = open(path).await;
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    fn newest() -> i64 {
        sqlx::migrate!("./migrations").migrations.last().unwrap().version
    }

    fn baks(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).collect())
            .unwrap_or_default();
        names.retain(|n| n.ends_with(".bak"));
        names.sort();
        names
    }

    async fn count_meetings(backup: &Path) -> i64 {
        let options = SqliteConnectOptions::new().filename(backup).read_only(true);
        let pool = SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap();
        let row = sqlx::query("SELECT COUNT(*) AS n FROM meetings").fetch_one(&pool).await.unwrap();
        row.get::<i64, _>("n")
    }

    async fn add_meeting(pool: &SqlitePool, id: &str) {
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES (?, 'T', 'x', 'x')")
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
    }

    /// Make the database look one migration behind, as it would before an update.
    async fn make_behind(pool: &SqlitePool) {
        sqlx::query("DELETE FROM _sqlx_migrations WHERE version = (SELECT MAX(version) FROM _sqlx_migrations)")
            .execute(pool)
            .await
            .unwrap();
    }

    #[test]
    fn file_names_carry_the_target_and_sort_by_time() {
        let a = backup_file_name(20261102000000, at(9, 0));
        let b = backup_file_name(20261102000000, at(10, 30));
        assert_eq!(a, "meeting_minutes.pre-20261102000000.20261102T090000Z.bak");
        assert!(a < b);
    }

    #[tokio::test]
    async fn an_empty_new_database_is_not_backed_up() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("meeting_minutes.sqlite");
        let pool = open(&db).await;
        assert_eq!(backup_before_migration(&pool, &db, newest(), at(9, 0)).await.unwrap(), None);
        assert!(!backups_dir(&db).exists(), "no folder is made when there is nothing to protect");
    }

    #[tokio::test]
    async fn an_up_to_date_database_is_not_backed_up() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("meeting_minutes.sqlite");
        let pool = migrated(&db).await;
        add_meeting(&pool, "m1").await;
        assert_eq!(backup_before_migration(&pool, &db, newest(), at(9, 0)).await.unwrap(), None);
    }

    #[tokio::test]
    async fn a_database_behind_the_newest_migration_is_backed_up_with_its_data() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("meeting_minutes.sqlite");
        let pool = migrated(&db).await;
        add_meeting(&pool, "m1").await;
        add_meeting(&pool, "m2").await;
        make_behind(&pool).await;

        let made = backup_before_migration(&pool, &db, newest(), at(9, 0)).await.unwrap().expect("a backup");
        assert!(made.starts_with(backups_dir(&db)));
        assert_eq!(made.file_name().unwrap().to_string_lossy(), backup_file_name(newest(), at(9, 0)));
        assert_eq!(count_meetings(&made).await, 2);
        assert_eq!(baks(&backups_dir(&db)).len(), 1);
    }

    #[tokio::test]
    async fn rows_still_in_the_write_ahead_log_are_in_the_backup() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("meeting_minutes.sqlite");
        let pool = migrated(&db).await;
        sqlx::query("PRAGMA wal_autocheckpoint = 0").execute(&pool).await.unwrap();
        sqlx::query("PRAGMA journal_mode = WAL").execute(&pool).await.unwrap();
        add_meeting(&pool, "fresh").await; // sits in the -wal file, not yet in the main file
        make_behind(&pool).await;

        let made = backup_before_migration(&pool, &db, newest(), at(9, 0)).await.unwrap().expect("a backup");
        assert_eq!(count_meetings(&made).await, 1);
    }

    #[tokio::test]
    async fn a_legacy_database_with_tables_but_no_migration_record_is_backed_up() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("meeting_minutes.sqlite");
        let pool = open(&db).await;
        sqlx::query("CREATE TABLE meetings (id TEXT PRIMARY KEY, title TEXT, created_at TEXT, updated_at TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        add_meeting(&pool, "old").await;
        let made = backup_before_migration(&pool, &db, newest(), at(9, 0)).await.unwrap().expect("a backup");
        assert_eq!(count_meetings(&made).await, 1);
    }

    #[tokio::test]
    async fn repeating_for_the_same_target_does_not_make_another_backup() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("meeting_minutes.sqlite");
        let pool = migrated(&db).await;
        make_behind(&pool).await;
        let first = backup_before_migration(&pool, &db, newest(), at(9, 0)).await.unwrap();
        assert!(first.is_some());
        let second = backup_before_migration(&pool, &db, newest(), at(9, 5)).await.unwrap();
        assert_eq!(second, None);
        assert_eq!(baks(&backups_dir(&db)).len(), 1);
    }

    #[tokio::test]
    async fn only_the_three_newest_backups_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("meeting_minutes.sqlite");
        let pool = migrated(&db).await;
        let base = newest();
        // Five updates in a row, each to a higher target version
        for i in 0..5i64 {
            backup_before_migration(&pool, &db, base + 1 + i, at(9 + i as u32, 0)).await.unwrap().expect("a backup");
        }
        let kept = baks(&backups_dir(&db));
        assert_eq!(kept.len(), KEEP_BACKUPS);
        // The oldest two (targets +1 and +2) are gone, the newest three remain
        for gone in [base + 1, base + 2] {
            assert!(!kept.iter().any(|n| n.contains(&format!("pre-{gone}."))), "{gone} should have been rotated out");
        }
        for stays in [base + 3, base + 4, base + 5] {
            assert!(kept.iter().any(|n| n.contains(&format!("pre-{stays}."))), "{stays} should remain");
        }
    }

    #[tokio::test]
    async fn a_backup_that_cannot_be_written_is_an_error_and_leaves_no_partial_file() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("meeting_minutes.sqlite");
        let pool = migrated(&db).await;
        add_meeting(&pool, "m1").await;
        make_behind(&pool).await;
        // A plain file where the backups folder should be makes the folder impossible to create
        fs::write(backups_dir(&db), b"in the way").unwrap();

        let err = backup_before_migration(&pool, &db, newest(), at(9, 0)).await.unwrap_err();
        assert!(err.to_string().contains("back up"), "message should say what failed: {err}");
        assert!(!err.to_string().to_lowercase().contains("corrupt"), "must not trigger the corruption recovery path");
    }

    #[tokio::test]
    async fn leftover_partial_files_are_cleaned_and_never_counted() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("meeting_minutes.sqlite");
        let pool = migrated(&db).await;
        make_behind(&pool).await;
        let folder = backups_dir(&db);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join(format!("{}.part", backup_file_name(newest(), at(8, 0)))), b"half").unwrap();

        let made = backup_before_migration(&pool, &db, newest(), at(9, 0)).await.unwrap();
        assert!(made.is_some(), "a .part file is not a backup");
        let names: Vec<String> = fs::read_dir(&folder).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
        assert!(!names.iter().any(|n| n.ends_with(".part")), "stale partial files are removed: {names:?}");
    }
}
