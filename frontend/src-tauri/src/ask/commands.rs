//! Tauri commands for Ask (spec 3.4). The answer streams through the `ask-stream` event and the
//! finished answer, with checked citations, is the command's result.

use super::citations::{check_citations, Citation};
use super::context::{answered_from, build_context, AskLine};
use super::policy::{
    backlog_refusal, download_is_complete, fit_budgets, is_on_this_computer, speaker_name, transcript_window,
    user_prompt, ASK_BACKLOG_LIMIT_SECONDS, SYSTEM_PROMPT,
};
use crate::audio::transcription::backlog;
use crate::database::repositories::{setting::SettingsRepository, user_notes::UserNotesRepository};
use crate::ollama::metadata::ModelMetadataCache;
use crate::state::AppState;
use crate::summary::llm_client::LLMProvider;
use crate::summary::llm_stream::{stream_completion, StreamRequest};
use crate::summary::summary_engine::models;
use crate::summary::CustomOpenAIConfig;
use log::{info, warn};
use serde::Serialize;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio_util::sync::CancellationToken;

/// Longest a question may be, in characters.
const MAX_QUESTION_CHARS: usize = 2000;
/// How long a model may stay silent before the answer is given up (prompt processing on a CPU is slow).
const IDLE_TIMEOUT: Duration = Duration::from_secs(180);

static RUNNING: LazyLock<Mutex<HashMap<String, CancellationToken>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
/// Keeps a request's cancel token registered from the start of the command until it returns,
/// whichever way it returns, so Stop works during setup too.
struct Registered(String);

impl Registered {
    fn new(request_id: &str, token: &CancellationToken) -> Self {
        RUNNING.lock().unwrap().insert(request_id.to_string(), token.clone());
        Registered(request_id.to_string())
    }
}

impl Drop for Registered {
    fn drop(&mut self) {
        RUNNING.lock().unwrap().remove(&self.0);
    }
}

static OLLAMA_METADATA: LazyLock<ModelMetadataCache> = LazyLock::new(|| ModelMetadataCache::new(Duration::from_secs(300)));

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AskChunk {
    pub request_id: String,
    pub text: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AskAnswer {
    pub answer: String,
    pub citations: Vec<Citation>,
    pub answered_from: String,
}

/// What the Ask box shows before a question: which model answers, whether the transcript leaves
/// this computer (F-QA-2), and what is missing when it cannot answer (F-QA-8).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AskReadiness {
    pub provider_label: String,
    pub model: String,
    pub local: bool,
    pub ready: bool,
    pub problem: Option<String>,
}

pub(crate) struct ChosenModel {
    pub(crate) provider: LLMProvider,
    pub(crate) model: String,
    pub(crate) api_key: String,
    pub(crate) ollama_endpoint: Option<String>,
    pub(crate) custom: Option<CustomOpenAIConfig>,
}

impl ChosenModel {
    fn endpoint(&self) -> Option<&str> {
        match self.provider {
            LLMProvider::Ollama => self.ollama_endpoint.as_deref(),
            LLMProvider::CustomOpenAI => self.custom.as_ref().map(|c| c.endpoint.as_str()),
            _ => None,
        }
    }

    fn label(&self) -> String {
        match self.provider {
            LLMProvider::BuiltInAI => "the built-in model".to_string(),
            LLMProvider::Ollama => "Ollama".to_string(),
            LLMProvider::OpenAI => "OpenAI".to_string(),
            LLMProvider::Claude => "Anthropic".to_string(),
            LLMProvider::Groq => "Groq".to_string(),
            LLMProvider::OpenRouter => "OpenRouter".to_string(),
            LLMProvider::CustomOpenAI => self
                .endpoint()
                .and_then(|e| url::Url::parse(e).ok())
                .and_then(|u| u.host_str().map(str::to_string))
                .unwrap_or_else(|| "the custom server".to_string()),
        }
    }

    /// Why this model cannot answer now, if it cannot.
    pub(crate) fn problem(&self, app_data_dir: Option<&PathBuf>) -> Option<String> {
        match self.provider {
            LLMProvider::BuiltInAI => {
                // A partial download is not installed (same size rule as the model list)
                let expected_mb = models::get_model_by_name(&self.model).map(|m| m.size_mb);
                let installed = app_data_dir
                    .and_then(|dir| models::get_model_path(dir, &self.model).ok())
                    .and_then(|path| std::fs::metadata(path).ok())
                    .zip(expected_mb)
                    .is_some_and(|(meta, mb)| download_is_complete(meta.len(), mb));
                (!installed).then(|| "Download a model in Settings".to_string())
            }
            LLMProvider::CustomOpenAI if self.custom.is_none() => Some("Set up the custom server in Settings".to_string()),
            LLMProvider::OpenAI | LLMProvider::Claude | LLMProvider::Groq | LLMProvider::OpenRouter if self.api_key.is_empty() => {
                Some("Add an API key in Settings".to_string())
            }
            _ => None,
        }
    }
}

