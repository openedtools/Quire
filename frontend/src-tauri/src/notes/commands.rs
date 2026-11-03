use super::models::UserNotes;
use crate::database::repositories::user_notes::{UserNotesError, UserNotesRepository};
use crate::state::AppState;
use log::error as log_error;

fn ui_error(context: &str, e: UserNotesError) -> String {
    if matches!(e, UserNotesError::Db(_)) {
        log_error!("{}: {}", context, e);
    }
    e.to_string()
}

/// The notes for a meeting, or None when there are none.
#[tauri::command]
pub async fn api_get_user_notes(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Option<UserNotes>, String> {
    UserNotesRepository::get(state.db_manager.pool(), &meeting_id)
        .await
        .map_err(|e| ui_error("api_get_user_notes", e))
}

/// Save the notes for a meeting. Empty notes remove the row. Returns what is stored now.
#[tauri::command]
pub async fn api_save_user_notes(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    body_blocks: String,
    body_markdown: String,
) -> Result<Option<UserNotes>, String> {
    let pool = state.db_manager.pool();
    let saved = UserNotesRepository::save(pool, &meeting_id, &body_blocks, &body_markdown)
        .await
        .map_err(|e| ui_error("api_save_user_notes", e))?;
    crate::markdown_export::schedule(pool, &meeting_id, crate::markdown_export::NOTES_DELAY);
    Ok(saved)
}
