//! Naming a meeting with the chosen model (plan Phase L, PL1 and PL2): automatically when a
//! recording or import is finalized, while the meeting still has its default name, and again
//! on request ("Suggest a name"). The rules are in `naming.rs`.

use chrono::{DateTime, Datelike, Local, Timelike, Utc};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::future::Future;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::naming::{clean_topic, format_title, is_default_title, title_prompt, TitleFields, DEFAULT_TITLE_FORMAT, TITLE_INSTRUCTIONS};
use crate::database::repositories::user_notes::UserNotesRepository;
use crate::state::AppState;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamingSettings {
    pub auto_name: bool,
    pub title_format: String,
}

impl Default for NamingSettings {
    fn default() -> Self {
        Self { auto_name: true, title_format: DEFAULT_TITLE_FORMAT.to_string() }
    }
}

pub async fn settings(pool: &SqlitePool) -> Result<NamingSettings, sqlx::Error> {
    let row: Option<(i64, String)> = sqlx::query_as("SELECT auto_name, title_format FROM naming_settings WHERE id = 1").fetch_optional(pool).await?;
    Ok(row.map_or_else(NamingSettings::default, |(auto, format)| NamingSettings { auto_name: auto != 0, title_format: format }))
}

pub async fn set_settings(pool: &SqlitePool, s: &NamingSettings) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO naming_settings (id, auto_name, title_format) VALUES (1, ?, ?)
         ON CONFLICT(id) DO UPDATE SET auto_name = excluded.auto_name, title_format = excluded.title_format",
    )
    .bind(s.auto_name as i64)
    .bind(s.title_format.trim())
    .execute(pool)
    .await?;
    Ok(())
}

/// Whether a name is made: automatically (only for a default name, with the setting on) or on
/// request (always).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    Finalized,
    Requested,
}

