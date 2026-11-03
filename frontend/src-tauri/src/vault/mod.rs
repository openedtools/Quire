//! The shared vault (spec F-SYNC, `sync-format.md`): Phase E writes it one way, from the database
//! to a folder the person chose, for the iPhone app to read. Nothing is ever read back into the
//! database in this phase.
//!
//! Every change that schedules the Markdown export also schedules a vault run; one background
//! worker exports the whole database after a short pause, writing only what changed.

pub mod export;
pub mod format;
pub mod store;

use std::sync::Mutex;
use std::time::{Duration, Instant};

use log::{info, warn};
use once_cell::sync::Lazy;
use serde::Serialize;
use sqlx::SqlitePool;
use tauri::{AppHandle, Runtime};
use tauri_plugin_dialog::DialogExt;

use crate::state::AppState;
use export::ExportReport;

/// Pause after a change, so a burst of changes (typing notes) writes once.
pub const DELAY: Duration = Duration::from_secs(5);
const RETRY_DELAY: Duration = Duration::from_secs(60);

static RUN_LOCK: Lazy<tokio::sync::Mutex<()>> = Lazy::new(|| tokio::sync::Mutex::new(()));
static NEXT: Lazy<Mutex<Option<(Instant, SqlitePool)>>> = Lazy::new(|| Mutex::new(None));
static WORKER: Lazy<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>> = Lazy::new(|| Mutex::new(None));

/// Runs one export now if the vault is on. None when it is off.
pub async fn run(pool: &SqlitePool) -> Result<Option<ExportReport>, String> {
    let _guard = RUN_LOCK.lock().await;
    let settings = store::settings(pool).await.map_err(|e| {
        warn!("Vault: could not read settings: {}", e);
        "Could not read the vault settings".to_string()
    })?;
    let Some(folder) = settings.folder.filter(|_| settings.enabled) else { return Ok(None) };
    let report = export::export(pool, &folder).await?;
    if report.written + report.moved + report.deleted + report.failed + report.changed_in_vault > 0 || !report.verified {
        info!("Vault export: {:?}", report);
    }
    if !report.verified {
        // Try again in a minute: a file locked by iCloud or still downloading is usually free by then
        schedule(pool, RETRY_DELAY);
    }
    Ok(Some(report))
}

/// Asks for a vault run after `delay`; a later call pushes it back.
pub fn schedule(pool: &SqlitePool, delay: Duration) {
    // The worker lock is held while the request is set, and the worker only stops under it after
    // seeing no request, so a request is never lost between the two
    let mut worker = WORKER.lock().unwrap_or_else(|e| e.into_inner());
    *NEXT.lock().unwrap_or_else(|e| e.into_inner()) = Some((Instant::now() + delay, pool.clone()));
    let running = matches!(&*worker, Some(tauri::async_runtime::JoinHandle::Tokio(w)) if !w.is_finished());
    if !running {
        *worker = Some(tauri::async_runtime::spawn(async {
            loop {
                let due = {
                    let mut worker = WORKER.lock().unwrap_or_else(|e| e.into_inner());
                    let due = NEXT.lock().unwrap_or_else(|e| e.into_inner()).clone();
                    if due.is_none() {
                        *worker = None;
                    }
                    due
                };
                let Some((when, pool)) = due else { return };
                let now = Instant::now();
                if when > now {
                    tokio::time::sleep(when - now).await;
                    continue;
                }
                *NEXT.lock().unwrap_or_else(|e| e.into_inner()) = None;
                if let Err(e) = run(&pool).await {
                    warn!("Vault export did not run: {}", e);
                }
            }
        }));
    }
}

// ===== Commands for the Sync screen (F-SYNC-1, F-SYNC-2, F-SYNC-9) =====

#[derive(Debug, Clone, Serialize)]
pub struct VaultStatus {
    pub enabled: bool,
    pub folder: Option<String>,
    pub last_export_at: Option<String>,
    pub last_report: Option<ExportReport>,
    /// The iCloud Drive folder to offer, when this computer has one
    pub suggested_folder: Option<String>,
    /// The chosen folder's path is long enough to warn about (section 15)
    pub long_path: bool,
}

fn db_error(e: sqlx::Error) -> String {
    warn!("Vault: database error: {}", e);
    "Could not read or update the vault settings".to_string()
}

/// `iCloudDrive\Quire` on Windows, the iCloud Drive folder on a Mac (plan PE5).
pub fn icloud_suggestion() -> Option<String> {
    let home = dirs::home_dir()?;
    let drive = if cfg!(windows) { home.join("iCloudDrive") } else { home.join("Library/Mobile Documents/com~apple~CloudDocs") };
    drive.is_dir().then(|| drive.join(crate::brand_generated::NAME).to_string_lossy().to_string())
}

async fn status(pool: &SqlitePool) -> Result<VaultStatus, String> {
    let s = store::settings(pool).await.map_err(db_error)?;
    Ok(VaultStatus {
        enabled: s.enabled,
        long_path: s.folder.as_deref().map_or(false, |f| f.chars().count() > export::ROOT_WARN_CHARS),
        folder: s.folder,
        last_export_at: s.last_export_at,
        last_report: s.last_report.and_then(|r| serde_json::from_str(&r).ok()),
        suggested_folder: icloud_suggestion(),
    })
}

#[tauri::command]
pub async fn api_vault_status(state: tauri::State<'_, AppState>) -> Result<VaultStatus, String> {
    status(state.db_manager.pool()).await
}

