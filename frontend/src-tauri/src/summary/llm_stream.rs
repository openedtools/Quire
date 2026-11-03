//! Streamed answers for Ask (plan Phase D, D2, decision PD2).
//!
//! HTTP providers (OpenAI, Groq, OpenRouter, Ollama, a custom OpenAI-compatible server such as
//! LM Studio, and Claude) send the answer as server-sent events, read here as they arrive. The
//! built-in model answers in one piece. A stream that goes quiet for longer than the idle
//! timeout is abandoned, instead of one timeout for the whole answer.

use super::llm_client::{build_openai_compat_chat_body, provider_endpoint, LLMProvider};
use futures_util::StreamExt;
use std::path::PathBuf;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// One server-sent event's meaning for the answer.
#[derive(Debug, PartialEq, Eq)]
pub enum StreamEvent {
    Text(String),
    Done,
    /// The server reported an error inside the stream.
    Error(String),
    /// Role markers, hidden reasoning, keep-alives and anything unreadable.
    Ignore,
}

/// Collects bytes into complete `data:` payloads, across network chunk boundaries.
#[derive(Default)]
pub struct SseBuffer {
    pending: Vec<u8>,
}

impl SseBuffer {
    /// Add received text; returns the `data:` payloads of every line now complete.
    pub fn push(&mut self, chunk: &str) -> Vec<String> {
        self.push_bytes(chunk.as_bytes())
    }

    /// Add received bytes. A character split across two network chunks is decoded whole.
    pub fn push_bytes(&mut self, chunk: &[u8]) -> Vec<String> {
        self.pending.extend_from_slice(chunk);
        let mut payloads = Vec::new();
        while let Some(end) = self.pending.iter().position(|b| *b == b'\n') {
            let bytes: Vec<u8> = self.pending.drain(..=end).collect();
            let line = String::from_utf8_lossy(&bytes);
            let line = line.trim_end_matches(['\n', '\r']);
            if let Some(data) = line.strip_prefix("data:") {
                payloads.push(data.strip_prefix(' ').unwrap_or(data).to_string());
            }
        }
        payloads
    }
}

/// An OpenAI-compatible stream chunk (OpenAI, Groq, OpenRouter, Ollama, LM Studio).
pub fn openai_event(data: &str) -> StreamEvent {
    if data.trim() == "[DONE]" {
        return StreamEvent::Done;
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(data) else {
        return StreamEvent::Ignore;
    };
    if let Some(error) = value.get("error") {
        return StreamEvent::Error(error_message(error));
    }
    match value.pointer("/choices/0/delta/content").and_then(|c| c.as_str()) {
        Some(text) if !text.is_empty() => StreamEvent::Text(text.to_string()),
        _ => StreamEvent::Ignore,
    }
}

/// An Anthropic Messages stream event.
pub fn claude_event(data: &str) -> StreamEvent {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(data) else {
        return StreamEvent::Ignore;
    };
    match value.get("type").and_then(|t| t.as_str()) {
        Some("message_stop") => StreamEvent::Done,
        Some("error") => StreamEvent::Error(value.get("error").map(error_message).unwrap_or_else(|| "unknown error".to_string())),
        Some("content_block_delta") => match value.pointer("/delta/text").and_then(|t| t.as_str()) {
            Some(text) if !text.is_empty() && value.pointer("/delta/type").and_then(|t| t.as_str()) == Some("text_delta") => {
                StreamEvent::Text(text.to_string())
            }
            _ => StreamEvent::Ignore,
        },
        _ => StreamEvent::Ignore,
    }
}

/// The message of an error object, or the value itself when it is plain text.
fn error_message(error: &serde_json::Value) -> String {
    error
        .get("message")
        .and_then(|m| m.as_str())
        .or_else(|| error.as_str())
        .unwrap_or("unknown error")
        .chars()
        .take(300)
        .collect()
}

/// True when an OpenAI-compatible chunk says the answer is finished, for servers that end the
/// stream without `[DONE]`.
fn openai_finished(data: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(data)
        .ok()
        .and_then(|v| v.pointer("/choices/0/finish_reason").and_then(|r| r.as_str()).map(|r| !r.is_empty()))
        .unwrap_or(false)
}

/// Everything a streamed request needs, so callers pass one value.
pub struct StreamRequest<'a> {
    pub provider: &'a LLMProvider,
    pub model_name: &'a str,
    pub api_key: &'a str,
    pub system_prompt: &'a str,
    pub user_prompt: &'a str,
    pub ollama_endpoint: Option<&'a str>,
    pub custom_openai_endpoint: Option<&'a str>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub app_data_dir: Option<&'a PathBuf>,
}

