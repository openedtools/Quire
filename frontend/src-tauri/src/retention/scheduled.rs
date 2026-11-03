//! The daily audio check while the app is closed (spec F-RET-11, plan PK3). On Windows a Task
//! Scheduler entry runs the app with `--retention-sweep`: it opens no window, does one sweep with
//! the same rules and guards, and exits. It never runs alongside the app: the app holds a lock
//! file while it is open, and the check exits when it cannot take it.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use once_cell::sync::Lazy;
use sqlx::SqlitePool;

use super::sweep;

pub const FLAG: &str = "--retention-sweep";
const LOCK_FILE: &str = "app.lock";
/// One task per Windows account, so two accounts never replace each other's.
#[cfg_attr(not(windows), allow(dead_code))]
fn task_name() -> String {
    let user: String = std::env::var("USERNAME").unwrap_or_default().chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
    format!("Quire daily audio check ({})", if user.is_empty() { "this account" } else { &user })
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

/// The task definition: daily at noon, run as soon as possible after a missed start (a laptop that
/// was asleep or off), on battery too, at most 10 minutes, never twice at once (review finding).
pub fn task_xml(exe: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>Deletes recording audio whose retention period has ended, while Quire is closed.</Description></RegistrationInfo>
  <Triggers>
    <CalendarTrigger>
      <StartBoundary>2026-01-01T12:00:00</StartBoundary>
      <ScheduleByDay><DaysInterval>1</DaysInterval></ScheduleByDay>
    </CalendarTrigger>
  </Triggers>
  <Settings>
    <StartWhenAvailable>true</StartWhenAvailable>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <ExecutionTimeLimit>PT10M</ExecutionTimeLimit>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
  </Settings>
  <Actions>
    <Exec>
      <Command>{}</Command>
      <Arguments>{}</Arguments>
    </Exec>
  </Actions>
</Task>
"#,
        xml_escape(exe),
        FLAG
    )
}

#[derive(Debug, Clone, Copy, serde::Serialize, PartialEq)]
pub struct Scheduled {
    /// Whether this install should have the daily check
    pub wanted: bool,
    /// Whether Windows has it
    pub exists: bool,
}

static APP_LOCK: Lazy<Mutex<Option<File>>> = Lazy::new(|| Mutex::new(None));
static APP_LOCK_HELD: AtomicBool = AtomicBool::new(false);

/// The app's data folder, the same one Tauri uses, found without starting Tauri.
pub fn app_data_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join(crate::brand_generated::BUNDLE_ID))
}

fn try_lock(dir: &Path) -> Option<File> {
    std::fs::create_dir_all(dir).ok()?;
    let file = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(dir.join(LOCK_FILE)).ok()?;
    file.try_lock().ok().map(|_| file)
}