#[tauri::command]
pub async fn api_vault_pick_folder<R: Runtime>(app: AppHandle<R>) -> Result<Option<String>, String> {
    let picked = tokio::task::spawn_blocking(move || app.dialog().file().set_title("Choose a folder for the Quire vault").blocking_pick_folder())
        .await
        .map_err(|e| format!("Folder dialog failed: {}", e))?;
    Ok(picked.and_then(|p| p.into_path().ok()).map(|p| p.to_string_lossy().to_string()))
}

/// Checks a folder for the vault and turns it on: a missing folder inside an existing one is
/// created, and the vault never shares a folder with the Markdown export (plan PE2).
pub async fn enable(pool: &SqlitePool, folder: &str) -> Result<(), String> {
    let folder = folder.trim().to_string();
    let path = std::path::Path::new(&folder);
    let markdown = crate::markdown_export::store::settings(pool).await.ok().and_then(|s| s.folder).and_then(|m| std::fs::canonicalize(m).ok());
    let overlaps = |root: &std::path::Path| markdown.as_ref().map_or(false, |md| md == root || root.starts_with(md) || md.starts_with(root));
    const OVERLAP: &str = "Choose a folder apart from the Markdown export folder, so Obsidian does not show the vault's files.";
    if path.is_absolute() && !path.exists() {
        // Check before creating anything, then create the missing last folder
        let parent = path.parent().and_then(|p| std::fs::canonicalize(p).ok()).ok_or("That folder could not be found")?;
        if overlaps(&parent.join(path.file_name().unwrap_or_default())) {
            return Err(OVERLAP.into());
        }
        std::fs::create_dir(path).map_err(|e| format!("Could not create that folder ({:?})", e.kind()))?;
    }
    let root = export::check_root(&folder)?;
    if overlaps(&root) {
        return Err(OVERLAP.into());
    }
    let _guard = RUN_LOCK.lock().await;
    store::enable(pool, &folder).await.map_err(db_error)
}

/// Turns the vault on after the window has shown the disclosure, then exports in the background.
#[tauri::command]
pub async fn api_vault_enable(state: tauri::State<'_, AppState>, folder: String) -> Result<VaultStatus, String> {
    let pool = state.db_manager.pool();
    enable(pool, &folder).await?;
    schedule(pool, Duration::ZERO);
    status(pool).await
}

/// Stops writing; the files stay (F-SYNC-9).
#[tauri::command]
pub async fn api_vault_disable(state: tauri::State<'_, AppState>) -> Result<VaultStatus, String> {
    let pool = state.db_manager.pool();
    // No lock: a running export notices within a few files and stops (review finding)
    store::disable(pool).await.map_err(db_error)?;
    status(pool).await
}

#[tauri::command]
pub async fn api_vault_export_now(state: tauri::State<'_, AppState>) -> Result<ExportReport, String> {
    run(state.db_manager.pool()).await?.ok_or_else(|| "The vault is off".to_string())
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct RemoveReport {
    pub removed: usize,
    /// Files changed in the vault since the app wrote them: left in place
    pub kept: usize,
}

/// The separate, confirmed action that removes the app's files from the vault (F-SYNC-9):
/// only files it wrote and that are unchanged, then the folders it made once they are empty.
/// The vault is turned off.
pub async fn remove_files(pool: &SqlitePool) -> Result<RemoveReport, String> {
    let _guard = RUN_LOCK.lock().await;
    let settings = store::settings(pool).await.map_err(db_error)?;
    let Some(folder) = settings.folder.clone() else { return Ok(RemoveReport::default()) };
    let root = export::check_root(&folder)?;
    let mut report = RemoveReport::default();
    let mut dirs: Vec<String> = vec!["_people".into(), "_tmp".into(), "_trash".into()];
    let mut rows = store::pending(pool).await.map_err(db_error)?;
    rows.extend(store::state(pool).await.map_err(db_error)?);
    let mut left: Vec<store::StateRow> = Vec::new();
    for row in rows {
        // Every part of the path is checked, so a link never leads outside the vault
        let Some(bytes) = export::read_owned(&root, &row.rel_path) else { continue };
        let path = row.rel_path.split('/').fold(root.clone(), |p, c| p.join(c));
        if format::sha256_hex(&bytes) != row.sha256 {
            report.kept += 1;
        } else if std::fs::remove_file(&path).is_ok() {
            report.removed += 1;
        } else {
            left.push(row.clone());
        }
        let mut dir = row.rel_path.as_str();
        while let Some((parent, _)) = dir.rsplit_once('/') {
            dirs.push(parent.to_string());
            dir = parent;
        }
    }
    report.kept += left.len();
    let marker = root.join("quire-format.json");
    let made_here = std::fs::read_to_string(&marker)
        .ok()
        .and_then(|t| format::parse_marker(&t).ok())
        .map_or(false, |m| Some(&m.created_by) == settings.device_id.as_ref());
    if made_here && report.kept == 0 && std::fs::remove_file(&marker).is_ok() {
        report.removed += 1;
    }
    // Deepest first, so `_trash/<id>` goes before `_trash`
    dirs.sort_by_key(|d| std::cmp::Reverse(d.matches('/').count()));
    dirs.dedup();
    for dir in dirs {
        let _ = std::fs::remove_dir(dir.split('/').fold(root.clone(), |p, c| p.join(c)));
    }
    store::clear_state(pool).await.map_err(db_error)?;
    // A file that could not be removed is still the app's
    store::replace_state(pool, &left).await.map_err(db_error)?;
    store::disable(pool).await.map_err(db_error)?;
    Ok(report)
}

#[tauri::command]
pub async fn api_vault_remove_files(state: tauri::State<'_, AppState>) -> Result<RemoveReport, String> {
    remove_files(state.db_manager.pool()).await
}

#[cfg(test)]
mod tests;
