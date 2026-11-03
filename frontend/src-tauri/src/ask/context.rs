//! The context an Ask question is answered from (spec 3.4, F-QA-3, F-NOTE-5).
//!
//! Pure and deterministic: the person's notes first, then the best keyword (BM25) matches from
//! earlier in the meeting, then the newest lines. Every line carries its time and, when known,
//! its speaker, so the model can cite `[mm:ss]` and the app can check the citations.

use crate::summary::processor::rough_token_count;
use std::cmp::Ordering;
use std::collections::HashMap;

/// One transcript line, in seconds from the start of the recording.
#[derive(Debug, Clone, PartialEq)]
pub struct AskLine {
    pub start: f64,
    pub end: f64,
    pub speaker: Option<String>,
    pub text: String,
}

/// Token budgets for the two parts of the transcript (F-QA-3). Notes come out of the recent
/// budget and take at most half of it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Budgets {
    pub recent_tokens: usize,
    pub retrieval_tokens: usize,
}

/// Starting values from the spec, tuned after the benchmark (AC-QA-3).
pub const DEFAULT_BUDGETS: Budgets = Budgets { recent_tokens: 8000, retrieval_tokens: 3000 };

/// Room for the section headings the builder adds around the lines.
pub const HEADING_TOKENS: usize = 40;

/// A supplied line's time range, used to check citations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AskContext {
    /// The block that goes into the user prompt.
    pub text: String,
    /// The time ranges of every transcript line in `text`.
    pub segments: Vec<Segment>,
    pub notes_included: bool,
    /// Start of the oldest line in the recent part, when there is one.
    pub recent_from: Option<f64>,
    /// How many earlier lines were picked by keyword match.
    pub passages: usize,
    /// True when every transcript line fit.
    pub whole_transcript: bool,
}

/// `mm:ss`, or `h:mm:ss` from one hour on. Seconds are rounded down, as on screen.
pub fn format_time(seconds: f64) -> String {
    let total = if seconds.is_finite() && seconds > 0.0 { seconds.floor() as u64 } else { 0 };
    let (hours, minutes, secs) = (total / 3600, (total % 3600) / 60, total % 60);
    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, minutes, secs)
    } else {
        format!("{:02}:{:02}", minutes, secs)
    }
}

/// One line as the model sees it: `[mm:ss] Speaker: text`. Line breaks inside the text are
/// flattened so a line can never pose as a heading.
fn format_line(line: &AskLine) -> String {
    let text = line.text.split_whitespace().collect::<Vec<_>>().join(" ");
    match line.speaker.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(speaker) => format!("[{}] {}: {}\n", format_time(line.start), speaker, text),
        None => format!("[{}] {}\n", format_time(line.start), text),
    }
}

/// Cut `text` to about `tokens` tokens, at a word boundary.
fn truncate_to_tokens(text: &str, tokens: usize) -> String {
    if rough_token_count(text) <= tokens {
        return text.to_string();
    }
    let max_chars = (tokens as f64 / 0.35).floor() as usize;
    let cut: String = text.chars().take(max_chars).collect();
    match cut.rfind(char::is_whitespace) {
        Some(at) => cut[..at].trim_end().to_string(),
        None => cut,
    }
}

const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "but", "by", "did", "do", "does", "for", "from", "had",
    "has", "have", "how", "i", "in", "is", "it", "its", "me", "my", "of", "on", "or", "so", "that",
    "the", "their", "then", "there", "they", "this", "to", "was", "we", "were", "what", "when",
    "where", "which", "who", "why", "will", "with", "you", "your",
];

fn terms(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|w| w.chars().count() >= 2 && !STOPWORDS.contains(&w.as_str()))
        .collect()
}

/// Okapi BM25 score of each line for the question (k1 1.2, b 0.75).
fn bm25_scores(lines: &[&AskLine], question: &str) -> Vec<f64> {
    let mut query = terms(question);
    query.sort();
    query.dedup();
    let docs: Vec<Vec<String>> = lines.iter().map(|l| terms(&l.text)).collect();
    if query.is_empty() || docs.is_empty() {
        return vec![0.0; lines.len()];
    }
    let n = docs.len() as f64;
    let average = (docs.iter().map(Vec::len).sum::<usize>() as f64 / n).max(1.0);
    let idf: HashMap<&str, f64> = query
        .iter()
        .map(|term| {
            let df = docs.iter().filter(|d| d.contains(term)).count() as f64;
            (term.as_str(), ((n - df + 0.5) / (df + 0.5) + 1.0).ln())
        })
        .collect();
    docs.iter()
        .map(|doc| {
            let norm = 1.0 - 0.75 + 0.75 * doc.len() as f64 / average;
            query
                .iter()
                .map(|term| {
                    let tf = doc.iter().filter(|w| *w == term).count() as f64;
                    if tf == 0.0 { 0.0 } else { idf[term.as_str()] * tf * 2.2 / (tf + 1.2 * norm) }
                })
                .sum()
        })
        .collect()
}