/// The app takes the lock as it starts; if a scheduled check is running, it waits for it (up
/// to two minutes) before its own sweeps begin.
pub fn hold_app_lock(dir: PathBuf) {
    std::thread::spawn(move || {
        for _ in 0..120 {
            if let Some(file) = try_lock(&dir) {
                *APP_LOCK.lock().unwrap_or_else(|e| e.into_inner()) = Some(file);
                APP_LOCK_HELD.store(true, Ordering::SeqCst);
                return;
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        // A stuck check never stops the app's own sweeps, but the app keeps trying for the lock,
        // so the next daily check sees it open and leaves (review finding)
        APP_LOCK_HELD.store(true, Ordering::SeqCst);
        loop {
            std::thread::sleep(std::time::Duration::from_secs(5));
            if let Some(file) = try_lock(&dir) {
                *APP_LOCK.lock().unwrap_or_else(|e| e.into_inner()) = Some(file);
                return;
            }
        }
    });
}

/// False only while the app waits for a scheduled check to finish.
pub fn app_may_sweep() -> bool {
    APP_LOCK_HELD.load(Ordering::SeqCst)
}

/// The folders deletion may act in, from the saved preferences on disk (as the app computes them).
pub fn roots_from_disk(app_data: &Path) -> Vec<PathBuf> {
    use crate::audio::recording_preferences::{earlier_default_folder, stored_on_disk, StoredOnDisk};
    let setting = match stored_on_disk(&app_data.join("recording_preferences.json")) {
        StoredOnDisk::Saved(p) if !p.save_folder.as_os_str().is_empty() => p.save_folder,
        _ => app_data.join("recordings"),
    };
    let adopted = app_data.join(".adopted").is_file();
    sweep::with_earlier_root(sweep::known_roots(&setting, Some(app_data)), adopted.then(earlier_default_folder))
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// The app is open: it sweeps itself
    AppRunning,
    /// No database yet, or one the app has not migrated to this version yet
    NotReady,
    Swept(sweep::SweepReport),
}

/// One check against the app data folder `app_data`.
pub async fn check(app_data: &Path) -> Result<Outcome, String> {
    let Some(_lock) = try_lock(app_data) else { return Ok(Outcome::AppRunning) };
    let db = app_data.join("meeting_minutes.sqlite");
    if !db.is_file() {
        return Ok(Outcome::NotReady);
    }
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(&db).pragma("secure_delete", "ON");
    let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect_with(options).await.map_err(|e| e.to_string())?;
    if !migrated(&pool).await {
        return Ok(Outcome::NotReady);
    }
    let roots = roots_from_disk(app_data);
    // The guards hold: many recordings at once or a clock set back wait for the app to ask
    let (report, _) = sweep::sweep_with(&pool, &roots, chrono::Utc::now(), sweep::Confirmation::Count(0)).await.map_err(|e| e.to_string())?;
    pool.close().await;
    Ok(Outcome::Swept(report))
}

/// Never migrates: the database must already be at this build's newest migration.
async fn migrated(pool: &SqlitePool) -> bool {
    let newest = sqlx::migrate!("./migrations").migrations.last().map(|m| m.version).unwrap_or(0);
    let applied: Option<(i64,)> = sqlx::query_as("SELECT MAX(version) FROM _sqlx_migrations WHERE success = 1").fetch_optional(pool).await.ok().flatten();
    applied.map(|(v,)| v) == Some(newest)
}

/// Called first thing in `main`: with the flag, does one check and returns true (the process then
/// exits without opening a window).
pub fn run_if_requested() -> bool {
    if !std::env::args().any(|a| a == FLAG) {
        return false;
    }
    if let (Some(dir), Ok(rt)) = (app_data_dir(), tokio::runtime::Builder::new_current_thread().enable_all().build()) {
        let _ = rt.block_on(check(&dir));
    }
    true
}

/// Whether the daily check should exist: the default keeps audio for a limited time, or some
/// recording still has a deadline ahead.
pub async fn wanted(pool: &SqlitePool) -> bool {
    let default_limited = sweep::settings(pool).await.map_or(false, |s| s.default_retention != super::policy::Retention::Keep);
    // Deadlines still ahead; one already past is the app's to handle (or kept by the safety rules)
    let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM meetings WHERE audio_state = 'present' AND audio_expires_at > ?")
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    default_limited || pending > 0
}

#[cfg(windows)]
fn schtasks(args: String) -> bool {
    use std::os::windows::process::CommandExt;
    let mut cmd = std::process::Command::new("schtasks");
    cmd.creation_flags(0x0800_0000); // no console window
    cmd.raw_arg(args);
    cmd.output().map_or(false, |o| o.status.success())
}

/// Whether the daily check exists, without changing anything (for Settings).
#[cfg(windows)]
pub async fn status(pool: &SqlitePool) -> Option<Scheduled> {
    let want = wanted(pool).await;
    let name = task_name();
    let exists = tokio::task::spawn_blocking(move || schtasks(format!("/Query /TN \"{name}\""))).await.unwrap_or(false);
    Some(Scheduled { wanted: want, exists })
}

/// Adds or removes the daily check to match `wanted`, only when it differs (at start-up and when
/// the retention setting changes). None where there is no scheduled check (the Mac, for now).
#[cfg(windows)]
pub async fn sync(pool: &SqlitePool) -> Option<Scheduled> {
    let want = wanted(pool).await;
    let exe = std::env::current_exe().ok()?.to_string_lossy().to_string();
    let name = task_name();
    let exists = tokio::task::spawn_blocking(move || {
        let exists = schtasks(format!("/Query /TN \"{name}\""));
        if want && !exists {
            // The definition goes through a UTF-16 XML file, the form Task Scheduler reads
            let path = std::env::temp_dir().join(format!("quire-task-{}.xml", std::process::id()));
            let mut bytes = vec![0xFF, 0xFE];
            bytes.extend(task_xml(&exe).encode_utf16().flat_map(|u| u.to_le_bytes()));
            let created = std::fs::write(&path, bytes).is_ok() && schtasks(format!("/Create /F /TN \"{name}\" /XML \"{}\"", path.display()));
            let _ = std::fs::remove_file(&path);
            created
        } else if !want && exists {
            !schtasks(format!("/Delete /F /TN \"{name}\""))
        } else {
            exists
        }
    })
    .await
    .ok()?;
    Some(Scheduled { wanted: want, exists })
}

#[cfg(not(windows))]
pub async fn status(_pool: &SqlitePool) -> Option<Scheduled> {
    None
}

#[cfg(not(windows))]
pub async fn sync(_pool: &SqlitePool) -> Option<Scheduled> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_check_sweeps_with_the_apps_rules_and_never_runs_beside_the_app() {
        let data = tempfile::tempdir().unwrap();
        let app_data = data.path();
        // A database at this build's migration, as the app leaves it
        let db = app_data.join("meeting_minutes.sqlite");
        let manager = crate::database::manager::DatabaseManager::new(db.to_str().unwrap(), "/nonexistent").await.unwrap();
        let pool = manager.pool().clone();
        let recordings = app_data.join("recordings");
        let folder = recordings.join("Rec_a");
        std::fs::create_dir_all(&folder).unwrap();
        for f in ["audio.mp4", "metadata.json", "transcripts.json"] {
            std::fs::write(folder.join(f), b"x").unwrap();
        }
        sqlx::query(
            "INSERT INTO meetings (id, title, created_at, updated_at, folder_path, audio_retention, audio_expires_at, audio_state, finalized_at)
             VALUES ('a', 'M', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', ?, 'h24', '2026-01-02T00:00:00Z', 'present', '2026-01-01T00:00:00Z')",
        )
        .bind(folder.to_string_lossy().to_string())
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;

        // While the app holds the lock, the check does nothing
        let held = try_lock(app_data).unwrap();
        assert_eq!(check(app_data).await.unwrap(), Outcome::AppRunning);
        assert!(folder.join("audio.mp4").exists());
        drop(held);

        match check(app_data).await.unwrap() {
            Outcome::Swept(report) => assert_eq!(report.deleted, 1, "{report:?}"),
            other => panic!("{other:?}"),
        }
        assert!(!folder.join("audio.mp4").exists());
        assert!(folder.join("transcripts.json").exists(), "only audio goes");
    }

    #[tokio::test]
    async fn no_database_or_an_older_one_is_left_to_the_app() {
        let data = tempfile::tempdir().unwrap();
        assert_eq!(check(data.path()).await.unwrap(), Outcome::NotReady);
        let db = data.path().join("meeting_minutes.sqlite");
        std::fs::write(&db, b"").unwrap();
        let options = sqlx::sqlite::SqliteConnectOptions::new().filename(&db);
        let pool = sqlx::sqlite::SqlitePoolOptions::new().connect_with(options).await.unwrap();
        sqlx::query("CREATE TABLE _sqlx_migrations (version INTEGER, success INTEGER)").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO _sqlx_migrations VALUES (20250916100000, 1)").execute(&pool).await.unwrap();
        pool.close().await;
        assert_eq!(check(data.path()).await.unwrap(), Outcome::NotReady, "never migrates");
    }

    #[tokio::test]
    async fn the_task_is_wanted_while_audio_has_a_deadline() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("UPDATE retention_state SET default_retention = 'keep' WHERE id = 1")
            .execute(&pool)
            .await
            .unwrap();
        assert!(!wanted(&pool).await);
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at, audio_state, audio_expires_at) VALUES ('m', 'M', 'x', 'x', 'present', '2999-12-01T00:00:00Z')")
            .execute(&pool)
            .await
            .unwrap();
        assert!(wanted(&pool).await);
    }

    #[test]
    fn the_task_runs_after_a_missed_start_and_on_battery() {
        let xml = task_xml(r"C:\Program Files\Q & A\quire.exe");
        assert!(xml.contains("<StartWhenAvailable>true</StartWhenAvailable>"));
        assert!(xml.contains("<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>"));
        assert!(xml.contains("<StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>"));
        assert!(xml.contains(r"<Command>C:\Program Files\Q &amp; A\quire.exe</Command>"), "the path is escaped, never quoted by hand");
        assert!(xml.contains("<Arguments>--retention-sweep</Arguments>"));
        assert!(task_name().starts_with("Quire daily audio check ("));
    }

    #[test]
    fn roots_come_from_the_saved_preferences_on_disk() {
        let data = tempfile::tempdir().unwrap();
        assert!(roots_from_disk(data.path()).contains(&data.path().join("recordings")));
        std::fs::write(
            data.path().join("recording_preferences.json"),
            serde_json::json!({ "preferences": crate::audio::recording_preferences::RecordingPreferences { save_folder: PathBuf::from("/chosen/folder"), ..Default::default() } }).to_string(),
        )
        .unwrap();
        assert!(roots_from_disk(data.path()).contains(&PathBuf::from("/chosen/folder")));
    }
}
