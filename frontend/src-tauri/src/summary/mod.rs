/// Summary module - handles all meeting summary generation functionality
///
/// This module contains:
/// - LLM client for communicating with various AI providers (OpenAI, Claude, Groq, Ollama, OpenRouter, CustomOpenAI)
/// - Processor for chunking transcripts and generating summaries
/// - Service layer for orchestrating summary generation
/// - Templates for structured meeting summary generation
/// - Tauri commands for frontend integration

use serde::{Deserialize, Serialize};

/// Custom OpenAI-compatible endpoint configuration
/// Stored as JSON in the database and used for connecting to any OpenAI-compatible API server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomOpenAIConfig {
    /// Base URL of the OpenAI-compatible API endpoint (e.g., "http://localhost:8000/v1")
    pub endpoint: String,
    /// API key for authentication (optional if server doesn't require it)
    #[serde(rename = "apiKey")]
    pub api_key: Option<String>,
    /// Model identifier to use (e.g., "gpt-4", "llama-3-70b", "mistral-7b")
    pub model: String,
    /// Maximum tokens for completion (optional)
    #[serde(rename = "maxTokens")]
    pub max_tokens: Option<i32>,
    /// Temperature parameter (0.0-2.0, optional)
    pub temperature: Option<f32>,
    /// Top-P sampling parameter (0.0-1.0, optional)
    #[serde(rename = "topP")]
    pub top_p: Option<f32>,
    /// The server's context window in tokens (decision PD3). None means the 8,192 default,
    /// which is LM Studio's. Used to split long transcripts and to budget Ask.
    #[serde(rename = "contextSize", default)]
    pub context_size: Option<u32>,
}

/// Context window assumed for a custom server whose size was not set.
pub const DEFAULT_CUSTOM_CONTEXT_TOKENS: u32 = 8192;

/// How many transcript tokens fit in one request to a custom server: the window minus room for
/// the answer and for the prompts around the transcript. Never below 512.
pub fn custom_server_chunk_tokens(context_size: Option<u32>, max_tokens: Option<u32>) -> usize {
    const PROMPT_ROOM: usize = 1200;
    let window = context_size.unwrap_or(DEFAULT_CUSTOM_CONTEXT_TOKENS) as usize;
    let answer_room = (max_tokens.unwrap_or(2048) as usize).min(window / 2);
    window.saturating_sub(answer_room + PROMPT_ROOM).max(512)
}

#[cfg(test)]
mod context_tests {
    use super::*;

    #[test]
    fn the_default_window_leaves_room_for_the_answer_and_prompts() {
        assert_eq!(custom_server_chunk_tokens(None, None), 8192 - 2048 - 1200);
    }

    #[test]
    fn a_set_window_and_answer_size_are_used() {
        assert_eq!(custom_server_chunk_tokens(Some(32768), Some(4096)), 32768 - 4096 - 1200);
    }

    #[test]
    fn a_huge_answer_size_cannot_take_more_than_half_the_window() {
        assert_eq!(custom_server_chunk_tokens(Some(8192), Some(100_000)), 8192 - 4096 - 1200);
    }

    #[test]
    fn a_tiny_window_still_leaves_a_usable_chunk() {
        assert_eq!(custom_server_chunk_tokens(Some(1024), None), 512);
    }

    #[test]
    fn a_config_saved_before_context_size_existed_still_loads() {
        let config: CustomOpenAIConfig = serde_json::from_str(r#"{"endpoint":"http://localhost:1234/v1","apiKey":null,"model":"m","maxTokens":null,"temperature":null,"topP":null}"#).unwrap();
        assert_eq!(config.context_size, None);
    }
}

pub mod commands;
pub(crate) mod language_detection;
pub mod llm_client;
pub mod llm_stream;
pub(crate) mod metadata;
pub mod auto_name;
pub mod model_guide;
pub mod naming;
pub mod processor;
pub mod service;
pub mod summary_engine;
pub mod template_commands;
pub mod templates;

// Re-export Tauri commands (with their generated __cmd__ variants)
pub use commands::{
    __cmd__api_cancel_summary, __cmd__api_detect_transcript_summary_language,
    __cmd__api_get_meeting_detected_summary_language, __cmd__api_get_meeting_summary_language,
    __cmd__api_get_summary, __cmd__api_process_transcript,
    __cmd__api_save_meeting_detected_summary_language, __cmd__api_save_meeting_summary,
    __cmd__api_save_meeting_summary_language, __tauri_command_name_api_cancel_summary,
    __tauri_command_name_api_detect_transcript_summary_language,
    __tauri_command_name_api_get_meeting_detected_summary_language,
    __tauri_command_name_api_get_meeting_summary_language,
    __tauri_command_name_api_get_summary, __tauri_command_name_api_process_transcript,
    __tauri_command_name_api_save_meeting_detected_summary_language,
    __tauri_command_name_api_save_meeting_summary,
    __tauri_command_name_api_save_meeting_summary_language, api_cancel_summary,
    api_detect_transcript_summary_language, api_get_meeting_detected_summary_language,
    api_get_meeting_summary_language, api_get_summary, api_process_transcript,
    api_save_meeting_detected_summary_language, api_save_meeting_summary,
    api_save_meeting_summary_language,
};

// Re-export template commands
pub use template_commands::{
    __cmd__api_get_template_details, __cmd__api_list_templates, __cmd__api_validate_template,
    __tauri_command_name_api_get_template_details, __tauri_command_name_api_list_templates,
    __tauri_command_name_api_validate_template,
    api_get_template_details, api_list_templates, api_validate_template,
};

// Re-export commonly used items
pub use llm_client::LLMProvider;
pub use processor::{chunk_text, extract_meeting_name_from_markdown, rough_token_count};
pub use service::SummaryService;