pub(crate) async fn chosen_model(pool: &SqlitePool) -> Result<Option<ChosenModel>, String> {
    let Some(setting) = SettingsRepository::get_model_config(pool).await.map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let Ok(provider) = LLMProvider::from_str(&setting.provider) else {
        return Ok(None);
    };
    let custom = if provider == LLMProvider::CustomOpenAI {
        SettingsRepository::get_custom_openai_config(pool).await.map_err(|e| e.to_string())?
    } else {
        None
    };
    let api_key = match provider {
        LLMProvider::BuiltInAI | LLMProvider::Ollama => String::new(),
        LLMProvider::CustomOpenAI => custom.as_ref().and_then(|c| c.api_key.clone()).unwrap_or_default(),
        _ => SettingsRepository::get_api_key(pool, &setting.provider).await.map_err(|e| e.to_string())?.unwrap_or_default(),
    };
    let model = match &custom {
        Some(c) if !c.model.trim().is_empty() => c.model.clone(),
        _ => setting.model.clone(),
    };
    Ok(Some(ChosenModel { provider, model, api_key, ollama_endpoint: setting.ollama_endpoint.clone(), custom }))
}

/// Which model would answer, and whether it can.
#[tauri::command]
pub async fn api_ask_readiness<R: Runtime>(app: AppHandle<R>, state: tauri::State<'_, AppState>) -> Result<AskReadiness, String> {
    let app_data_dir = app.path().app_data_dir().ok();
    Ok(match chosen_model(state.db_manager.pool()).await? {
        None => AskReadiness {
            provider_label: String::new(),
            model: String::new(),
            local: true,
            ready: false,
            problem: Some("Choose a summary model in Settings".to_string()),
        },
        Some(chosen) => {
            let problem = chosen.problem(app_data_dir.as_ref());
            AskReadiness {
                provider_label: chosen.label(),
                model: chosen.model.clone(),
                local: is_on_this_computer(&chosen.provider, chosen.endpoint()),
                ready: problem.is_none(),
                problem,
            }
        }
    })
}

/// A saved meeting's lines, with speaker names as the transcript view shows them.
async fn saved_lines(pool: &SqlitePool, meeting_id: &str) -> Result<Vec<AskLine>, String> {
    let rows: Vec<(String, Option<f64>, Option<f64>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT t.transcript, t.audio_start_time, t.audio_end_time, t.speaker, p.name
         FROM transcripts t
         LEFT JOIN meeting_speakers ms ON ms.meeting_id = t.meeting_id AND ms.speaker_key = t.speaker
         LEFT JOIN people p ON p.id = ms.person_id
         WHERE t.meeting_id = ?",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .map_err(|e| {
        warn!("Ask: could not read the transcript: {}", e);
        "Could not read this meeting's transcript".to_string()
    })?;
    Ok(rows
        .into_iter()
        .map(|(text, start, end, key, person)| {
            let start = start.unwrap_or(0.0);
            AskLine { start, end: end.unwrap_or(start), speaker: speaker_name(key.as_deref(), person.as_deref()), text }
        })
        .collect())
}

/// The lines of the recording in progress. Live lines have no speaker yet.
async fn live_lines() -> Result<Vec<AskLine>, String> {
    let segments = crate::audio::recording_commands::get_transcript_history().await?;
    Ok(segments
        .into_iter()
        .map(|s| AskLine { start: s.audio_start_time, end: s.audio_end_time, speaker: None, text: s.text })
        .collect())
}

async fn model_context(chosen: &ChosenModel) -> Option<usize> {
    match chosen.provider {
        LLMProvider::BuiltInAI => models::get_model_by_name(&chosen.model).map(|m| m.context_size as usize),
        LLMProvider::Ollama => OLLAMA_METADATA.get_or_fetch(&chosen.model, chosen.ollama_endpoint.as_deref()).await.ok().map(|m| m.context_size),
        _ => None,
    }
}

