//! Turns one meeting into the bytes and file name of its Markdown file (spec F-MD-2, F-MD-3,
//! F-MD-7, F-MD-8). Pure: no database, no files, no clock.

use chrono::NaiveDateTime;
use unicode_normalization::UnicodeNormalization;

use crate::ask::context::format_time;

/// Longest cleaned name, in UTF-16 code units, as in the vault format (sync-format section 9).
const MAX_NAME_UNITS: usize = 80;

/// Characters that cannot appear in a file name on Windows or macOS, plus the ones Obsidian
/// refuses in a linkable note name.
const FORBIDDEN: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*', '#', '^', '[', ']'];

const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

#[derive(Debug, Clone, PartialEq)]
pub struct ExportLine {
    pub start: f64,
    pub end: f64,
    pub speaker: Option<String>,
    pub text: String,
}

/// Everything the file is made from. `created_local` is the meeting's start in local time.
#[derive(Debug, Clone, Default)]
pub struct ExportMeeting {
    pub id: String,
    pub title: String,
    pub created_local: NaiveDateTime,
    pub folder: Option<String>,
    pub notes_markdown: Option<String>,
    pub summary_markdown: Option<String>,
    pub lines: Vec<ExportLine>,
}

impl ExportMeeting {
    /// A meeting with nothing to read in it (no notes, summary or transcript).
    pub fn is_empty(&self) -> bool {
        non_blank(&self.notes_markdown).is_none()
            && non_blank(&self.summary_markdown).is_none()
            && self.lines.iter().all(|l| l.text.trim().is_empty())
    }
}

fn non_blank(text: &Option<String>) -> Option<&str> {
    text.as_deref().filter(|t| !t.trim().is_empty())
}

/// Cleans a display name into a safe file or directory name: the vault format's rules
/// (sync-format section 9, steps 1 to 5) plus the characters Obsidian refuses.
pub fn clean_name(name: &str, fallback: &str) -> String {
    let nfc: String = name.nfc().collect();
    let mut kept = String::with_capacity(nfc.len());
    for c in nfc.chars() {
        if c.is_whitespace() {
            kept.push(' ');
        } else if !c.is_control() && !FORBIDDEN.contains(&c) {
            kept.push(c);
        }
    }
    let collapsed = kept.split(' ').filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ");
    let mut cleaned = trim_edges(&collapsed).to_string();

    let mut units = 0;
    let mut cut = cleaned.len();
    for (i, c) in cleaned.char_indices() {
        units += c.len_utf16();
        if units > MAX_NAME_UNITS {
            cut = i;
            break;
        }
    }
    cleaned.truncate(cut);
    let mut cleaned = trim_edges(&cleaned).to_string();

    if cleaned.is_empty() {
        cleaned = fallback.to_string();
    }
    let base = cleaned.split('.').next().unwrap_or("").to_ascii_uppercase();
    if RESERVED.contains(&base.as_str()) {
        cleaned.push(' ');
        cleaned.push_str(&fallback.to_lowercase());
    }
    cleaned
}

fn trim_edges(name: &str) -> &str {
    name.trim_start_matches(|c| c == '.' || c == '_' || c == ' ')
        .trim_end_matches(|c| c == '.' || c == ' ')
}

/// `2026-11-02 1405 Weekly planning`: sorts by date and rarely clashes (decision PM2).
pub fn file_stem(meeting: &ExportMeeting) -> String {
    format!("{} {}", meeting.created_local.format("%Y-%m-%d %H%M"), clean_name(&meeting.title, "Meeting"))
}

/// The directory a filed meeting goes in, or None for the export folder's root.
pub fn folder_dir(meeting: &ExportMeeting) -> Option<String> {
    meeting.folder.as_deref().filter(|f| !f.trim().is_empty()).map(|f| clean_name(f, "Folder"))
}

/// `stem.md`, or `stem (n).md` for the n-th clash.
pub fn file_name(stem: &str, clash: u32) -> String {
    if clash <= 1 {
        format!("{stem}.md")
    } else {
        format!("{stem} ({clash}).md")
    }
}

