//! API keys in the operating system's credential store: Credential Manager on Windows, the
//! Keychain on the Mac (spec F-MOD-6, plan PC1). Keys never go back to the window, and nothing
//! falls back to plain text: when the store is unavailable, saving a key is refused.

use log::{info, warn};
use serde::Serialize;
use sqlx::SqlitePool;

/// Every key is stored under the app identifier, which never changes with the name.
pub const SERVICE: &str = crate::brand_generated::BUNDLE_ID;

#[derive(Debug, Clone, PartialEq)]
pub enum SecretError {
    /// The credential store cannot be used on this computer right now
    Unavailable(String),
}

impl std::fmt::Display for SecretError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecretError::Unavailable(reason) => write!(
                f,
                "The system credential store is not available ({}), so the key was not saved. Keys are never stored as plain text.",
                reason
            ),
        }
    }
}

/// The account a key is stored under: `summary:<provider>` or `transcript:<provider>`.
pub fn account(scope: &str, provider: &str) -> String {
    format!("{}:{}", scope, provider)
}

#[cfg(not(test))]
mod backend {
    use super::{SecretError, SERVICE};

    #[cfg(any(windows, target_os = "macos"))]
    fn entry(account: &str) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new(SERVICE, account).map_err(|e| SecretError::Unavailable(e.to_string()))
    }

    #[cfg(any(windows, target_os = "macos"))]
    pub fn get(account: &str) -> Result<Option<String>, SecretError> {
        match entry(account)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(SecretError::Unavailable(e.to_string())),
        }
    }

    #[cfg(any(windows, target_os = "macos"))]
    pub fn set(account: &str, value: &str) -> Result<(), SecretError> {
        entry(account)?.set_password(value).map_err(|e| SecretError::Unavailable(e.to_string()))
    }

    #[cfg(any(windows, target_os = "macos"))]
    pub fn delete(account: &str) -> Result<(), SecretError> {
        match entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(SecretError::Unavailable(e.to_string())),
        }
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    pub fn get(_account: &str) -> Result<Option<String>, SecretError> {
        Err(SecretError::Unavailable("not supported on this system".into()))
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    pub fn set(_account: &str, _value: &str) -> Result<(), SecretError> {
        Err(SecretError::Unavailable("not supported on this system".into()))
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    pub fn delete(_account: &str) -> Result<(), SecretError> {
        Err(SecretError::Unavailable("not supported on this system".into()))
    }
}

/// Tests use an in-memory store per thread (each tokio test runs on its own thread), and can make
/// it unavailable.
#[cfg(test)]
pub mod backend {
    use super::SecretError;
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;

    thread_local! {
        static STORE: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
        static UNAVAILABLE: Cell<bool> = Cell::new(false);
    }

    pub fn make_unavailable(unavailable: bool) {
        UNAVAILABLE.with(|u| u.set(unavailable));
    }

    fn check() -> Result<(), SecretError> {
        if UNAVAILABLE.with(|u| u.get()) {
            Err(SecretError::Unavailable("test".into()))
        } else {
            Ok(())
        }
    }

    pub fn get(account: &str) -> Result<Option<String>, SecretError> {
        check()?;
        Ok(STORE.with(|s| s.borrow().get(account).cloned()))
    }

    pub fn set(account: &str, value: &str) -> Result<(), SecretError> {
        check()?;
        STORE.with(|s| s.borrow_mut().insert(account.to_string(), value.to_string()));
        Ok(())
    }

    pub fn delete(account: &str) -> Result<(), SecretError> {
        check()?;
        STORE.with(|s| s.borrow_mut().remove(account));
        Ok(())
    }
}

/// The saved key, if any. An empty value counts as none.
pub fn get(account: &str) -> Result<Option<String>, SecretError> {
    Ok(backend::get(account)?.filter(|v| !v.trim().is_empty()))
}

/// Saves a key; an empty key removes it. The value is read back before this returns `Ok`.
pub fn set(account: &str, value: &str) -> Result<(), SecretError> {
    let value = value.trim();
    if value.is_empty() {
        return backend::delete(account);
    }
    backend::set(account, value)?;
    match backend::get(account)? {
        Some(stored) if stored == value => Ok(()),
        _ => Err(SecretError::Unavailable("the saved key could not be read back".into())),
    }
}

pub fn delete(account: &str) -> Result<(), SecretError> {
    backend::delete(account)
}

/// The same calls off the async worker threads: a Keychain prompt can wait on the person for a
/// long time (review finding). Tests use the in-memory store of their own thread, so they call
/// directly.
pub async fn off_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    #[cfg(test)]
    {
        f()
    }
    #[cfg(not(test))]
    {
        tokio::task::spawn_blocking(f).await.expect("credential store call panicked")
    }
}

