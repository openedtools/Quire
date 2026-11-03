//! Ask benchmark (spec AC-QA-3, plan D4). Ignored by default; run it by hand against a model:
//!
//! ```text
//! set ASK_BENCH_PROVIDER=custom-openai        (or ollama)
//! set ASK_BENCH_ENDPOINT=http://localhost:1234/v1
//! set ASK_BENCH_MODEL=<model id>
//! set ASK_BENCH_CONTEXT=8192                  (the server's context size, optional)
//! cargo test --lib ask::bench -- --ignored --nocapture
//! ```
//!
//! For each of a 30, 60 and 120 minute synthetic meeting it builds the Ask context with the
//! production budgets and reports the context size, the time to the first streamed text and the
//! total answer time. Start a recording in the app at the same time to see the transcription
//! backlog in the app log ("Ask ... backlog").

#[cfg(test)]
mod tests {
    use crate::ask::context::{build_context, AskLine};
    use crate::ask::policy::{fit_budgets, transcript_window, user_prompt, SYSTEM_PROMPT};
    use crate::summary::llm_client::LLMProvider;
    use crate::summary::llm_stream::{stream_completion, StreamRequest};
    use crate::summary::processor::rough_token_count;
    use std::time::{Duration, Instant};
    use tokio_util::sync::CancellationToken;

    fn meeting(minutes: usize) -> Vec<AskLine> {
        let topics = ["budget", "venue", "schedule", "staffing", "training", "travel", "equipment", "risk"];
        (0..minutes * 6)
            .map(|i| {
                let topic = topics[(i / 7) % topics.len()];
                AskLine {
                    start: i as f64 * 10.0,
                    end: i as f64 * 10.0 + 9.0,
                    speaker: Some(format!("Speaker {}", i % 4 + 1)),
                    text: format!("On the {topic} item, point {i}: we reviewed the numbers and agreed to follow up next week with the team."),
                }
            })
            .collect()
    }

    #[tokio::test]
    #[ignore = "needs a running model; see the module notes"]
    async fn ask_benchmark() {
        let provider = LLMProvider::from_str(&std::env::var("ASK_BENCH_PROVIDER").unwrap_or_else(|_| "custom-openai".into())).unwrap();
        let endpoint = std::env::var("ASK_BENCH_ENDPOINT").ok();
        let model = std::env::var("ASK_BENCH_MODEL").expect("set ASK_BENCH_MODEL");
        let context_size = std::env::var("ASK_BENCH_CONTEXT").ok().and_then(|v| v.parse().ok());
        let window = transcript_window(&provider, context_size.map(|c: u32| c as usize), context_size, None);
        let budgets = fit_budgets(window);
        let client = reqwest::Client::new();
        println!("provider {:?}, model {}, budgets {:?}", provider, model, budgets);
        println!("| Meeting | Context tokens | First text | Total |");
        println!("|---|---|---|---|");
        for minutes in [30, 60, 120] {
            let context = build_context(&meeting(minutes), Some("- Ask about the venue deposit"), "What was agreed about the venue?", budgets);
            let prompt = user_prompt(&context.text, "What was agreed about the venue?");
            let request = StreamRequest {
                provider: &provider,
                model_name: &model,
                api_key: "",
                system_prompt: SYSTEM_PROMPT,
                user_prompt: &prompt,
                ollama_endpoint: endpoint.as_deref(),
                custom_openai_endpoint: endpoint.as_deref(),
                max_tokens: Some(300),
                temperature: None,
                top_p: None,
                app_data_dir: None,
            };
            let started = Instant::now();
            let mut first: Option<Duration> = None;
            let mut on_text = |_: &str| {
                if first.is_none() {
                    first = Some(started.elapsed());
                }
            };
            let result = stream_completion(&client, &request, &CancellationToken::new(), Duration::from_secs(600), &mut on_text).await;
            let total = started.elapsed();
            match result {
                Ok(_) => println!(
                    "| {} min | {} | {:.1} s | {:.1} s |",
                    minutes,
                    rough_token_count(&context.text),
                    first.unwrap_or(total).as_secs_f64(),
                    total.as_secs_f64()
                ),
                Err(e) => println!("| {} min | {} | failed: {} | |", minutes, rough_token_count(&context.text), e),
            }
        }
    }
}
