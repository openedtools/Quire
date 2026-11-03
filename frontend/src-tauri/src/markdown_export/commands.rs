//! Commands behind the Markdown export card in Settings (spec F-MD-1, F-MD-5, F-MD-6).

use log::warn;
use serde::Serialize;
use sqlx::SqlitePool;
use tauri::{AppHandle, Runtime};
use tauri_plugin_dialog::DialogExt;

use super::store;
use super::writer::{self, FileState};
use super::ExportReport;
use crate::state::AppState;

/// A file the app will not change because it was edited, moved or removed in another app.
#[derive(Debug, Clone, Serialize)]
pub struct ChangedFile {
    pub meeting_id: String,
    pub title: String,
    pub path: String,
    /// `edited` or `missing`.
    pub state: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportStatus {
    pub enabled: bool,
    pub folder: Option<String>,
    /// The sync service the folder is in, when it can be told from the path.
    pub cloud_provider: Option<String>,
    pub last_export_at: Option<String>,
    pub exported: usize,
    pub changed: Vec<ChangedFile>,
}

fn db_error(context: &str, e: sqlx::Error) -> String {
    warn!("Markdown export: {} failed: {}", context, e);
    "Could not read the export settings".to_string()
}

async fn status(pool: &SqlitePool) -> Result<ExportStatus, String> {
    let settings = store::settings(pool).await.map_err(|e| db_error("status", e))?;
    let rows = store::rows(pool).await.map_err(|e| db_error("status", e))?;
    let root = settings.folder.as_deref().and_then(|f| writer::open_root(f).ok());
    let mut changed = Vec::new();
    if let Some(root) = &root {
        for row in &rows {
            let state = match writer::file_state(root, &row.rel_path, &row.sha256) {
                FileState::Clean => continue,
                FileState::Edited => "edited",
                FileState::Missing => "missing",
            };
            let title: Option<(String,)> = sqlx::query_as("SELECT title FROM meetings WHERE id = ?")
                .bind(&row.meeting_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| db_error("status", e))?;
            let Some((title,)) = title else { continue };
            changed.push(ChangedFile { meeting_id: row.meeting_id.clone(), title, path: row.rel_path.clone(), state: state.to_string() });
        }
    }
    let cloud_provider = settings
        .folder
        .as_deref()
        .and_then(|f| crate::audio::recording_preferences::cloud_sync_provider(std::path::Path::new(f)))
        .map(str::to_string);
    Ok(ExportStatus {
        enabled: settings.enabled,
        cloud_provider,
        folder: settings.folder,
        last_export_at: settings.last_export_at,
        exported: rows.len(),
        changed,
    })
}

#[tauri::command]
pub async fn api_markdown_export_status(state: tauri::State<'_, AppState>) -> Result<ExportStatus, String> {
    status(state.db_manager.pool()).await
}

/// Turns the export on for a folder the person chose and confirmed, then exports every
/// meeting in the background.
#[tauri::command]
pub async fn api_markdown_export_enable(state: tauri::State<'_, AppState>, folder: String) -> Result<ExportStatus, String> {
    let pool = state.db_manager.pool();
    let root = writer::open_root(&folder).map_err(|e| e.to_string())?;
    // Never inside the vault or around it (plan PE2)
    let vault = crate::vault::store::settings(pool).await.ok().and_then(|s| s.folder).and_then(|v| std::fs::canonicalize(v).ok());
    if vault.map_or(false, |v| v == root || root.starts_with(&v) || v.starts_with(&root)) {
        return Err("Choose a folder apart from the vault folder, so Obsidian does not show the vault's files.".to_string());
    }
    let root = root.to_string_lossy().to_string();
    {
        let _guard = super::settings_lock().await;
        store::enable(pool, &root).await.map_err(|e| db_error("enable", e))?;
    }
    let ids = store::all_meeting_ids(pool).await.map_err(|e| db_error("enable", e))?;
    super::schedule_many(pool, &ids, std::time::Duration::ZERO);
    status(pool).await
}

/// Stops writing. Files already written stay where they are.
#[tauri::command]
pub async fn api_markdown_export_disable(state: tauri::State<'_, AppState>) -> Result<ExportStatus, String> {
    let pool = state.db_manager.pool();
    {
        let _guard = super::settings_lock().await;
        store::disable(pool).await.map_err(|e| db_error("disable", e))?;
    }
    status(pool).await
}

#[tauri::command]
pub async fn api_markdown_export_all(state: tauri::State<'_, AppState>) -> Result<ExportReport, String> {
    let pool = state.db_manager.pool();
    let ids = store::all_meeting_ids(pool).await.map_err(|e| db_error("export all", e))?;
    super::run(pool, &ids, false).await?.ok_or_else(|| "Markdown export is off".to_string())
}

/// Puts Quire's version back over a file changed in another app, at the person's request.
#[tauri::command]
pub async fn api_markdown_export_replace(state: tauri::State<'_, AppState>, meeting_id: String) -> Result<ExportStatus, String> {
    let pool = state.db_manager.pool();
    let report = super::run(pool, &[meeting_id], true).await?.ok_or_else(|| "Markdown export is off".to_string())?;
    if report.failed > 0 {
        return Err("Could not replace that file".to_string());
    }
    status(pool).await
}

/// A folder the person picked, before they confirm it.
#[derive(Debug, Clone, Serialize)]
pub struct FolderChoice {
    pub path: String,
    /// The sync service the folder is in, when it can be told from the path.
    pub cloud_provider: Option<String>,
}

#[tauri::command]
pub async fn api_markdown_export_pick_folder<R: Runtime>(app: AppHandle<R>) -> Result<Option<FolderChoice>, String> {
    let picked = tokio::task::spawn_blocking(move || {
        app.dialog().file().set_title("Choose a folder for Markdown files").blocking_pick_folder()
    })
    .await
    .map_err(|e| format!("Folder dialog failed: {}", e))?;
    let Some(picked) = picked else { return Ok(None) };
    let path = picked.into_path().map_err(|_| "The chosen folder is not a local folder".to_string())?;
    Ok(Some(FolderChoice {
        cloud_provider: crate::audio::recording_preferences::cloud_sync_provider(&path).map(str::to_string),
        path: path.to_string_lossy().to_string(),
    }))
}