pub async fn get_async(account: String) -> Result<Option<String>, SecretError> {
    off_thread(move || get(&account)).await
}

pub async fn set_async(account: String, value: String) -> Result<(), SecretError> {
    off_thread(move || set(&account, &value)).await
}

pub async fn delete_async(account: String) -> Result<(), SecretError> {
    off_thread(move || delete(&account)).await
}

pub fn available() -> bool {
    backend::get(&account("check", "store")).is_ok()
}

/// Every plain-text key column the app uses, and the account it moves to (spec F-MOD-6).
pub const KEY_COLUMNS: &[(&str, &str, &str)] = &[
    ("settings", "openaiApiKey", "summary:openai"),
    ("settings", "anthropicApiKey", "summary:claude"),
    ("settings", "groqApiKey", "summary:groq"),
    ("settings", "ollamaApiKey", "summary:ollama"),
    ("settings", "openRouterApiKey", "summary:openrouter"),
];

/// Key columns nothing uses (the removed cloud transcription options, PC3, and Gemini, which was
/// never wired up): cleared, not moved, so no key sits in the credential store that the app
/// could never use or remove.
pub const CLEARED_COLUMNS: &[(&str, &str)] = &[
    ("settings", "geminiApiKey"),
    ("transcript_settings", "whisperApiKey"),
    ("transcript_settings", "deepgramApiKey"),
    ("transcript_settings", "elevenLabsApiKey"),
    ("transcript_settings", "groqApiKey"),
    ("transcript_settings", "openaiApiKey"),
];

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct MigrationReport {
    pub moved: usize,
    /// Keys of options that no longer exist, removed from the database
    pub cleared: usize,
    /// Keys still in the database because the store could not take them
    pub left: usize,
}

async fn column_exists(pool: &SqlitePool, table: &str, column: &str) -> bool {
    let sql = format!("SELECT COUNT(*) FROM pragma_table_info('{}') WHERE name = ?", table);
    sqlx::query_scalar::<_, i64>(&sql).bind(column).fetch_one(pool).await.map_or(false, |n| n > 0)
}

/// Moves each plain-text key into the store, reads it back, and only then clears the column.
/// A key the store cannot take stays where it is; nothing is written anywhere else.
pub async fn migrate_plaintext_keys(pool: &SqlitePool) -> MigrationReport {
    let mut report = MigrationReport::default();
    for (table, column) in CLEARED_COLUMNS {
        if !column_exists(pool, table, column).await {
            continue;
        }
        let sql = format!("UPDATE {} SET \"{}\" = NULL WHERE id = '1' AND TRIM(COALESCE(\"{}\", '')) <> ''", table, column, column);
        if let Ok(done) = sqlx::query(&sql).execute(pool).await {
            report.cleared += done.rows_affected() as usize;
        }
    }
    for (table, column, account) in KEY_COLUMNS {
        if !column_exists(pool, table, column).await {
            continue;
        }
        let select = format!("SELECT \"{}\" FROM {} WHERE id = '1'", column, table);
        let value: Option<String> = sqlx::query_scalar::<_, Option<String>>(&select).fetch_optional(pool).await.ok().flatten().flatten();
        let Some(value) = value.filter(|v| !v.trim().is_empty()) else { continue };
        match set_async(account.to_string(), value.clone()).await {
            Ok(()) => {
                let clear = format!("UPDATE {} SET \"{}\" = NULL WHERE id = '1'", table, column);
                if sqlx::query(&clear).execute(pool).await.is_ok() {
                    report.moved += 1;
                } else {
                    report.left += 1;
                }
            }
            Err(e) => {
                warn!("Could not move a saved key to the credential store: {}", e);
                report.left += 1;
            }
        }
    }
    // The custom server's key lives inside its JSON configuration
    if column_exists(pool, "settings", "customOpenAIConfig").await {
        let json: Option<String> = sqlx::query_scalar::<_, Option<String>>("SELECT customOpenAIConfig FROM settings WHERE id = '1'")
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .flatten();
        if let Some(mut config) = json.and_then(|j| serde_json::from_str::<serde_json::Value>(&j).ok()) {
            let key = config.get("apiKey").and_then(|k| k.as_str()).map(str::to_string).filter(|k| !k.trim().is_empty());
            if let Some(key) = key {
                match set_async(account("summary", "custom-openai"), key.clone()).await {
                    Ok(()) => {
                        config["apiKey"] = serde_json::Value::Null;
                        let saved = sqlx::query("UPDATE settings SET customOpenAIConfig = ? WHERE id = '1'")
                            .bind(config.to_string())
                            .execute(pool)
                            .await;
                        if saved.is_ok() {
                            report.moved += 1;
                        } else {
                            report.left += 1;
                        }
                    }
                    Err(e) => {
                        warn!("Could not move a saved key to the credential store: {}", e);
                        report.left += 1;
                    }
                }
            }
        }
    }
    if report.moved + report.left + report.cleared > 0 {
        info!(
            "API keys: {} moved to the credential store, {} left in place, {} unused ones cleared",
            report.moved, report.left, report.cleared
        );
    }
    report
}

