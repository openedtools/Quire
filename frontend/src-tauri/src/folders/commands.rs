use super::models::{Folder, FolderSummary};
use crate::database::repositories::folders::{FolderError, FoldersRepository};
use crate::state::AppState;
use log::{error as log_error, info as log_info};

fn ui_error(context: &str, e: FolderError) -> String {
    if matches!(e, FolderError::Db(_)) {
        log_error!("{}: {}", context, e);
    }
    e.to_string()
}

#[tauri::command]
pub async fn api_list_folders(state: tauri::State<'_, AppState>) -> Result<Vec<FolderSummary>, String> {
    FoldersRepository::list_folders(state.db_manager.pool())
        .await
        .map_err(|e| ui_error("api_list_folders", e))
}

#[tauri::command]
pub async fn api_create_folder(
    state: tauri::State<'_, AppState>,
    name: String,
) -> Result<Folder, String> {
    let folder = FoldersRepository::create_folder(state.db_manager.pool(), &name)
        .await
        .map_err(|e| ui_error("api_create_folder", e))?;
    crate::vault::schedule(state.db_manager.pool(), crate::vault::DELAY);
    Ok(folder)
}

#[tauri::command]
pub async fn api_rename_folder(
    state: tauri::State<'_, AppState>,
    folder_id: String,
    name: String,
) -> Result<Folder, String> {
    let pool = state.db_manager.pool();
    let folder = FoldersRepository::rename_folder(pool, &folder_id, &name)
        .await
        .map_err(|e| ui_error("api_rename_folder", e))?;
    schedule_folder(pool, &folder_id).await;
    Ok(folder)
}

/// Re-exports the meetings filed in a folder, whose Markdown files live in its directory.
async fn schedule_folder(pool: &sqlx::SqlitePool, folder_id: &str) {
    if let Ok(ids) = crate::markdown_export::store::meeting_ids_in_folder(pool, folder_id).await {
        crate::markdown_export::schedule_many(pool, &ids, crate::markdown_export::CHANGE_DELAY);
    }
}

/// Delete a folder; its meetings are kept and become unfiled.
#[tauri::command]
pub async fn api_delete_folder(
    state: tauri::State<'_, AppState>,
    folder_id: String,
) -> Result<bool, String> {
    // log-ok: a folder id, not its name
    log_info!("api_delete_folder called for {}", folder_id);
    let pool = state.db_manager.pool();
    let filed = crate::markdown_export::store::meeting_ids_in_folder(pool, &folder_id).await.unwrap_or_default();
    let deleted = FoldersRepository::delete_folder(pool, &folder_id)
        .await
        .map_err(|e| ui_error("api_delete_folder", e))?;
    crate::markdown_export::schedule_many(pool, &filed, crate::markdown_export::CHANGE_DELAY);
    Ok(deleted)
}

/// File a meeting in a folder, or pass no folder to make it unfiled.
#[tauri::command]
pub async fn api_move_meeting_to_folder(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    folder_id: Option<String>,
) -> Result<(), String> {
    let pool = state.db_manager.pool();
    FoldersRepository::move_meeting(pool, &meeting_id, folder_id.as_deref())
        .await
        .map_err(|e| ui_error("api_move_meeting_to_folder", e))?;
    crate::markdown_export::schedule(pool, &meeting_id, crate::markdown_export::CHANGE_DELAY);
    Ok(())
}
