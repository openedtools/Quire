//! The master "Allow sending data to cloud services" switch (spec F-MOD-5, F-MOD-7, plan PC2).
//! One stored boolean, read by `cloud_allowed()`, which every path that could send a transcript
//! off this computer checks before it builds a request. Off for fresh installs.

use std::sync::atomic::{AtomicBool, Ordering};

use log::info;
use serde::Serialize;
use sqlx::SqlitePool;

use crate::database::repositories::setting::SettingsRepository;
use crate::summary::llm_client::LLMProvider;

static ALLOWED: AtomicBool = AtomicBool::new(false);

/// True when the person has allowed sending data to cloud services. False until the database
/// has been read, so nothing leaves early.
pub fn cloud_allowed() -> bool {
    ALLOWED.load(Ordering::SeqCst)
}

/// True when the address is on this computer (a loopback address or `localhost`).
pub fn is_local_address(address: &str) -> bool {
    let Ok(parsed) = url::Url::parse(address.trim()) else { return false };
    match parsed.host() {
        Some(url::Host::Domain(name)) => {
            let name = name.to_ascii_lowercase();
            name == "localhost" || name.ends_with(".localhost")
        }
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

/// True when two server addresses are the same, ignoring surrounding spaces and a final slash.
pub fn same_address(a: &str, b: &str) -> bool {
    a.trim().trim_end_matches('/') == b.trim().trim_end_matches('/')
}

/// An address for a log line: scheme, host and port only, so a key in the address never reaches
/// the log.
pub fn redacted(address: &str) -> String {
    match url::Url::parse(address.trim()) {
        Ok(u) => format!("{}://{}{}", u.scheme(), u.host_str().unwrap_or("?"), u.port().map(|p| format!(":{}", p)).unwrap_or_default()),
        Err(_) => "an invalid address".to_string(),
    }
}

/// True when using this provider sends data off this computer: a cloud provider, or Ollama or a
/// custom server at another computer's address (PC2).
pub fn needs_cloud(provider: &LLMProvider, endpoint: Option<&str>) -> bool {
    !crate::ask::policy::is_on_this_computer(provider, endpoint)
}

pub const SWITCH_OFF: &str = "Sending data to cloud services is off. Turn it on in Settings, Privacy, to use this provider.";

/// The check every cloud path makes before a request; `allowed` is `cloud_allowed()` in the app.
pub fn guard(provider: &LLMProvider, endpoint: Option<&str>, allowed: bool) -> Result<(), String> {
    if needs_cloud(provider, endpoint) && !allowed {
        return Err(SWITCH_OFF.to_string());
    }
    Ok(())
}

/// The same check for a request to an address (model lists, Ollama, the custom server test).
pub fn guard_address(address: &str, allowed: bool) -> Result<(), String> {
    if !is_local_address(address) && !allowed {
        return Err(SWITCH_OFF.to_string());
    }
    Ok(())
}

/// Saving a non-local Ollama or custom server address needs the person's confirmation (F-MOD-7).
pub fn remote_needs_confirmation(address: Option<&str>, confirmed: bool) -> bool {
    match address.map(str::trim).filter(|a| !a.is_empty()) {
        Some(address) => !is_local_address(address) && !confirmed,
        None => false,
    }
}

pub const NEEDS_CONFIRMATION: &str = "needs_remote_confirmation";

fn provider_name(provider: &str) -> &'static str {
    match provider {
        "openai" => "OpenAI",
        "claude" => "Anthropic Claude",
        "groq" => "Groq",
        "openrouter" => "OpenRouter",
        "ollama" => "Ollama on another computer",
        "custom-openai" => "a custom server on another computer",
        _ => "a cloud provider",
    }
}

/// What an existing install already uses that sends data off this computer, if anything: a cloud
/// provider with a saved key, or Ollama or a custom server at a remote address.
pub async fn already_in_use(pool: &SqlitePool) -> Option<String> {
    let setting = SettingsRepository::get_model_config(pool).await.ok().flatten()?;
    let provider = LLMProvider::from_str(&setting.provider).ok()?;
    let in_use = match provider {
        LLMProvider::OpenAI | LLMProvider::Claude | LLMProvider::Groq | LLMProvider::OpenRouter => {
            SettingsRepository::get_api_key(pool, &setting.provider).await.ok().flatten().is_some()
        }
        LLMProvider::Ollama => needs_cloud(&provider, setting.ollama_endpoint.as_deref()),
        LLMProvider::CustomOpenAI => {
            let endpoint = SettingsRepository::get_custom_openai_config(pool).await.ok().flatten().map(|c| c.endpoint);
            endpoint.is_some() && needs_cloud(&provider, endpoint.as_deref())
        }
        LLMProvider::BuiltInAI => false,
    };
    in_use.then(|| provider_name(&setting.provider).to_string())
}

/// Reads the switch, deciding it once on the first launch of this build: on (with a one-time
/// notice) only when the install already uses a cloud provider, otherwise off.
pub async fn initialize(pool: &SqlitePool) -> bool {
    let row: Option<(i64,)> = sqlx::query_as("SELECT allow_cloud FROM cloud_settings WHERE id = 1").fetch_optional(pool).await.ok().flatten();
    let allowed = match row {
        Some((allow,)) => allow != 0,
        // Saved keys cannot be read right now (a Keychain prompt declined, say): decide on a later
        // launch rather than lock an existing cloud user out. Off until then.
        None if !crate::secrets::off_thread(crate::secrets::available).await => false,
        None => {
            let in_use = already_in_use(pool).await;
            let allowed = in_use.is_some();
            let _ = sqlx::query("INSERT OR IGNORE INTO cloud_settings (id, allow_cloud, notice) VALUES (1, ?, ?)")
                .bind(allowed as i64)
                .bind(in_use.as_deref())
                .execute(pool)
                .await;
            if allowed {
                info!("Cloud services stay allowed: this install already used one");
            }
            allowed
        }
    };
    ALLOWED.store(allowed, Ordering::SeqCst);
    allowed
}

pub async fn set_allowed(pool: &SqlitePool, allowed: bool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO cloud_settings (id, allow_cloud, notice) VALUES (1, ?, NULL)
         ON CONFLICT(id) DO UPDATE SET allow_cloud = excluded.allow_cloud, notice = NULL",
    )
    .bind(allowed as i64)
    .execute(pool)
    .await?;
    ALLOWED.store(allowed, Ordering::SeqCst);
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct CloudStatus {
    pub allowed: bool,
    /// Set once after an upgrade that kept the switch on: what is enabled
    pub notice: Option<String>,
}

#[tauri::command]
pub async fn api_cloud_status(state: tauri::State<'_, crate::state::AppState>) -> Result<CloudStatus, String> {
    let pool = state.db_manager.pool();
    let notice: Option<(Option<String>,)> =
        sqlx::query_as("SELECT notice FROM cloud_settings WHERE id = 1").fetch_optional(pool).await.map_err(|e| e.to_string())?;
    Ok(CloudStatus { allowed: cloud_allowed(), notice: notice.and_then(|(n,)| n) })
}

#[tauri::command]
pub async fn api_set_cloud_allowed(state: tauri::State<'_, crate::state::AppState>, allowed: bool) -> Result<CloudStatus, String> {
    set_allowed(state.db_manager.pool(), allowed).await.map_err(|e| e.to_string())?;
    Ok(CloudStatus { allowed, notice: None })
}

#[tauri::command]
pub async fn api_cloud_dismiss_notice(state: tauri::State<'_, crate::state::AppState>) -> Result<(), String> {
    sqlx::query("UPDATE cloud_settings SET notice = NULL WHERE id = 1")
        .execute(state.db_manager.pool())
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    /// Every provider, so a new one cannot be added without deciding whether it is cloud.
    fn every_provider() -> Vec<LLMProvider> {
        let all = vec![
            LLMProvider::OpenAI,
            LLMProvider::Claude,
            LLMProvider::Groq,
            LLMProvider::Ollama,
            LLMProvider::OpenRouter,
            LLMProvider::BuiltInAI,
            LLMProvider::CustomOpenAI,
        ];
        for p in &all {
            // Fails to compile when a variant is added and not listed above
            match p {
                LLMProvider::OpenAI
                | LLMProvider::Claude
                | LLMProvider::Groq
                | LLMProvider::Ollama
                | LLMProvider::OpenRouter
                | LLMProvider::BuiltInAI
                | LLMProvider::CustomOpenAI => {}
            }
        }
        all
    }

    #[test]
    fn with_the_switch_off_every_cloud_path_stops_before_a_request() {
        for provider in every_provider() {
            let local = match provider {
                LLMProvider::CustomOpenAI => Some("http://localhost:1234/v1"),
                _ => None,
            };
            let cloud = matches!(provider, LLMProvider::OpenAI | LLMProvider::Claude | LLMProvider::Groq | LLMProvider::OpenRouter);
            assert_eq!(guard(&provider, local, false).is_err(), cloud, "{:?}", provider);
            assert!(guard(&provider, local, true).is_ok(), "{:?}", provider);
            if cloud && provider != LLMProvider::BuiltInAI {
                // The real request builder refuses too
                let built = crate::summary::llm_client::provider_endpoint_with(&provider, "k", None, None, false);
                assert_eq!(built.err().as_deref(), Some(SWITCH_OFF), "{:?}", provider);
            }
        }
    }

    #[test]
    fn ollama_or_a_custom_server_elsewhere_needs_the_switch_too() {
        assert!(guard(&LLMProvider::Ollama, Some("http://192.168.1.20:11434"), false).is_err());
        assert!(guard(&LLMProvider::Ollama, Some("http://127.0.0.1:11434"), false).is_ok());
        assert!(guard(&LLMProvider::CustomOpenAI, Some("https://example.com/v1"), false).is_err());
        assert!(guard_address("https://api.openai.com/v1/models", false).is_err());
        assert!(guard_address("http://localhost:11434/api/tags", false).is_ok());
    }

    #[test]
    fn a_remote_address_needs_confirmation_before_it_is_saved() {
        assert!(remote_needs_confirmation(Some("http://192.168.1.20:11434"), false));
        assert!(!remote_needs_confirmation(Some("http://192.168.1.20:11434"), true));
        assert!(!remote_needs_confirmation(Some("http://localhost:11434"), false));
        assert!(!remote_needs_confirmation(Some("http://[::1]:8080/v1"), false));
        assert!(!remote_needs_confirmation(None, false));
        assert!(!remote_needs_confirmation(Some("  "), false));
        assert!(remote_needs_confirmation(Some("http://localhost.example.com"), false), "a look-alike host is remote");
    }

    #[tokio::test]
    async fn a_fresh_install_starts_off() {
        let pool = pool().await;
        assert!(!initialize(&pool).await);
        let notice: Option<String> = sqlx::query_scalar("SELECT notice FROM cloud_settings").fetch_one(&pool).await.unwrap();
        assert_eq!(notice, None);
    }

    #[tokio::test]
    async fn an_upgrade_with_a_cloud_provider_and_key_starts_on_with_a_notice() {
        let pool = pool().await;
        sqlx::query("INSERT INTO settings (id, provider, model, whisperModel) VALUES ('1', 'claude', 'm', 'large-v3')")
            .execute(&pool)
            .await
            .unwrap();
        SettingsRepository::save_api_key(&pool, "claude", "sk-ant").await.unwrap();
        assert!(initialize(&pool).await);
        let notice: Option<String> = sqlx::query_scalar("SELECT notice FROM cloud_settings").fetch_one(&pool).await.unwrap();
        assert_eq!(notice.as_deref(), Some("Anthropic Claude"));
        // Decided once: turning it off sticks across launches
        set_allowed(&pool, false).await.unwrap();
        assert!(!initialize(&pool).await);
    }

    #[tokio::test]
    async fn a_cloud_provider_without_a_key_starts_off() {
        let pool = pool().await;
        sqlx::query("INSERT INTO settings (id, provider, model, whisperModel) VALUES ('1', 'openai', 'm', 'large-v3')")
            .execute(&pool)
            .await
            .unwrap();
        assert!(!initialize(&pool).await);
    }

    #[tokio::test]
    async fn the_decision_waits_when_saved_keys_cannot_be_read() {
        let pool = pool().await;
        sqlx::query("INSERT INTO settings (id, provider, model, whisperModel) VALUES ('1', 'claude', 'm', 'large-v3')")
            .execute(&pool)
            .await
            .unwrap();
        SettingsRepository::save_api_key(&pool, "claude", "sk-ant").await.unwrap();
        crate::secrets::backend::make_unavailable(true);
        assert!(!initialize(&pool).await, "off for now");
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cloud_settings").fetch_one(&pool).await.unwrap();
        assert_eq!(rows, 0, "nothing decided, so a later launch can keep the cloud user on");
        crate::secrets::backend::make_unavailable(false);
        assert!(initialize(&pool).await);
    }

    #[tokio::test]
    async fn a_saved_custom_server_key_stays_with_its_address() {
        use crate::summary::CustomOpenAIConfig;
        let pool = pool().await;
        let config = |endpoint: &str, key: Option<&str>| CustomOpenAIConfig {
            endpoint: endpoint.to_string(),
            api_key: key.map(str::to_string),
            model: "m".to_string(),
            max_tokens: None,
            temperature: None,
            top_p: None,
            context_size: None,
        };
        SettingsRepository::save_custom_openai_config(&pool, &config("http://localhost:1234/v1", Some("sk-1"))).await.unwrap();
        // Same address (a final slash does not matter), no new key: the key stays
        SettingsRepository::save_custom_openai_config(&pool, &config("http://localhost:1234/v1/", None)).await.unwrap();
        assert_eq!(crate::secrets::get("summary:custom-openai").unwrap().as_deref(), Some("sk-1"));
        // A different address without a new key: the old server's key is removed, never reused
        SettingsRepository::save_custom_openai_config(&pool, &config("http://localhost:9999/v1", None)).await.unwrap();
        assert_eq!(crate::secrets::get("summary:custom-openai").unwrap(), None);
        let saved = SettingsRepository::get_custom_openai_config(&pool).await.unwrap().unwrap();
        assert_eq!(saved.api_key, None);
    }

    #[test]
    fn addresses_in_logs_keep_only_scheme_host_and_port() {
        assert_eq!(redacted("https://user:sk-secret@example.com:8443/v1?key=abc"), "https://example.com:8443");
        assert_eq!(redacted("http://localhost:11434"), "http://localhost:11434");
        assert_eq!(redacted("not a url"), "an invalid address");
        assert!(same_address(" http://a/v1/ ", "http://a/v1"));
        assert!(!same_address("http://a/v1", "http://b/v1"));
    }
}