/// Build the context for `question` from `lines` (any order) and the person's notes.
pub fn build_context(lines: &[AskLine], notes: Option<&str>, question: &str, budgets: Budgets) -> AskContext {
    let mut sorted: Vec<&AskLine> = lines.iter().filter(|l| !l.text.trim().is_empty()).collect();
    sorted.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(Ordering::Equal).then_with(|| a.text.cmp(&b.text)));

    let mut context = AskContext::default();
    let mut recent_left = budgets.recent_tokens;

    // Notes first (F-NOTE-5), at most half the recent budget
    if let Some(body) = notes.map(str::trim).filter(|n| !n.is_empty()) {
        const LABEL: &str = "[your notes]\n";
        let room = (budgets.recent_tokens / 2).saturating_sub(rough_token_count(LABEL) + 1);
        let block = format!("{}{}\n\n", LABEL, truncate_to_tokens(body, room));
        recent_left = recent_left.saturating_sub(rough_token_count(&block));
        context.text.push_str(&block);
        context.notes_included = true;
    }

    // The newest lines that fit
    let formatted: Vec<String> = sorted.iter().map(|l| format_line(l)).collect();
    let mut first_recent = sorted.len();
    while first_recent > 0 {
        let cost = rough_token_count(&formatted[first_recent - 1]);
        if cost > recent_left {
            break;
        }
        recent_left -= cost;
        first_recent -= 1;
    }
    context.whole_transcript = first_recent == 0 && !sorted.is_empty();

    // The best keyword matches from before them
    let earlier = &sorted[..first_recent];
    let scores = bm25_scores(earlier, question);
    let mut ranked: Vec<usize> = (0..earlier.len()).filter(|&i| scores[i] > 0.0).collect();
    ranked.sort_by(|&a, &b| scores[b].partial_cmp(&scores[a]).unwrap_or(Ordering::Equal).then(a.cmp(&b)));
    let mut retrieval_left = budgets.retrieval_tokens;
    let mut picked = Vec::new();
    for i in ranked {
        let cost = rough_token_count(&formatted[i]);
        if cost <= retrieval_left {
            retrieval_left -= cost;
            picked.push(i);
        }
    }
    picked.sort_unstable();
    context.passages = picked.len();

    if !picked.is_empty() {
        context.text.push_str("Earlier passages:\n");
        for &i in &picked {
            context.text.push_str(&formatted[i]);
            context.segments.push(Segment { start: sorted[i].start, end: sorted[i].end });
        }
        context.text.push('\n');
    }
    if first_recent < sorted.len() {
        context.text.push_str(if context.whole_transcript { "Transcript:\n" } else { "Most recent lines:\n" });
        for i in first_recent..sorted.len() {
            context.text.push_str(&formatted[i]);
            context.segments.push(Segment { start: sorted[i].start, end: sorted[i].end });
        }
        context.recent_from = Some(sorted[first_recent].start);
    }
    context
}