/// The transcript as `Speaker: text` lines, in time order, for the title prompt.
async fn transcript_text(pool: &SqlitePool, meeting_id: &str) -> Result<String, sqlx::Error> {
    let rows: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT t.transcript, t.speaker, p.name
         FROM transcripts t
         LEFT JOIN meeting_speakers ms ON ms.meeting_id = t.meeting_id AND ms.speaker_key = t.speaker
         LEFT JOIN people p ON p.id = ms.person_id
         WHERE t.meeting_id = ?
         ORDER BY t.audio_start_time, t.timestamp",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter(|(text, _, _)| !text.trim().is_empty())
        .map(|(text, key, person)| match person.or(key) {
            Some(speaker) => format!("{speaker}: {}", text.trim()),
            None => text.trim().to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

/// Names the meeting with `generate` (the model call: instructions and prompt in, reply out).
/// Finalized: writes the name and returns it, or None when nothing was changed. Requested:
/// returns the suggestion without writing it.
pub async fn name_meeting<F, Fut>(pool: &SqlitePool, meeting_id: &str, trigger: Trigger, generate: F) -> Result<Option<String>, String>
where
    F: FnOnce(&'static str, String) -> Fut,
    Fut: Future<Output = Result<String, String>>,
{
    let settings = settings(pool).await.map_err(|e| e.to_string())?;
    let meeting: Option<(String, DateTime<Utc>, Option<String>)> = sqlx::query_as(
        "SELECT m.title, m.created_at, f.name FROM meetings m LEFT JOIN folders f ON f.id = m.folder_id WHERE m.id = ?",
    )
    .bind(meeting_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    let Some((title, created, folder)) = meeting else { return Err("The meeting was not found".to_string()) };
    if trigger == Trigger::Finalized && (!settings.auto_name || !is_default_title(&title)) {
        return Ok(None);
    }
    let transcript = transcript_text(pool, meeting_id).await.map_err(|e| e.to_string())?;
    if transcript.trim().is_empty() {
        return match trigger {
            Trigger::Finalized => Ok(None),
            Trigger::Requested => Err("There is no transcript to name this meeting from yet".to_string()),
        };
    }
    let notes = UserNotesRepository::get(pool, meeting_id).await.ok().flatten().map(|n| n.body_markdown);
    let reply = generate(TITLE_INSTRUCTIONS, title_prompt(&transcript, notes.as_deref())).await?;
    let Some(topic) = clean_topic(&reply) else {
        return match trigger {
            Trigger::Finalized => Ok(None),
            Trigger::Requested => Err("The model did not suggest a usable name. Try again.".to_string()),
        };
    };
    let local = created.with_timezone(&Local);
    let name = format_title(
        &settings.title_format,
        &TitleFields {
            start: (local.year(), local.month(), local.day(), local.hour(), local.minute()),
            topic: &topic,
            folder: folder.as_deref().unwrap_or(""),
        },
    );
    // A name typed while the model was working wins
    let changed = match trigger {
        Trigger::Finalized => sqlx::query("UPDATE meetings SET title = ?, updated_at = ? WHERE id = ? AND title = ?")
            .bind(&name)
            .bind(Utc::now())
            .bind(meeting_id)
            .bind(&title)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?
            .rows_affected()
            > 0,
        // A suggestion goes into the rename box; the person saves it
        Trigger::Requested => true,
    };
    Ok(changed.then_some(name))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Renamed {
    meeting_id: String,
    title: String,
}

/// One reply from the model chosen in Settings (also the model guide's speed test).
pub(crate) async fn complete_with_chosen_model<R: Runtime>(app: &AppHandle<R>, instructions: &str, prompt: &str, max_tokens: Option<u32>) -> Result<String, String> {
    let state = app.try_state::<AppState>().ok_or("The app is still starting")?;
    let pool = state.db_manager.pool().clone();
    let app_data_dir = app.path().app_data_dir().ok();
    let chosen = crate::ask::commands::chosen_model(&pool).await?.ok_or_else(|| "Choose a summary model in Settings".to_string())?;
    if let Some(problem) = chosen.problem(app_data_dir.as_ref()) {
        return Err(problem);
    }
    super::llm_client::generate_summary(
        &reqwest::Client::new(),
        &chosen.provider,
        &chosen.model,
        &chosen.api_key,
        instructions,
        prompt,
        chosen.ollama_endpoint.as_deref(),
        chosen.custom.as_ref().map(|c| c.endpoint.as_str()),
        max_tokens.or_else(|| chosen.custom.as_ref().and_then(|c| c.max_tokens).map(|t| t.max(1) as u32)),
        chosen.custom.as_ref().and_then(|c| c.temperature),
        chosen.custom.as_ref().and_then(|c| c.top_p),
        app_data_dir.as_ref(),
        None,
    )
    .await
    .map(|c| c.content)
}

/// Names `meeting_id` with the chosen model, then exports it and tells the window.
pub async fn name_with_chosen_model<R: Runtime>(app: &AppHandle<R>, meeting_id: &str, trigger: Trigger) -> Result<Option<String>, String> {
    let state = app.try_state::<AppState>().ok_or("The app is still starting")?;
    let pool = state.db_manager.pool().clone();
    // No length cap: a thinking model needs room before its answer, and only the first line is used
    let generate = |instructions: &'static str, prompt: String| async move { complete_with_chosen_model(app, instructions, &prompt, None).await };
    let named = name_meeting(&pool, meeting_id, trigger, generate).await?;
    if let (Some(title), Trigger::Finalized) = (&named, trigger) {
        // log-ok: the id only, never the name
        info!("Named meeting {}", meeting_id);
        crate::markdown_export::schedule(&pool, meeting_id, crate::markdown_export::CHANGE_DELAY);
        let _ = app.emit("meeting-renamed", Renamed { meeting_id: meeting_id.to_string(), title: title.clone() });
    }
    Ok(named)
}

/// The start of an error, up to its first colon: "LLM API request failed with status 500", never
/// the server's body.
pub(crate) fn error_kind(e: &str) -> &str {
    e.split(':').next().unwrap_or("").trim()
}

/// After a recording or import is finalized: name it in the background, quietly.
pub fn name_in_background<R: Runtime>(app: &AppHandle<R>, meeting_id: &str) {
    let (app, meeting_id) = (app.clone(), meeting_id.to_string());
    tauri::async_runtime::spawn(async move {
        if let Err(e) = name_with_chosen_model(&app, &meeting_id, Trigger::Finalized).await {
            // log-ok: the kind of error only; a server's error text can repeat the prompt
            warn!("Automatic name for meeting {} not made: {}", meeting_id, error_kind(&e));
        }
    });
}

#[tauri::command]
pub async fn api_naming_get_settings(state: tauri::State<'_, AppState>) -> Result<NamingSettings, String> {
    settings(state.db_manager.pool()).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn api_naming_set_settings(state: tauri::State<'_, AppState>, settings: NamingSettings) -> Result<NamingSettings, String> {
    let pool = state.db_manager.pool();
    set_settings(pool, &settings).await.map_err(|e| e.to_string())?;
    self::settings(pool).await.map_err(|e| e.to_string())
}

/// "Suggest a name" in the rename box: a name for the meeting, whatever its name is now.
#[tauri::command]
pub async fn api_suggest_meeting_name<R: Runtime>(app: AppHandle<R>, meeting_id: String) -> Result<String, String> {
    name_with_chosen_model(&app, &meeting_id, Trigger::Requested).await?.ok_or_else(|| "The meeting was not found".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::sync::atomic::{AtomicUsize, Ordering};

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    async fn meeting(pool: &SqlitePool, id: &str, title: &str, lines: &[&str]) {
        let created: DateTime<Utc> = Local.with_ymd_and_hms(2026, 11, 3, 14, 5, 0).unwrap().with_timezone(&Utc);
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES (?, ?, ?, ?)")
            .bind(id)
            .bind(title)
            .bind(created)
            .bind(created)
            .execute(pool)
            .await
            .unwrap();
        for (i, line) in lines.iter().enumerate() {
            sqlx::query("INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time) VALUES (?, ?, ?, ?, ?)")
                .bind(format!("{id}-{i}"))
                .bind(id)
                .bind(line)
                .bind(created)
                .bind(i as f64 * 5.0)
                .execute(pool)
                .await
                .unwrap();
        }
    }

    async fn title(pool: &SqlitePool, id: &str) -> String {
        sqlx::query_scalar("SELECT title FROM meetings WHERE id = ?").bind(id).fetch_one(pool).await.unwrap()
    }

    use chrono::TimeZone;

    fn reply(text: &'static str) -> impl FnOnce(&'static str, String) -> std::future::Ready<Result<String, String>> {
        move |_, _| std::future::ready(Ok(text.to_string()))
    }

    #[tokio::test]
    async fn settings_default_to_on_and_the_date_format() {
        let pool = pool().await;
        assert_eq!(settings(&pool).await.unwrap(), NamingSettings::default());
        let changed = NamingSettings { auto_name: false, title_format: " {topic} ".to_string() };
        set_settings(&pool, &changed).await.unwrap();
        assert_eq!(settings(&pool).await.unwrap(), NamingSettings { auto_name: false, title_format: "{topic}".to_string() });
    }

    #[tokio::test]
    async fn a_default_name_is_replaced_in_the_chosen_format() {
        let pool = pool().await;
        meeting(&pool, "m1", "Meeting 03_11_26_14_05_00", &["The roof leaks again.", "Three quotes came in."]).await;
        let seen = std::sync::Mutex::new(String::new());
        let named = name_meeting(&pool, "m1", Trigger::Finalized, |instructions, prompt| {
            assert_eq!(instructions, TITLE_INSTRUCTIONS);
            *seen.lock().unwrap() = prompt;
            std::future::ready(Ok("Title: Roof repair quotes.".to_string()))
        })
        .await
        .unwrap();
        assert_eq!(named.as_deref(), Some("2026-11-03 Roof repair quotes"));
        assert_eq!(title(&pool, "m1").await, "2026-11-03 Roof repair quotes");
        assert!(seen.lock().unwrap().contains("The roof leaks again.\nThree quotes came in."));
    }

    #[tokio::test]
    async fn a_typed_name_is_never_replaced_automatically() {
        let pool = pool().await;
        meeting(&pool, "m1", "Choir planning", &["We need more altos."]).await;
        let calls = AtomicUsize::new(0);
        let named = name_meeting(&pool, "m1", Trigger::Finalized, |_, _| {
            calls.fetch_add(1, Ordering::SeqCst);
            std::future::ready(Ok("Alto recruitment".to_string()))
        })
        .await
        .unwrap();
        assert_eq!(named, None);
        assert_eq!(calls.load(Ordering::SeqCst), 0, "no model call for a typed name");
        assert_eq!(title(&pool, "m1").await, "Choir planning");
    }

    #[tokio::test]
    async fn suggest_a_name_works_for_any_name() {
        let pool = pool().await;
        meeting(&pool, "m1", "Choir planning", &["We need more altos."]).await;
        let named = name_meeting(&pool, "m1", Trigger::Requested, reply("Alto recruitment")).await.unwrap();
        assert_eq!(named.as_deref(), Some("2026-11-03 Alto recruitment"));
        assert_eq!(title(&pool, "m1").await, "Choir planning", "a suggestion is only saved by the person");
    }

    #[tokio::test]
    async fn the_setting_off_stops_automatic_names_only() {
        let pool = pool().await;
        set_settings(&pool, &NamingSettings { auto_name: false, title_format: "{topic}".to_string() }).await.unwrap();
        meeting(&pool, "m1", "Meeting 03_11_26_14_05_00", &["The bus timetable changes."]).await;
        assert_eq!(name_meeting(&pool, "m1", Trigger::Finalized, reply("Bus timetable")).await.unwrap(), None);
        assert_eq!(name_meeting(&pool, "m1", Trigger::Requested, reply("Bus timetable")).await.unwrap().as_deref(), Some("Bus timetable"));
    }

    #[tokio::test]
    async fn a_name_typed_while_the_model_works_wins() {
        let pool = pool().await;
        meeting(&pool, "m1", "Meeting 03_11_26_14_05_00", &["The bus timetable changes."]).await;
        let p = pool.clone();
        let named = name_meeting(&pool, "m1", Trigger::Finalized, |_, _| async move {
            sqlx::query("UPDATE meetings SET title = 'Bus talk' WHERE id = 'm1'").execute(&p).await.unwrap();
            Ok("Bus timetable".to_string())
        })
        .await
        .unwrap();
        assert_eq!(named, None);
        assert_eq!(title(&pool, "m1").await, "Bus talk");
    }

    #[tokio::test]
    async fn an_unusable_reply_or_empty_transcript_changes_nothing() {
        let pool = pool().await;
        meeting(&pool, "m1", "Meeting 03_11_26_14_05_00", &["The bus timetable changes."]).await;
        assert_eq!(name_meeting(&pool, "m1", Trigger::Finalized, reply("...")).await.unwrap(), None);
        assert!(name_meeting(&pool, "m1", Trigger::Requested, reply("...")).await.is_err());
        meeting(&pool, "m2", "Meeting 03_11_26_14_05_00", &[]).await;
        assert_eq!(name_meeting(&pool, "m2", Trigger::Finalized, reply("Anything")).await.unwrap(), None);
        assert_eq!(title(&pool, "m1").await, "Meeting 03_11_26_14_05_00");
    }

    #[tokio::test]
    async fn the_folder_can_be_part_of_the_name() {
        let pool = pool().await;
        set_settings(&pool, &NamingSettings { auto_name: true, title_format: "{folder} - {topic}".to_string() }).await.unwrap();
        meeting(&pool, "m1", "Meeting 03_11_26_14_05_00", &["Seed swap on Saturday."]).await;
        sqlx::query("INSERT INTO folders (id, name, created_at, updated_at) VALUES ('f1', 'Garden Club', '2026-11-01', '2026-11-01')").execute(&pool).await.unwrap();
        sqlx::query("UPDATE meetings SET folder_id = 'f1' WHERE id = 'm1'").execute(&pool).await.unwrap();
        assert_eq!(name_meeting(&pool, "m1", Trigger::Finalized, reply("Seed swap")).await.unwrap().as_deref(), Some("Garden Club - Seed swap"));
    }
}
