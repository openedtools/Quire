//! Citation checking for Ask answers (spec F-QA-4, AC-QA-2).
//!
//! The model is asked to cite `[mm:ss]`. Only citations that fall inside a line the app supplied
//! are kept; anything else is removed from the answer so it cannot point at nothing.

use super::context::Segment;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Citation {
    /// As written in the answer, without brackets, for example `02:15`.
    pub label: String,
    /// Seconds from the start of the recording, used to scroll to the line.
    pub seconds: f64,
}

/// Read `mm:ss` or `h:mm:ss`.
pub fn parse_time(label: &str) -> Option<f64> {
    let parts: Vec<&str> = label.split(':').collect();
    if !parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())) {
        return None;
    }
    let two = |p: &str| -> Option<u64> { if p.len() == 2 { p.parse().ok().filter(|v| *v < 60) } else { None } };
    let seconds = match parts.as_slice() {
        [m, s] if m.len() >= 2 => m.parse::<u64>().ok()? * 60 + two(s)?,
        [h, m, s] => h.parse::<u64>().ok()? * 3600 + two(m)? * 60 + two(s)?,
        _ => return None,
    };
    Some(seconds as f64)
}

/// Inside a supplied line. Shown times are rounded down, so the start is too.
fn supplied(seconds: f64, segments: &[Segment]) -> bool {
    segments.iter().any(|s| seconds >= s.start.floor() && seconds <= s.end.ceil())
}

/// Remove citations outside every segment. Returns the cleaned answer and the kept citations,
/// in order, without repeats.
pub fn check_citations(answer: &str, segments: &[Segment]) -> (String, Vec<Citation>) {
    // Innermost bracket pairs, so a time inside other brackets is still checked
    let mut pairs = Vec::new();
    let mut open = None;
    for (i, c) in answer.char_indices() {
        match c {
            '[' => open = Some(i),
            ']' => {
                if let Some(start) = open.take() {
                    pairs.push((start, i));
                }
            }
            _ => {}
        }
    }

    let mut cleaned = String::with_capacity(answer.len());
    let mut cited: Vec<Citation> = Vec::new();
    let mut copied = 0;
    for (start, end) in pairs {
        let label = &answer[start + 1..end];
        cleaned.push_str(&answer[copied..start]);
        copied = end + 1;
        match parse_time(label) {
            Some(seconds) if supplied(seconds, segments) => {
                cleaned.push_str(&answer[start..=end]);
                if !cited.iter().any(|c| c.label == label) {
                    cited.push(Citation { label: label.to_string(), seconds });
                }
            }
            // A time, or something shaped like one, that points at nothing: drop it and the space before it
            _ if looks_like_time(label) => {
                if cleaned.ends_with(' ') {
                    cleaned.pop();
                }
            }
            _ => cleaned.push_str(&answer[start..=end]),
        }
    }
    cleaned.push_str(&answer[copied..]);
    (cleaned, cited)
}

/// Digits in two or three groups split by colons, like `5:30` or `1:75:00`.
fn looks_like_time(label: &str) -> bool {
    let parts: Vec<&str> = label.split(':').collect();
    (2..=3).contains(&parts.len()) && parts.iter().all(|p| !p.is_empty() && p.len() <= 3 && p.chars().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segments() -> Vec<Segment> {
        vec![Segment { start: 125.6, end: 131.0 }, Segment { start: 3723.0, end: 3730.0 }]
    }

    #[test]
    fn times_parse_with_and_without_hours() {
        assert_eq!(parse_time("02:05"), Some(125.0));
        assert_eq!(parse_time("1:02:03"), Some(3723.0));
        assert_eq!(parse_time("2:5"), None);
        assert_eq!(parse_time("aa:bb"), None);
        assert_eq!(parse_time("02:75"), None);
    }

    #[test]
    fn a_citation_inside_a_supplied_line_is_kept() {
        let (answer, cited) = check_citations("Jo said March [02:05].", &segments());
        assert_eq!(answer, "Jo said March [02:05].");
        assert_eq!(cited, vec![Citation { label: "02:05".into(), seconds: 125.0 }]);
    }

    #[test]
    fn a_citation_outside_every_line_is_dropped() {
        let (answer, cited) = check_citations("Jo said March [45:00] and later [1:02:04].", &segments());
        assert_eq!(answer, "Jo said March and later [1:02:04].");
        assert_eq!(cited, vec![Citation { label: "1:02:04".into(), seconds: 3724.0 }]);
    }

    #[test]
    fn repeated_citations_are_listed_once() {
        let (_, cited) = check_citations("[02:06] then [02:06] again", &segments());
        assert_eq!(cited.len(), 1);
    }

    #[test]
    fn notes_references_and_other_brackets_are_left_alone() {
        let text = "From [your notes]: ask Jo. See [link] too.";
        let (answer, cited) = check_citations(text, &segments());
        assert_eq!(answer, text);
        assert!(cited.is_empty());
    }
}

#[cfg(test)]
mod review_tests {
    use super::*;

    fn segments() -> Vec<Segment> {
        vec![Segment { start: 125.6, end: 131.0 }]
    }

    #[test]
    fn badly_formed_times_are_dropped_too() {
        let (answer, cited) = check_citations("A [5:30] B [02:75] C [1:75:00] D [02:05]", &segments());
        assert_eq!(answer, "A B C D [02:05]");
        assert_eq!(cited.len(), 1);
    }

    #[test]
    fn a_time_inside_other_brackets_is_still_checked() {
        let (answer, cited) = check_citations("see [note [45:00]] and [x [02:05]]", &segments());
        assert_eq!(answer, "see [note] and [x [02:05]]");
        assert_eq!(cited, vec![Citation { label: "02:05".into(), seconds: 125.0 }]);
    }
}