/// Answer a question about a saved meeting (`meeting_id`) or the recording in progress (none).
/// During a recording the notes live in the window, so it passes them as `live_notes`.
#[tauri::command]
pub async fn api_ask_transcript<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    request_id: String,
    meeting_id: Option<String>,
    question: String,
    live_notes: Option<String>,
) -> Result<AskAnswer, String> {
    let question = question.trim().to_string();
    if question.is_empty() {
        return Err("Type a question first".to_string());
    }
    if question.chars().count() > MAX_QUESTION_CHARS {
        return Err(format!("Questions can be up to {} characters", MAX_QUESTION_CHARS));
    }
    // Transcription comes first (F-QA-5)
    if let Some(refusal) = backlog_refusal(backlog::pending_seconds(), ASK_BACKLOG_LIMIT_SECONDS) {
        return Err(refusal);
    }
    let cancel = CancellationToken::new();
    let _registered = Registered::new(&request_id, &cancel);

    let pool = state.db_manager.pool();
    let app_data_dir = app.path().app_data_dir().ok();
    let chosen = chosen_model(pool).await?.ok_or_else(|| "Choose a summary model in Settings".to_string())?;
    if let Some(problem) = chosen.problem(app_data_dir.as_ref()) {
        return Err(problem);
    }

    let (lines, notes) = match &meeting_id {
        Some(id) => {
            let notes = UserNotesRepository::get(pool, id).await.ok().flatten().map(|n| n.body_markdown);
            (saved_lines(pool, id).await?, notes)
        }
        None => (live_lines().await?, live_notes),
    };

    let custom_context = chosen.custom.as_ref().and_then(|c| c.context_size);
    let custom_max_tokens = chosen.custom.as_ref().and_then(|c| c.max_tokens).map(|t| t.max(1) as u32);
    let window = transcript_window(&chosen.provider, model_context(&chosen).await, custom_context, custom_max_tokens);
    let context = build_context(&lines, notes.as_deref(), &question, fit_budgets(window));
    if context.text.is_empty() {
        return Err("Nothing has been transcribed yet".to_string());
    }
    // log-ok: counts and a flag only
    info!(
        "Ask {}: {} lines supplied, {} earlier passages, notes {}",
        request_id,
        context.segments.len(),
        context.passages,
        context.notes_included
    );

    if cancel.is_cancelled() {
        return Err("Cancelled".to_string());
    }

    // Stop the answer if transcription falls well behind while it runs
    let fell_behind = Arc::new(AtomicBool::new(false));
    let watcher = {
        let (cancel, fell_behind) = (cancel.clone(), fell_behind.clone());
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                if backlog::pending_seconds() > ASK_BACKLOG_LIMIT_SECONDS * 2.0 {
                    fell_behind.store(true, Ordering::SeqCst);
                    cancel.cancel();
                    break;
                }
            }
        })
    };

    let prompt = user_prompt(&context.text, &question);
    let request = StreamRequest {
        provider: &chosen.provider,
        model_name: &chosen.model,
        api_key: &chosen.api_key,
        system_prompt: SYSTEM_PROMPT,
        user_prompt: &prompt,
        ollama_endpoint: chosen.ollama_endpoint.as_deref(),
        custom_openai_endpoint: chosen.custom.as_ref().map(|c| c.endpoint.as_str()),
        max_tokens: custom_max_tokens,
        temperature: chosen.custom.as_ref().and_then(|c| c.temperature),
        top_p: chosen.custom.as_ref().and_then(|c| c.top_p),
        app_data_dir: app_data_dir.as_ref(),
    };
    let client = reqwest::Client::new();
    let emitter = app.clone();
    let id_for_events = request_id.clone();
    // Timings for the benchmark (AC-QA-3): first streamed text and total, with the backlog
    let started = Instant::now();
    let backlog_at_start = backlog::pending_seconds();
    let first_text_ms = Arc::new(AtomicU64::new(0));
    let first_text = first_text_ms.clone();
    let mut on_text = move |text: &str| {
        let _ = first_text.compare_exchange(0, started.elapsed().as_millis().max(1) as u64, Ordering::SeqCst, Ordering::SeqCst);
        let _ = emitter.emit("ask-stream", AskChunk { request_id: id_for_events.clone(), text: text.to_string() });
    };
    let result = stream_completion(&client, &request, &cancel, IDLE_TIMEOUT, &mut on_text).await;
    // log-ok: timings only
    info!(
        "Ask {}: first text after {} ms, total {} ms, backlog {:.1} s at start and {:.1} s at end",
        request_id,
        first_text_ms.load(Ordering::SeqCst),
        started.elapsed().as_millis(),
        backlog_at_start,
        backlog::pending_seconds()
    );

    watcher.abort();

    let answer = match result {
        Ok(answer) => answer,
        Err(_) if fell_behind.load(Ordering::SeqCst) => {
            return Err("Stopped so transcription can catch up. Ask again in a moment.".to_string())
        }
        Err(_) if cancel.is_cancelled() => return Err("Cancelled".to_string()),
        Err(e) => {
            // Only the start of the message: a server's error body can echo the prompt
            let kind: String = e.split(':').next().unwrap_or("").chars().take(80).collect();
            warn!("Ask {} failed: {}", request_id, kind);
            return Err(e);
        }
    };

    let (answer, citations) = check_citations(answer.trim(), &context.segments);
    let meeting_end = lines.iter().map(|l| l.end).fold(0.0, f64::max);
    Ok(AskAnswer { answer, citations, answered_from: answered_from(&context, meeting_end) })
}

/// Cancel a running answer. Returns false when it already finished.
#[tauri::command]
pub async fn api_cancel_ask(request_id: String) -> Result<bool, String> {
    Ok(match RUNNING.lock().unwrap().get(&request_id) {
        Some(token) => {
            token.cancel();
            true
        }
        None => false,
    })
}