/// A double-quoted front matter string with the escapes allowed by sync-format section 4.
fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn speakers(meeting: &ExportMeeting) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for line in &meeting.lines {
        if let Some(name) = line.speaker.as_deref().map(clean_speaker).filter(|n| !n.is_empty()) {
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names
}

fn clean_speaker(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .filter(|c| !c.is_control() && !matches!(c, '*' | '[' | ']' | '_' | '`'))
        .collect::<String>()
        .split(' ')
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Obsidian properties (F-MD-3), written with the strict front matter grammar.
fn front_matter(meeting: &ExportMeeting) -> String {
    let mut out = String::from("---\n");
    out.push_str(&format!("title: {}\n", quote(meeting.title.trim())));
    out.push_str(&format!("date: {}\n", quote(&meeting.created_local.format("%Y-%m-%dT%H:%M:%S").to_string())));
    let seconds = meeting.lines.iter().map(|l| l.end.max(l.start)).filter(|s| s.is_finite()).fold(0.0_f64, f64::max);
    if seconds > 0.0 {
        let minutes = ((seconds / 60.0).round() as u64).max(1);
        out.push_str(&format!("duration: {}\n", quote(&format!("{minutes} min"))));
    }
    if let Some(folder) = meeting.folder.as_deref().map(str::trim).filter(|f| !f.is_empty()) {
        out.push_str(&format!("folder: {}\n", quote(folder)));
    }
    let names = speakers(meeting);
    if !names.is_empty() {
        out.push_str("speakers:\n");
        for name in names {
            out.push_str(&format!("  - {}\n", quote(&name)));
        }
    }
    out.push_str("tags:\n  - \"meeting\"\n");
    out.push_str(&format!("quire_id: {}\n", quote(&meeting.id)));
    out.push_str("---\n");
    out
}

/// The whole file (F-MD-2): properties, then My notes, Summary and Transcript, leaving out
/// sections with nothing in them.
pub fn render(meeting: &ExportMeeting) -> String {
    let mut body = render_body(meeting, false);
    if !is_inert(&body) {
        body = render_body(meeting, true);
    }
    front_matter(meeting) + &body
}

/// The sections under the properties. `strict` escapes every Markdown character, for the rare
/// body that still holds live content after the normal pass.
fn render_body(meeting: &ExportMeeting, strict: bool) -> String {
    let clean = |text: &str| if strict { neutralize_strict(text) } else { neutralize(text) };
    let mut out = String::new();
    if let Some(notes) = non_blank(&meeting.notes_markdown) {
        out.push_str("\n## My notes\n\n");
        out.push_str(clean(&normalize_newlines(notes)).trim_matches('\n'));
        out.push('\n');
    }
    if let Some(summary) = non_blank(&meeting.summary_markdown) {
        out.push_str("\n## Summary\n\n");
        out.push_str(clean(&normalize_newlines(summary)).trim_matches('\n'));
        out.push('\n');
    }
    let lines: Vec<String> = meeting
        .lines
        .iter()
        .filter(|l| !l.text.trim().is_empty())
        .map(|l| {
            let text = clean(&l.text.split_whitespace().collect::<Vec<_>>().join(" "));
            match l.speaker.as_deref().map(clean_speaker).filter(|n| !n.is_empty()) {
                Some(name) => format!("**{}** [{}] {}", escape_all(&name), format_time(l.start), text),
                None => format!("[{}] {}", format_time(l.start), text),
            }
        })
        .collect();
    if !lines.is_empty() {
        out.push_str("\n## Transcript\n\n");
        out.push_str(&lines.join("\n\n"));
        out.push('\n');
    }
    out
}

fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Makes remote content inert (F-MD-8): image and embed syntax, raw HTML, script links and
/// Dataview queries come out as plain text, so Obsidian makes no network request and runs
/// nothing when it opens the file. Code is left readable where possible. The result is checked
/// with a CommonMark parser, and anything still live falls back to escaping every character.
pub fn neutralize(markdown: &str) -> String {
    let first = neutralize_readable(markdown);
    if is_inert(&first) {
        first
    } else {
        neutralize_strict(markdown)
    }
}

/// True when a CommonMark parser finds nothing Obsidian would fetch or run: no raw HTML, images,
/// script links, fenced code with a language (Dataview runs `dataviewjs` blocks), inline queries
/// (`= ...`, `$= ...`) or unescaped `![[` embeds.
pub fn is_inert(markdown: &str) -> bool {
    use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag};
    if markdown.match_indices("![[").any(|(i, _)| !markdown[..i].ends_with('\\')) {
        return false;
    }
    Parser::new_ext(markdown, Options::all()).all(|event| match event {
        Event::Html(_) | Event::InlineHtml(_) => false,
        Event::Start(Tag::Image { .. }) => false,
        Event::Start(Tag::Link { dest_url, .. }) => !is_script_url(&dest_url),
        Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => info.trim().is_empty() || info.trim() == "text",
        Event::Code(code) => !is_inline_query(&code),
        _ => true,
    })
}

fn is_script_url(url: &str) -> bool {
    let url: String = url.chars().filter(|c| !c.is_whitespace() && !c.is_control()).collect::<String>().to_ascii_lowercase();
    ["javascript:", "vbscript:", "data:"].iter().any(|s| url.starts_with(s))
}

fn is_inline_query(code: &str) -> bool {
    let code = code.trim_start();
    code.starts_with('=') || code.starts_with("$=")
}

/// Backslash before every character that could start Markdown syntax.
fn escape_all(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    for c in text.chars() {
        if matches!(c, '\\' | '`' | '~' | '<' | '>' | '!' | '[' | ']' | '*' | '_' | '#' | '|' | '=' | '$') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Every character escaped, `<br>` as a space: plain text, whatever the input was.
pub fn neutralize_strict(markdown: &str) -> String {
    markdown.split('\n').map(|line| escape_all(&replace_breaks(line, " "))).collect::<Vec<_>>().join("\n")
}

fn neutralize_readable(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len() + 16);
    let mut fence: Option<(char, usize)> = None;
    for (i, line) in markdown.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let indent = line.len() - line.trim_start_matches(' ').len();
        let trimmed = &line[indent..];
        let marker = trimmed.chars().next().filter(|c| *c == '`' || *c == '~');
        let run = marker.map(|m| trimmed.chars().take_while(|c| *c == m).count()).unwrap_or(0);
        let info = marker.map(|m| trimmed.trim_start_matches(m).trim()).unwrap_or("");
        match fence {
            Some((m, n)) => {
                out.push_str(line);
                if indent <= 3 && marker == Some(m) && run >= n && info.is_empty() {
                    fence = None;
                }
            }
            // A fence opens at most three spaces in, and a backtick fence's info has no backtick.
            // Its language is replaced, so no plugin runs it.
            None if indent <= 3 && run >= 3 && !(marker == Some('`') && info.contains('`')) => {
                let m = marker.unwrap_or('`');
                out.push_str(&line[..indent]);
                out.push_str(&m.to_string().repeat(run));
                if !info.is_empty() {
                    out.push_str("text");
                }
                fence = Some((m, run));
            }
            None => out.push_str(&neutralize_line(&replace_breaks(line, "\n"))),
        }
    }
    out
}

/// `<br>` from the editors becomes `with` (a real line break) instead of an escaped tag.
fn replace_breaks(line: &str, with: &str) -> String {
    let lower = line.to_ascii_lowercase();
    if !lower.contains("<br") {
        return line.to_string();
    }
    let mut out = String::with_capacity(line.len());
    let mut rest = 0;
    let mut search = 0;
    while let Some(found) = lower[search..].find("<br") {
        let at = search + found;
        let tail = &lower[at + 3..];
        let close = tail.trim_start_matches(' ').strip_prefix('/').unwrap_or(tail.trim_start_matches(' '));
        if let Some(after) = close.strip_prefix('>') {
            let end = lower.len() - after.len();
            out.push_str(&line[rest..at]);
            out.push_str(with);
            rest = end;
            search = end;
        } else {
            search = at + 3;
        }
    }
    out.push_str(&line[rest..]);
    out
}

fn neutralize_line(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len() + 8);
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && i + 1 < chars.len() {
            out.push(c);
            out.push(chars[i + 1]);
            i += 2;
            continue;
        }
        if c == '`' {
            let run = chars[i..].iter().take_while(|x| **x == '`').count();
            let content: String = chars[i + run..].iter().collect();
            if is_inline_query(&content) {
                for _ in 0..run {
                    out.push_str("\\`");
                }
                i += run;
                continue;
            }
            if let Some(close) = find_code_close(&chars, i + run, run) {
                out.extend(&chars[i..close + run]);
                i = close + run;
                continue;
            }
            out.extend(&chars[i..i + run]);
            i += run;
            continue;
        }
        let next = chars.get(i + 1).copied();
        let escape = match c {
            '<' => next.map_or(false, |n| n.is_ascii_alphabetic() || matches!(n, '/' | '!' | '?')),
            '!' => next == Some('['),
            '[' => is_script_link(&chars, i),
            _ => false,
        };
        if escape {
            out.push('\\');
        }
        out.push(c);
        i += 1;
    }
    out
}

