//! Thought mode (plan Phase N, PN4): a recording of your own thoughts. It is an ordinary meeting,
//! saved and kept the same way, filed in a folder named Thoughts, with no speaker detection; its
//! summary is the "Prompt from my thoughts" template, which turns the talk into one clear prompt.

use sqlx::SqlitePool;
use tauri::{AppHandle, Manager, Runtime};

use crate::database::repositories::folders::FoldersRepository;
use crate::state::AppState;

pub const KIND: &str = "thoughts";
pub const FOLDER: &str = "Thoughts";
/// The summary template for thoughts (`templates/thought_prompt.json`)
pub const TEMPLATE: &str = "thought_prompt";

/// Whether `meeting_id` is a recording of thoughts.
pub async fn is_thoughts(pool: &SqlitePool, meeting_id: &str) -> bool {
    sqlx::query_scalar::<_, Option<String>>("SELECT kind FROM meetings WHERE id = ?")
        .bind(meeting_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .flatten()
        .as_deref()
        == Some(KIND)
}

/// Marks a saved recording as thoughts and files it in the Thoughts folder, made if needed.
pub async fn mark(pool: &SqlitePool, meeting_id: &str) -> Result<(), String> {
    let done = sqlx::query("UPDATE meetings SET kind = ? WHERE id = ?")
        .bind(KIND)
        .bind(meeting_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    if done.rows_affected() == 0 {
        return Err("The recording was not found".to_string());
    }
    let existing: Option<String> = sqlx::query_scalar("SELECT id FROM folders WHERE name = ? COLLATE NOCASE")
        .bind(FOLDER)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;
    let folder = match existing {
        Some(id) => id,
        None => FoldersRepository::create_folder(pool, FOLDER).await.map_err(|e| e.to_string())?.id,
    };
    FoldersRepository::move_meeting(pool, meeting_id, Some(&folder)).await.map_err(|e| e.to_string())?;
    crate::markdown_export::schedule(pool, meeting_id, crate::markdown_export::CHANGE_DELAY);
    Ok(())
}

fn pool_of<R: Runtime>(app: &AppHandle<R>) -> Result<SqlitePool, String> {
    Ok(app.try_state::<AppState>().ok_or("The app is still starting")?.db_manager.pool().clone())
}

#[tauri::command]
pub async fn api_mark_thoughts<R: Runtime>(app: AppHandle<R>, meeting_id: String) -> Result<(), String> {
    mark(&pool_of(&app)?, &meeting_id).await
}

#[tauri::command]
pub async fn api_meeting_kind<R: Runtime>(app: AppHandle<R>, meeting_id: String) -> Result<Option<String>, String> {
    Ok(is_thoughts(&pool_of(&app)?, &meeting_id).await.then(|| KIND.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use sqlx::sqlite::SqlitePoolOptions;

    fn fixture() -> Value {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/vault-fixtures/thoughts.json");
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn the_prompt_template_uses_the_shared_instructions() {
        let f = fixture();
        let template: Value = serde_json::from_str(crate::summary::templates::defaults::get_builtin_template(TEMPLATE).unwrap()).unwrap();
        assert_eq!(template["sections"][0]["instruction"], f["instructions"]);
        assert_eq!(template["sections"].as_array().unwrap().len(), 1);
        assert_eq!(f["folder"], FOLDER);
        assert_eq!(f["kindValue"], KIND);
        for t in f["defaultTitles"]["yes"].as_array().unwrap() {
            assert!(crate::summary::naming::is_default_title(t.as_str().unwrap()), "{t}");
        }
        for t in f["defaultTitles"]["no"].as_array().unwrap() {
            assert!(!crate::summary::naming::is_default_title(t.as_str().unwrap()), "{t}");
        }
    }

    #[tokio::test]
    async fn thoughts_are_marked_and_filed_in_one_folder() {
        let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        for id in ["a", "b", "c"] {
            sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES (?, 'Thoughts, 3 Nov 2026 14:05', '2026-11-03', '2026-11-03')").bind(id).execute(&pool).await.unwrap();
        }
        mark(&pool, "a").await.unwrap();
        mark(&pool, "b").await.unwrap();
        assert!(is_thoughts(&pool, "a").await && !is_thoughts(&pool, "c").await);
        let folders: Vec<(String, i64)> = sqlx::query_as("SELECT f.name, COUNT(m.id) FROM folders f JOIN meetings m ON m.folder_id = f.id GROUP BY f.id").fetch_all(&pool).await.unwrap();
        assert_eq!(folders, vec![("Thoughts".to_string(), 2)]);
        assert!(mark(&pool, "missing").await.is_err());
    }
}