/// The "answered from" line shown under an answer (F-QA-3).
pub fn answered_from(context: &AskContext, meeting_end: f64) -> String {
    if context.whole_transcript {
        return "Answered from the whole transcript".to_string();
    }
    let Some(from) = context.recent_from else {
        return if context.notes_included { "Answered from your notes".to_string() } else { String::new() };
    };
    let minutes = (((meeting_end - from) / 60.0).round() as i64).max(1);
    let mut line = format!("Answered from the last {} minute{}", minutes, if minutes == 1 { "" } else { "s" });
    if context.passages > 0 {
        let plural = if context.passages == 1 { "" } else { "s" };
        line.push_str(&format!(" and {} earlier passage{}", context.passages, plural));
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(start: f64, speaker: Option<&str>, text: &str) -> AskLine {
        AskLine { start, end: start + 9.0, speaker: speaker.map(str::to_string), text: text.to_string() }
    }

    /// Two hours of ten-second lines, with one line about a rare topic near the start.
    fn two_hour_meeting() -> Vec<AskLine> {
        (0..720)
            .map(|i| {
                let text = if i == 30 {
                    "We agreed the Kubernetes migration slips to March".to_string()
                } else {
                    format!("Routine status update number {} about the schedule and the budget review", i)
                };
                line(i as f64 * 10.0, Some("Prof Smith"), &text)
            })
            .collect()
    }

    #[test]
    fn times_are_minutes_and_seconds_then_hours() {
        assert_eq!(format_time(0.0), "00:00");
        assert_eq!(format_time(65.9), "01:05");
        assert_eq!(format_time(3723.0), "1:02:03");
    }

    #[test]
    fn the_context_never_exceeds_the_budget() {
        let budgets = Budgets { recent_tokens: 2000, retrieval_tokens: 500 };
        let context = build_context(&two_hour_meeting(), Some("Ask about the migration"), "kubernetes?", budgets);
        let used = rough_token_count(&context.text);
        assert!(used <= budgets.recent_tokens + budgets.retrieval_tokens + HEADING_TOKENS, "used {used}");
        assert!(!context.whole_transcript);
    }

    #[test]
    fn the_newest_lines_are_included() {
        let context = build_context(&two_hour_meeting(), None, "what did I miss?", DEFAULT_BUDGETS);
        assert!(context.text.contains("Routine status update number 719 "));
        assert!(context.recent_from.is_some());
    }

    #[test]
    fn keyword_search_finds_the_earlier_line_that_was_asked_about() {
        let context = build_context(&two_hour_meeting(), None, "When does the Kubernetes migration happen?", DEFAULT_BUDGETS);
        assert!(context.text.contains("[05:00] Prof Smith: We agreed the Kubernetes migration slips to March"));
        assert!(context.passages >= 1);
    }

    #[test]
    fn the_same_input_gives_the_same_context() {
        let lines = two_hour_meeting();
        let a = build_context(&lines, Some("note"), "budget review", DEFAULT_BUDGETS);
        let b = build_context(&lines, Some("note"), "budget review", DEFAULT_BUDGETS);
        assert_eq!(a, b);
    }

    #[test]
    fn notes_come_first_and_are_labelled() {
        let lines = vec![line(5.0, None, "Hello everyone")];
        let context = build_context(&lines, Some("Ask Jo about the budget"), "budget?", DEFAULT_BUDGETS);
        let notes_at = context.text.find("[your notes]").expect("notes label");
        let line_at = context.text.find("Hello everyone").expect("transcript line");
        assert!(notes_at < line_at);
        assert!(context.text.contains("Ask Jo about the budget"));
        assert!(context.notes_included);
    }

    #[test]
    fn long_notes_take_at_most_half_the_recent_budget() {
        let notes = "word ".repeat(20_000);
        let budgets = Budgets { recent_tokens: 1000, retrieval_tokens: 0 };
        let lines = vec![line(5.0, None, "Hello everyone")];
        let context = build_context(&lines, Some(&notes), "x", budgets);
        assert!(rough_token_count(&context.text) <= 1000 + HEADING_TOKENS);
        assert!(context.text.contains("Hello everyone"), "the transcript still fits");
    }

    #[test]
    fn blank_notes_are_left_out() {
        let lines = vec![line(5.0, None, "Hello everyone")];
        let context = build_context(&lines, Some("  \n "), "x", DEFAULT_BUDGETS);
        assert!(!context.text.contains("[your notes]"));
        assert!(!context.notes_included);
    }

    #[test]
    fn lines_without_a_speaker_show_only_the_time() {
        let lines = vec![line(65.0, None, "Live line")];
        let context = build_context(&lines, None, "x", DEFAULT_BUDGETS);
        assert!(context.text.contains("[01:05] Live line"));
    }

    #[test]
    fn a_short_meeting_fits_whole_and_in_time_order() {
        let lines = vec![line(20.0, Some("B"), "second"), line(0.0, Some("A"), "first")];
        let context = build_context(&lines, None, "x", DEFAULT_BUDGETS);
        assert!(context.whole_transcript);
        assert_eq!(context.passages, 0);
        assert!(context.text.find("first").unwrap() < context.text.find("second").unwrap());
        assert_eq!(context.segments, vec![Segment { start: 0.0, end: 9.0 }, Segment { start: 20.0, end: 29.0 }]);
        assert_eq!(answered_from(&context, 29.0), "Answered from the whole transcript");
    }

    #[test]
    fn segments_cover_only_the_lines_supplied() {
        let budgets = Budgets { recent_tokens: 400, retrieval_tokens: 100 };
        let lines = two_hour_meeting();
        let context = build_context(&lines, None, "kubernetes", budgets);
        assert!(context.segments.len() < lines.len());
        for segment in &context.segments {
            let shown = format!("[{}]", format_time(segment.start));
            assert!(context.text.contains(&shown), "{shown} supplied but not in the text");
        }
    }

    #[test]
    fn the_answered_from_line_names_the_span_and_passages() {
        let context = AskContext { recent_from: Some(3600.0 - 25.0 * 60.0), passages: 4, ..Default::default() };
        assert_eq!(answered_from(&context, 3600.0), "Answered from the last 25 minutes and 4 earlier passages");
        let one = AskContext { recent_from: Some(3540.0), passages: 1, ..Default::default() };
        assert_eq!(answered_from(&one, 3600.0), "Answered from the last 1 minute and 1 earlier passage");
        let none = AskContext { recent_from: Some(3000.0), passages: 0, ..Default::default() };
        assert_eq!(answered_from(&none, 3600.0), "Answered from the last 10 minutes");
    }

    #[test]
    fn nothing_to_read_gives_an_empty_context() {
        let context = build_context(&[], None, "x", DEFAULT_BUDGETS);
        assert!(context.text.is_empty());
        assert!(context.segments.is_empty());
    }
}