fn find_code_close(chars: &[char], from: usize, run: usize) -> Option<usize> {
    let mut j = from;
    while j < chars.len() {
        if chars[j] == '`' {
            let n = chars[j..].iter().take_while(|x| **x == '`').count();
            if n == run {
                return Some(j);
            }
            j += n;
        } else {
            j += 1;
        }
    }
    None
}

/// `[text](javascript:...)` and the like, at the `[`.
fn is_script_link(chars: &[char], open: usize) -> bool {
    let mut depth = 0;
    let mut j = open;
    while j < chars.len() {
        match chars[j] {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        j += 1;
    }
    if chars.get(j + 1) != Some(&'(') {
        return false;
    }
    let target: String = chars[j + 2..].iter().collect::<String>();
    let target = target.trim_start().trim_start_matches('<').to_ascii_lowercase();
    let target: String = target.chars().filter(|c| !c.is_whitespace() && !c.is_control()).collect();
    ["javascript:", "vbscript:", "data:"].iter().any(|s| target.starts_with(s))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at(h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 2).unwrap().and_hms_opt(h, m, 7).unwrap()
    }

    fn line(start: f64, speaker: Option<&str>, text: &str) -> ExportLine {
        ExportLine { start, end: start + 5.0, speaker: speaker.map(str::to_string), text: text.to_string() }
    }

    fn full() -> ExportMeeting {
        ExportMeeting {
            id: "meeting-1".to_string(),
            title: "Weekly planning".to_string(),
            created_local: at(14, 5),
            folder: Some("Planning".to_string()),
            notes_markdown: Some("- venue\r\n- budget\n".to_string()),
            summary_markdown: Some("# Weekly planning\n\n## Decisions\n\n- Agreed.\n".to_string()),
            lines: vec![
                line(0.0, Some("Alex Morgan"), "Thanks for joining."),
                line(9.4, None, "We can cover\nthe first two."),
                line(3125.0, Some("Sam Lee"), "Done."),
            ],
        }
    }

    #[test]
    fn full_meeting_renders_exactly() {
        let expected = "---\n\
title: \"Weekly planning\"\n\
date: \"2026-11-02T14:05:07\"\n\
duration: \"52 min\"\n\
folder: \"Planning\"\n\
speakers:\n  - \"Alex Morgan\"\n  - \"Sam Lee\"\n\
tags:\n  - \"meeting\"\n\
quire_id: \"meeting-1\"\n\
---\n\
\n## My notes\n\n- venue\n- budget\n\
\n## Summary\n\n# Weekly planning\n\n## Decisions\n\n- Agreed.\n\
\n## Transcript\n\n**Alex Morgan** [00:00] Thanks for joining.\n\n[00:09] We can cover the first two.\n\n**Sam Lee** [52:05] Done.\n";
        assert_eq!(render(&full()), expected);
    }

    #[test]
    fn notes_only_leaves_out_empty_sections() {
        let meeting = ExportMeeting {
            id: "meeting-2".to_string(),
            title: "Idea".to_string(),
            created_local: at(9, 0),
            notes_markdown: Some("Just a thought".to_string()),
            summary_markdown: Some("   \n".to_string()),
            ..Default::default()
        };
        let expected = "---\ntitle: \"Idea\"\ndate: \"2026-11-02T09:00:07\"\ntags:\n  - \"meeting\"\nquire_id: \"meeting-2\"\n---\n\n## My notes\n\nJust a thought\n";
        assert_eq!(render(&meeting), expected);
        assert!(!meeting.is_empty());
        assert!(ExportMeeting { notes_markdown: Some(" ".into()), ..Default::default() }.is_empty());
    }

    #[test]
    fn front_matter_values_are_always_quoted_and_escaped() {
        let mut meeting = full();
        meeting.title = "Plan: \"Q4\" \\ draft\n---\nevil: true".to_string();
        let text = render(&meeting);
        let fm = text.split("---\n").nth(1).unwrap();
        assert!(fm.contains("title: \"Plan: \\\"Q4\\\" \\\\ draft\\n---\\nevil: true\"\n"));
        // Every property line is `key: value` with a quoted value, a list, or a list entry
        for l in fm.lines() {
            let ok = l.starts_with("  - \"") || l.ends_with(':') || l.splitn(2, ": ").nth(1).map_or(false, |v| v.starts_with('"') && v.ends_with('"'));
            assert!(ok, "unexpected front matter line {l:?}");
        }
        assert!(!fm.contains("evil: true\n"));
    }

    #[test]
    fn file_names_use_date_and_cleaned_title() {
        assert_eq!(file_stem(&full()), "2026-11-02 1405 Weekly planning");
        assert_eq!(file_name("a", 1), "a.md");
        assert_eq!(file_name("a", 3), "a (3).md");
        let mut m = full();
        m.title = "  ../Q4: plan? [draft] #1 | a/b\\c  ".to_string();
        assert_eq!(file_stem(&m), "2026-11-02 1405 Q4 plan draft 1 abc");
        m.title = "\n\t ...".to_string();
        assert_eq!(file_stem(&m), "2026-11-02 1405 Meeting");
    }

    #[test]
    fn names_follow_the_vault_cleaning_rules() {
        assert_eq!(clean_name("CON", "Folder"), "CON folder");
        assert_eq!(clean_name("nul.txt", "Folder"), "nul.txt folder");
        assert_eq!(clean_name("_.hidden.", "Folder"), "hidden");
        assert_eq!(clean_name("", "Folder"), "Folder");
        // NFC: a decomposed e-acute becomes one character
        assert_eq!(clean_name("Cafe\u{301}", "Folder"), "Caf\u{e9}");
        // 80 UTF-16 units, cut at a character boundary, emoji count as two
        let long = "\u{1F600}".repeat(50);
        let cleaned = clean_name(&long, "Folder");
        assert_eq!(cleaned.encode_utf16().count(), 80);
        assert_eq!(clean_name(&format!("{} .", "a".repeat(79)), "Folder"), "a".repeat(79));
        let mut m = full();
        m.folder = Some("Work/Notes:".to_string());
        assert_eq!(folder_dir(&m).as_deref(), Some("WorkNotes"));
        m.folder = Some("  ".to_string());
        assert_eq!(folder_dir(&m), None);
    }

    #[test]
    fn remote_content_is_written_as_plain_text() {
        let hostile = "![x](https://evil.example/p.png) and ![[https://evil.example/a]]\n\
<img src=\"https://evil.example/t.gif\"> <iframe src=x></iframe> <script>alert(1)</script>\n\
[click](javascript:alert(1)) [ok](https://example.com) [d]( DATA:text/html,x)\n\
a < b and 2<3 stay; `<b>code</b>` and ![kept](in-code) inside `![x](y)`\n\
```\n<div>fenced</div>\n![a](b)\n```\n\
line<br>next<BR/>end";
        let out = neutralize(hostile);
        assert!(out.contains("\\![x](https://evil.example/p.png)"));
        assert!(out.contains("\\![[https://evil.example/a]]"));
        assert!(out.contains("\\<img src="));
        assert!(out.contains("\\<iframe src=x>\\</iframe>"));
        assert!(out.contains("\\<script>alert(1)\\</script>"));
        assert!(out.contains("\\[click](javascript:alert(1))"));
        assert!(out.contains(" [ok](https://example.com)"));
        assert!(out.contains("\\[d]( DATA:text/html,x)"));
        assert!(out.contains("a < b and 2<3 stay"));
        assert!(out.contains("`<b>code</b>`"));
        assert!(out.contains("inside `![x](y)`"));
        assert!(out.contains("```\n<div>fenced</div>\n![a](b)\n```"));
        assert!(out.ends_with("line\nnext\nend"));
        // Nothing unescaped is left that could start an embed or a tag outside code
        for (n, l) in out.lines().enumerate() {
            if (4..=7).contains(&n) || l.contains('`') {
                continue;
            }
            assert!(!l.contains(" ![") && !l.starts_with("!["), "embed left in {l:?}");
        }
    }

    #[test]
    fn transcript_and_summary_are_neutralized_in_the_file() {
        let mut m = full();
        m.summary_markdown = Some("![t](https://evil.example/x.png)".to_string());
        m.lines = vec![line(1.0, Some("**Eve** [boss]"), "<img src=https://evil.example/y>")];
        let text = render(&m);
        assert!(text.contains("\\![t](https://evil.example/x.png)"));
        assert!(text.contains("**Eve boss** [00:01] \\<img src=https://evil.example/y>"));
        assert!(text.contains("  - \"Eve boss\"\n"));
    }

    /// What Obsidian would fetch or run, per a CommonMark parser: raw HTML, images, embeds,
    /// script links and Dataview code.
    fn live_content(markdown: &str) -> Vec<String> {
        use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag};
        let mut found = Vec::new();
        for event in Parser::new_ext(markdown, Options::all()) {
            match event {
                Event::Html(h) | Event::InlineHtml(h) => found.push(format!("html {h:?}")),
                Event::Start(Tag::Image { dest_url, .. }) => found.push(format!("image {dest_url}")),
                Event::Start(Tag::Link { dest_url, .. })
                    if ["javascript:", "vbscript:", "data:"].iter().any(|s| dest_url.trim().to_ascii_lowercase().starts_with(s)) =>
                {
                    found.push(format!("script link {dest_url}"))
                }
                Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) if !info.trim().is_empty() && info.trim() != "text" => {
                    found.push(format!("fence {info}"))
                }
                Event::Code(code) if code.trim_start().starts_with('=') || code.trim_start().starts_with("$=") => {
                    found.push(format!("inline query {code}"))
                }
                Event::Text(t) if t.contains("![[") => found.push(format!("embed {t}")),
                _ => {}
            }
        }
        found
    }

    #[test]
    fn hostile_inputs_from_the_review_stay_inert() {
        let cases = [
            "    ```\n<img src=https://x/a.png>",
            "- a\n\n  ```\n<iframe src=https://x/f></iframe>",
            "```dataviewjs\ndv.paragraph(await fetch('https://x'))\n```",
            "Total: `$= fetch('https://x')` and `= this.file`",
            "`<br><iframe src=https://x/f><br>`",
            "a `x\ny` <img src=https://x/p.png> `z`",
            "# a `x\n<img src=https://x/h.png> `y`",
            "~~~ js\n<img src=https://x/t.png>\n~~~",
            "```\ncode\n```\n<img src=https://x/after.png>",
            "``` `x`\n<img src=https://x/i.png>",
            "![ref][r]\n\n[r]: https://x/r.png",
            "<https://x/auto> and <style>body{background:url(https://x/c.png)}</style>",
        ];
        for case in cases {
            let out = neutralize(case);
            assert!(live_content(&out).is_empty(), "{case:?} became {out:?}: {:?}", live_content(&out));
        }
    }

    #[test]
    fn speaker_names_cannot_carry_html() {
        let mut m = full();
        m.lines = vec![line(1.0, Some("<img src=https://x/p.png>"), "hello"), line(2.0, Some("!Eve"), "hi")];
        let text = render(&m);
        let body = text.splitn(3, "---\n").nth(2).unwrap();
        assert!(live_content(body).is_empty(), "{body}: {:?}", live_content(body));
    }

    #[test]
    fn every_rendered_body_is_inert() {
        let mut m = full();
        m.notes_markdown = Some("a `x\ny` <img src=https://x/n.png> `z`".to_string());
        m.summary_markdown = Some("```dataviewjs\nx\n```".to_string());
        m.lines = vec![line(1.0, Some("A"), "`<br><img src=https://x/t.png>`")];
        let text = render(&m);
        let body = text.splitn(3, "---\n").nth(2).unwrap();
        assert!(live_content(body).is_empty(), "{body}: {:?}", live_content(body));
    }
}