/// Stream an answer, calling `on_text` with each piece as it arrives. Returns the whole answer.
pub async fn stream_completion(
    client: &reqwest::Client,
    request: &StreamRequest<'_>,
    cancel: &CancellationToken,
    idle_timeout: Duration,
    on_text: &mut (dyn FnMut(&str) + Send),
) -> Result<String, String> {
    if *request.provider == LLMProvider::BuiltInAI {
        let dir = request.app_data_dir.ok_or_else(|| "The app data folder is not available".to_string())?;
        let answer = super::summary_engine::generate_with_builtin(
            dir,
            request.model_name,
            request.system_prompt,
            request.user_prompt,
            Some(cancel),
        )
        .await
        .map_err(|e| e.to_string())?;
        on_text(&answer);
        return Ok(answer);
    }

    let (url, headers) = provider_endpoint(request.provider, request.api_key, request.ollama_endpoint, request.custom_openai_endpoint)?;
    let is_claude = *request.provider == LLMProvider::Claude;
    let body = if is_claude {
        serde_json::json!({
            "model": request.model_name,
            "max_tokens": 4096,
            "system": request.system_prompt,
            "stream": true,
            "messages": [{ "role": "user", "content": request.user_prompt }],
        })
    } else {
        let mut body = build_openai_compat_chat_body(
            request.provider,
            request.model_name,
            request.system_prompt,
            request.user_prompt,
            request.max_tokens,
            request.temperature,
            request.top_p,
        );
        body["stream"] = serde_json::Value::Bool(true);
        body
    };

    // net: Ask with the chosen provider: cloud needs the switch; Ollama or a custom server at its address
    let send = client.post(url).headers(headers).json(&body).send();
    let response = tokio::select! {
        _ = cancel.cancelled() => return Err("Answer cancelled".to_string()),
        r = tokio::time::timeout(idle_timeout, send) => match r {
            Err(_) => return Err("The model stopped responding".to_string()),
            Ok(Err(e)) => return Err(format!("Could not reach the model: {}", e)),
            Ok(Ok(r)) => r,
        },
    };
    let status = response.status();
    if !status.is_success() {
        // The error body gets the same cancel and idle timeout as the answer
        let detail = tokio::select! {
            _ = cancel.cancelled() => return Err("Answer cancelled".to_string()),
            d = tokio::time::timeout(idle_timeout, response.text()) => d.ok().and_then(Result::ok).unwrap_or_default(),
        };
        return Err(format!("The model returned {}: {}", status, detail.chars().take(300).collect::<String>()));
    }

    let mut bytes = response.bytes_stream();
    let mut buffer = SseBuffer::default();
    let mut answer = String::new();
    let mut finished = false;
    loop {
        let next = tokio::select! {
            _ = cancel.cancelled() => return Err("Answer cancelled".to_string()),
            n = tokio::time::timeout(idle_timeout, bytes.next()) => match n {
                Err(_) => return Err("The model stopped responding".to_string()),
                Ok(n) => n,
            },
        };
        let Some(chunk) = next else { break };
        let chunk = chunk.map_err(|e| format!("The answer was interrupted: {}", e))?;
        for data in buffer.push_bytes(&chunk) {
            let event = if is_claude { claude_event(&data) } else { openai_event(&data) };
            if !is_claude && openai_finished(&data) {
                finished = true;
            }
            match event {
                StreamEvent::Text(text) => {
                    answer.push_str(&text);
                    on_text(&text);
                }
                StreamEvent::Done => return Ok(answer),
                StreamEvent::Error(message) => return Err(format!("The model reported an error: {}", message)),
                StreamEvent::Ignore => {}
            }
        }
    }
    if finished {
        Ok(answer)
    } else {
        Err("The answer was cut off before it finished. Try again.".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn sse_lines_are_split_across_chunks_and_line_endings() {
        let mut buf = SseBuffer::default();
        assert_eq!(buf.push("data: {\"a\""), Vec::<String>::new());
        assert_eq!(buf.push(":1}\r\n\r\nevent: x\ndata: two\n: keep-alive\ndata:three\n"), vec!["{\"a\":1}", "two", "three"]);
        assert_eq!(buf.push("data: [DONE]"), Vec::<String>::new());
        assert_eq!(buf.push("\n"), vec!["[DONE]"]);
    }

    #[test]
    fn openai_chunks_give_text_and_skip_reasoning() {
        assert_eq!(openai_event(r#"{"choices":[{"delta":{"content":"Hel"}}]}"#), StreamEvent::Text("Hel".into()));
        assert_eq!(openai_event(r#"{"choices":[{"delta":{"role":"assistant"}}]}"#), StreamEvent::Ignore);
        assert_eq!(openai_event(r#"{"choices":[{"delta":{"reasoning_content":"thinking"}}]}"#), StreamEvent::Ignore);
        assert_eq!(openai_event(r#"{"choices":[{"delta":{"content":""}}]}"#), StreamEvent::Ignore);
        assert_eq!(openai_event("[DONE]"), StreamEvent::Done);
        assert_eq!(openai_event("not json"), StreamEvent::Ignore);
    }

    #[test]
    fn claude_events_give_text_and_skip_thinking() {
        assert_eq!(
            claude_event(r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hi"}}"#),
            StreamEvent::Text("Hi".into())
        );
        assert_eq!(
            claude_event(r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"hm"}}"#),
            StreamEvent::Ignore
        );
        assert_eq!(claude_event(r#"{"type":"message_stop"}"#), StreamEvent::Done);
        assert_eq!(claude_event(r#"{"type":"ping"}"#), StreamEvent::Ignore);
    }

    /// A server that answers one request with `body` written in `parts`, pausing `gap` between them.
    async fn sse_server(parts: Vec<&'static str>, gap: Duration) -> (String, tokio::task::JoinHandle<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = vec![0_u8; 65536];
            let n = stream.read(&mut request).await.unwrap();
            request.truncate(n);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\n\r\n")
                .await
                .unwrap();
            for part in parts {
                let chunk = format!("{:x}\r\n{}\r\n", part.len(), part);
                if stream.write_all(chunk.as_bytes()).await.is_err() {
                    break;
                }
                tokio::time::sleep(gap).await;
            }
            let _ = stream.write_all(b"0\r\n\r\n").await;
            request
        });
        (format!("http://{}", addr), handle)
    }

    fn custom_request<'a>(endpoint: &'a str) -> StreamRequest<'a> {
        StreamRequest {
            provider: &LLMProvider::CustomOpenAI,
            model_name: "m",
            api_key: "",
            system_prompt: "sys",
            user_prompt: "question",
            ollama_endpoint: None,
            custom_openai_endpoint: Some(endpoint),
            max_tokens: None,
            temperature: None,
            top_p: None,
            app_data_dir: None,
        }
    }

    #[tokio::test]
    async fn a_custom_server_streams_the_answer_piece_by_piece() {
        let (url, server) = sse_server(
            vec![
                "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\"You missed \"}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\"the budget.\"}}]}\n\ndata: [DONE]\n\n",
            ],
            Duration::from_millis(5),
        )
        .await;
        let mut pieces = Vec::new();
        let answer = stream_completion(
            &reqwest::Client::new(),
            &custom_request(&url),
            &CancellationToken::new(),
            Duration::from_secs(5),
            &mut |t| pieces.push(t.to_string()),
        )
        .await
        .unwrap();
        assert_eq!(answer, "You missed the budget.");
        assert_eq!(pieces, ["You missed ", "the budget."]);
        let request = String::from_utf8(server.await.unwrap()).unwrap();
        assert!(request.contains("\"stream\":true"), "{request}");
    }

    #[tokio::test]
    async fn a_server_that_goes_quiet_hits_the_idle_timeout() {
        let (url, _server) = sse_server(
            vec!["data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n", "data: [DONE]\n\n"],
            Duration::from_secs(3),
        )
        .await;
        let started = std::time::Instant::now();
        let err = stream_completion(
            &reqwest::Client::new(),
            &custom_request(&url),
            &CancellationToken::new(),
            Duration::from_millis(300),
            &mut |_| {},
        )
        .await
        .unwrap_err();
        assert!(err.contains("stopped responding"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[tokio::test]
    async fn cancelling_stops_the_stream_promptly() {
        let (url, _server) = sse_server(
            vec!["data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n", "data: [DONE]\n\n"],
            Duration::from_secs(3),
        )
        .await;
        let cancel = CancellationToken::new();
        let c2 = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            c2.cancel();
        });
        let started = std::time::Instant::now();
        let err = stream_completion(&reqwest::Client::new(), &custom_request(&url), &cancel, Duration::from_secs(10), &mut |_| {})
            .await
            .unwrap_err();
        assert!(err.contains("cancelled"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}

#[cfg(test)]
mod review_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn a_character_split_across_chunks_is_kept_whole() {
        let mut buf = SseBuffer::default();
        let line = "data: Привет\n".as_bytes();
        let cut = 8; // inside the first Cyrillic letter
        assert_eq!(buf.push_bytes(&line[..cut]), Vec::<String>::new());
        assert_eq!(buf.push_bytes(&line[cut..]), vec!["Привет"]);
    }

    #[test]
    fn error_events_are_reported() {
        assert_eq!(
            openai_event(r#"{"error":{"message":"context length exceeded"}}"#),
            StreamEvent::Error("context length exceeded".into())
        );
        assert_eq!(
            claude_event(r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#),
            StreamEvent::Error("Overloaded".into())
        );
    }

    async fn server(parts: Vec<&'static str>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = vec![0_u8; 65536];
            let _ = stream.read(&mut request).await;
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\n\r\n")
                .await;
            for part in parts {
                let _ = stream.write_all(format!("{:x}\r\n{}\r\n", part.len(), part).as_bytes()).await;
            }
            let _ = stream.write_all(b"0\r\n\r\n").await;
        });
        format!("http://{}", addr)
    }

    fn request(endpoint: &str) -> StreamRequest<'_> {
        StreamRequest {
            provider: &LLMProvider::CustomOpenAI,
            model_name: "m",
            api_key: "",
            system_prompt: "sys",
            user_prompt: "q",
            ollama_endpoint: None,
            custom_openai_endpoint: Some(endpoint),
            max_tokens: None,
            temperature: None,
            top_p: None,
            app_data_dir: None,
        }
    }

    async fn run(url: &str) -> Result<String, String> {
        stream_completion(&reqwest::Client::new(), &request(url), &CancellationToken::new(), Duration::from_secs(5), &mut |_| {}).await
    }

    #[tokio::test]
    async fn a_stream_that_ends_without_finishing_is_an_error() {
        let url = server(vec!["data: {\"choices\":[{\"delta\":{\"content\":\"Half an ans\"}}]}\n\n"]).await;
        let err = run(&url).await.unwrap_err();
        assert!(err.contains("cut off"), "{err}");
    }

    #[tokio::test]
    async fn a_finish_reason_without_done_still_completes() {
        let url = server(vec![
            "data: {\"choices\":[{\"delta\":{\"content\":\"All of it.\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
        ])
        .await;
        assert_eq!(run(&url).await.unwrap(), "All of it.");
    }

    #[tokio::test]
    async fn an_error_inside_the_stream_fails_the_answer() {
        let url = server(vec!["data: {\"error\":{\"message\":\"context length exceeded\"}}\n\n"]).await;
        let err = run(&url).await.unwrap_err();
        assert!(err.contains("context length exceeded"), "{err}");
    }
}