/// How many keys are still in the database (for the warning in Settings).
pub async fn keys_left_in_database(pool: &SqlitePool) -> usize {
    let mut left = 0;
    for (table, column, _) in KEY_COLUMNS {
        if !column_exists(pool, table, column).await {
            continue;
        }
        let sql = format!("SELECT COUNT(*) FROM {} WHERE TRIM(COALESCE(\"{}\", '')) <> ''", table, column);
        left += sqlx::query_scalar::<_, i64>(&sql).fetch_one(pool).await.unwrap_or(0) as usize;
    }
    let custom: Option<String> = sqlx::query_scalar::<_, Option<String>>("SELECT customOpenAIConfig FROM settings WHERE id = '1'")
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .flatten();
    if custom
        .and_then(|j| serde_json::from_str::<serde_json::Value>(&j).ok())
        .and_then(|c| c.get("apiKey").and_then(|k| k.as_str()).map(|k| !k.trim().is_empty()))
        == Some(true)
    {
        left += 1;
    }
    left
}

#[derive(Debug, Clone, Serialize)]
pub struct KeyStoreStatus {
    pub available: bool,
    pub left_in_database: usize,
}

#[tauri::command]
pub async fn api_key_store_status(state: tauri::State<'_, crate::state::AppState>) -> Result<KeyStoreStatus, String> {
    Ok(KeyStoreStatus { available: off_thread(available).await, left_in_database: keys_left_in_database(state.db_manager.pool()).await })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool_with_keys() -> SqlitePool {
        let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO settings (id, provider, model, whisperModel, openaiApiKey, anthropicApiKey, geminiApiKey, customOpenAIConfig)
             VALUES ('1', 'claude', 'm', 'large-v3', 'sk-open', 'sk-ant', 'g-key', ?)",
        )
        .bind(r#"{"endpoint":"http://localhost:1234/v1","apiKey":"sk-custom","model":"m"}"#)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO transcript_settings (id, provider, model, deepgramApiKey) VALUES ('1', 'parakeet', 'p', 'dg-key')")
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn plaintext_keys_move_to_the_store_and_the_columns_are_cleared() {
        let pool = pool_with_keys().await;
        let report = migrate_plaintext_keys(&pool).await;
        assert_eq!(report, MigrationReport { moved: 3, cleared: 2, left: 0 });
        assert_eq!(get("summary:openai").unwrap().as_deref(), Some("sk-open"));
        assert_eq!(get("summary:claude").unwrap().as_deref(), Some("sk-ant"));
        assert_eq!(get("summary:custom-openai").unwrap().as_deref(), Some("sk-custom"));
        // Unused keys are cleared, not kept anywhere
        assert_eq!(get("summary:gemini").unwrap(), None);
        assert_eq!(get("transcript:deepgram").unwrap(), None);
        assert_eq!(keys_left_in_database(&pool).await, 0);
        let (open, ant): (Option<String>, Option<String>) =
            sqlx::query_as("SELECT openaiApiKey, anthropicApiKey FROM settings").fetch_one(&pool).await.unwrap();
        assert_eq!((open, ant), (None, None));
        let custom: String = sqlx::query_scalar("SELECT customOpenAIConfig FROM settings").fetch_one(&pool).await.unwrap();
        assert!(!custom.contains("sk-custom"), "the custom server key left the JSON");
        assert!(custom.contains("localhost:1234"), "the rest of the configuration stays");
        // Running again changes nothing
        assert_eq!(migrate_plaintext_keys(&pool).await, MigrationReport::default());
    }

    #[tokio::test]
    async fn an_unavailable_store_leaves_every_key_in_place_and_refuses_new_ones() {
        let pool = pool_with_keys().await;
        backend::make_unavailable(true);
        let report = migrate_plaintext_keys(&pool).await;
        assert_eq!(report, MigrationReport { moved: 0, cleared: 2, left: 3 });
        assert_eq!(keys_left_in_database(&pool).await, 3, "nothing in use is lost or copied elsewhere");
        assert!(matches!(set("summary:openai", "sk-new"), Err(SecretError::Unavailable(_))));
        assert!(!available());
        backend::make_unavailable(false);
    }

    #[test]
    fn an_empty_key_removes_the_saved_one() {
        set("summary:groq", "gsk").unwrap();
        assert_eq!(get("summary:groq").unwrap().as_deref(), Some("gsk"));
        set("summary:groq", "  ").unwrap();
        assert_eq!(get("summary:groq").unwrap(), None);
    }
}
