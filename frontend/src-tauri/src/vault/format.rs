//! The shared vault format, version 1 (`sync-format.md`): identifiers, the strict front matter
//! grammar, and the canonical writers and readers for notes, transcript sidecars, people, folders
//! and the vault marker. Pure: no database, no files, no clock.

use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

pub const FORMAT_LEVEL: i64 = 1;

/// Reader size limits (section 4).
pub const MAX_NOTE_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_SIDECAR_BYTES: usize = 20 * 1024 * 1024;
pub const MAX_PERSON_BYTES: usize = 256 * 1024;
pub const MAX_FOLDER_BYTES: usize = 16 * 1024;

const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// A ULID (section 3): 48 bits of milliseconds then 80 random bits, 26 Crockford base32 characters.
pub fn ulid(millis: u64, random: [u8; 10]) -> String {
    let mut value: u128 = (millis as u128 & 0xFFFF_FFFF_FFFF) << 80;
    for (i, b) in random.iter().enumerate() {
        value |= (*b as u128) << (8 * (9 - i));
    }
    (0..26).rev().map(|i| CROCKFORD[((value >> (5 * i)) & 31) as usize] as char).collect()
}

pub fn new_ulid(millis: u64) -> String {
    let random: [u8; 10] = rand::random();
    ulid(millis, random)
}

pub fn is_ulid(text: &str) -> bool {
    text.len() == 26 && text.bytes().all(|b| CROCKFORD.contains(&b)) && text.as_bytes()[0] <= b'7'
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// UTC ISO 8601 with milliseconds and a trailing `Z` (section 3).
pub fn timestamp(t: chrono::DateTime<chrono::Utc>) -> String {
    t.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

pub fn nfc(text: &str) -> String {
    text.nfc().collect()
}

// ===== Front matter (section 4) =====

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    Int(i64),
    Bool(bool),
    Null,
    List(Vec<Value>),
    Map(Vec<(String, Value)>),
}

impl Value {
    pub fn str(text: &str) -> Value {
        Value::Str(nfc(text))
    }
    pub fn opt_str(text: Option<&str>) -> Value {
        text.map_or(Value::Null, Value::str)
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(*n),
            _ => None,
        }
    }
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Map(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FormatError(pub String);

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

fn bad<T>(message: impl Into<String>) -> Result<T, FormatError> {
    Err(FormatError(message.into()))
}

/// A double-quoted string with only the escapes the grammar allows; never `\/`.
pub fn quote(value: &str) -> String {
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

fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::Str(s) => Some(quote(s)),
        Value::Int(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Null => Some("null".into()),
        Value::List(items) if items.is_empty() => Some("[]".into()),
        Value::Map(entries) if entries.is_empty() => Some("{}".into()),
        _ => None,
    }
}

fn write_entries(out: &mut String, entries: &[(String, Value)], indent: usize, first_prefix: Option<&str>) {
    for (i, (key, value)) in entries.iter().enumerate() {
        let lead = match (i, first_prefix) {
            (0, Some(prefix)) => prefix.to_string(),
            _ => " ".repeat(indent),
        };
        match scalar(value) {
            Some(text) => out.push_str(&format!("{lead}{key}: {text}\n")),
            None => {
                out.push_str(&format!("{lead}{key}:\n"));
                write_block(out, value, indent + 2);
            }
        }
    }
}

fn write_block(out: &mut String, value: &Value, indent: usize) {
    match value {
        Value::Map(entries) => write_entries(out, entries, indent, None),
        Value::List(items) => {
            for item in items {
                let dash = format!("{}- ", " ".repeat(indent));
                match item {
                    Value::Map(entries) if !entries.is_empty() => write_entries(out, entries, indent + 2, Some(&dash)),
                    other => out.push_str(&format!("{dash}{}\n", scalar(other).unwrap_or_else(|| "null".into()))),
                }
            }
        }
        _ => {}
    }
}

/// `---`, the entries in the given order, `---`, then the body exactly as given.
pub fn write_document(entries: &[(String, Value)], body: &str) -> String {
    let mut out = String::from("---\n");
    write_entries(&mut out, entries, 0, None);
    out.push_str("---\n");
    out.push_str(body);
    out
}

struct Line<'a> {
    indent: usize,
    text: &'a str,
}

fn key_ok(key: &str) -> bool {
    let mut chars = key.chars();
    matches!(chars.next(), Some('a'..='z')) && chars.all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_'))
}

fn parse_string(text: &str) -> Result<String, FormatError> {
    let inner = &text[1..text.len() - 1];
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return bad("unescaped quote in a string"),
            c if c.is_control() => return bad("a control character must be escaped"),
            '\\' => match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('u') => {
                    let hex: String = chars.by_ref().take(4).collect();
                    let code = (hex.len() == 4 && hex.chars().all(|c| c.is_ascii_hexdigit())).then(|| u32::from_str_radix(&hex, 16).ok()).flatten();
                    match code.and_then(char::from_u32) {
                        Some(ch) => out.push(ch),
                        None => return bad("bad \\u escape"),
                    }
                }
                _ => return bad("escape not allowed"),
            },
            c => out.push(c),
        }
    }
    Ok(out)
}

fn parse_scalar(text: &str) -> Result<Value, FormatError> {
    match text {
        "true" => return Ok(Value::Bool(true)),
        "false" => return Ok(Value::Bool(false)),
        "null" => return Ok(Value::Null),
        "[]" => return Ok(Value::List(vec![])),
        "{}" => return Ok(Value::Map(vec![])),
        _ => {}
    }
    if text.len() >= 2 && text.starts_with('"') && text.ends_with('"') {
        return parse_string(text).map(Value::Str);
    }
    let digits = text.strip_prefix('-').unwrap_or(text);
    if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
        return text.parse::<i64>().map(Value::Int).or_else(|_| bad("integer out of range"));
    }
    // Never quote the value: it can be part of a title, and errors reach the log
    bad("value not allowed")
}

fn split_entry(text: &str) -> Result<(&str, Option<&str>), FormatError> {
    let Some(colon) = text.find(':') else { return bad("expected key: value") };
    let key = &text[..colon];
    if !key_ok(key) {
        return bad("bad key");
    }
    let rest = &text[colon + 1..];
    if rest.is_empty() {
        return Ok((key, None));
    }
    match rest.strip_prefix(' ') {
        Some(value) if !value.is_empty() && !value.starts_with(' ') => Ok((key, Some(value))),
        _ => bad("expected one space after the colon"),
    }
}

fn parse_child(lines: &[Line], i: &mut usize, indent: usize) -> Result<Value, FormatError> {
    match lines.get(*i) {
        Some(line) if line.indent == indent && line.text.starts_with("- ") => parse_list(lines, i, indent),
        Some(line) if line.indent == indent => parse_map(lines, i, indent, None),
        _ => bad("an empty value must be written as [] or {}"),
    }
}

fn parse_map(lines: &[Line], i: &mut usize, indent: usize, first: Option<&str>) -> Result<Value, FormatError> {
    let mut entries: Vec<(String, Value)> = Vec::new();
    let mut pending = first;
    loop {
        let text = match pending.take() {
            Some(t) => t,
            None => match lines.get(*i) {
                Some(line) if line.indent == indent && !line.text.starts_with("- ") => {
                    *i += 1;
                    line.text
                }
                Some(line) if line.indent > indent => return bad("unexpected indentation"),
                _ => break,
            },
        };
        let (key, value) = split_entry(text)?;
        if entries.iter().any(|(k, _)| k == key) {
            return bad(format!("duplicate key {key}"));
        }
        let value = match value {
            Some(v) => parse_scalar(v)?,
            None => parse_child(lines, i, indent + 2)?,
        };
        entries.push((key.to_string(), value));
    }
    Ok(Value::Map(entries))
}

fn parse_list(lines: &[Line], i: &mut usize, indent: usize) -> Result<Value, FormatError> {
    let mut items = Vec::new();
    while let Some(line) = lines.get(*i) {
        if line.indent != indent || !line.text.starts_with("- ") {
            if line.indent > indent {
                return bad("unexpected indentation");
            }
            break;
        }
        *i += 1;
        let rest = &line.text[2..];
        let looks_like_entry = rest.find(':').map_or(false, |c| key_ok(&rest[..c]));
        if looks_like_entry && !rest.starts_with('"') {
            items.push(parse_map(lines, i, indent + 2, Some(rest))?);
        } else {
            items.push(parse_scalar(rest)?);
        }
    }
    Ok(Value::List(items))
}

/// Splits a file into its front matter entries and its body, enforcing the grammar exactly.
/// Accepts a byte order mark and CRLF line endings in the front matter.
pub fn parse_document(text: &str) -> Result<(Vec<(String, Value)>, String), FormatError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let rest = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
        .ok_or_else(|| FormatError("missing front matter".into()))?;
    let mut lines = Vec::new();
    let mut offset = 0;
    let mut end = None;
    for raw in rest.split_inclusive('\n') {
        let line = raw.strip_suffix('\n').unwrap_or(raw);
        let line = line.strip_suffix('\r').unwrap_or(line);
        offset += raw.len();
        if line == "---" {
            end = Some(offset);
            break;
        }
        if line.contains('\t') {
            return bad("tabs are not allowed");
        }
        if line.trim().is_empty() {
            return bad("blank lines are not allowed in front matter");
        }
        let indent = line.len() - line.trim_start_matches(' ').len();
        if indent % 2 != 0 {
            return bad("indentation must be 2 spaces per level");
        }
        lines.push(Line { indent, text: &line[indent..] });
    }
    let Some(end) = end else { return bad("front matter is not closed") };
    let mut i = 0;
    let Value::Map(entries) = parse_map(&lines, &mut i, 0, None)? else { unreachable!() };
    if i != lines.len() {
        return bad("unexpected line in front matter");
    }
    Ok((entries, rest[end..].to_string()))
}

// ===== Notes (section 6) =====

#[derive(Debug, Clone, PartialEq)]
pub struct Speaker {
    pub slot: i64,
    pub name: String,
    pub person: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptRef {
    pub file: String,
    pub sha256: String,
    pub segments: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SummaryMeta {
    pub status: String,
    pub generated_at: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioMeta {
    pub retention: String,
    pub expires: Option<String>,
    pub state: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub created: String,
    pub updated: String,
    pub updated_by: String,
    pub rev: i64,
    pub started: Option<String>,
    pub ended: Option<String>,
    pub duration_s: Option<i64>,
    pub language: Option<String>,
    pub source: String,
    pub mode: String,
    pub speakers: Vec<Speaker>,
    pub transcript: Option<TranscriptRef>,
    pub summary: Option<SummaryMeta>,
    pub audio: AudioMeta,
    pub extra: Vec<(String, Value)>,
    pub body: String,
}

const NOTE_KEYS: &[&str] = &[
    "quire", "id", "title", "created", "updated", "updated_by", "rev", "started", "ended", "duration_s", "language", "source",
    "mode", "speakers", "transcript", "summary", "audio",
];

fn opt_int(n: Option<i64>) -> Value {
    n.map_or(Value::Null, Value::Int)
}

pub fn note_entries(note: &Note) -> Vec<(String, Value)> {
    let speakers = Value::List(
        note.speakers
            .iter()
            .map(|s| {
                Value::Map(vec![
                    ("slot".into(), Value::Int(s.slot)),
                    ("name".into(), Value::str(&s.name)),
                    ("person".into(), Value::opt_str(s.person.as_deref())),
                ])
            })
            .collect(),
    );
    let transcript = note.transcript.as_ref().map_or(Value::Null, |t| {
        Value::Map(vec![
            ("file".into(), Value::str(&t.file)),
            ("sha256".into(), Value::str(&t.sha256)),
            ("segments".into(), Value::Int(t.segments)),
        ])
    });
    let summary = note.summary.as_ref().map_or(Value::Null, |s| {
        Value::Map(vec![
            ("status".into(), Value::str(&s.status)),
            ("generated_at".into(), Value::opt_str(s.generated_at.as_deref())),
            ("model".into(), Value::opt_str(s.model.as_deref())),
        ])
    });
    let audio = Value::Map(vec![
        ("retention".into(), Value::str(&note.audio.retention)),
        ("expires".into(), Value::opt_str(note.audio.expires.as_deref())),
        ("state".into(), Value::str(&note.audio.state)),
        ("deleted_at".into(), Value::opt_str(note.audio.deleted_at.as_deref())),
        // Audio never enters the vault in Phase E (plan PE4)
        ("file".into(), Value::Null),
        ("sha256".into(), Value::Null),
        ("bytes".into(), Value::Null),
    ]);
    let mut entries = vec![
        ("quire".into(), Value::Int(FORMAT_LEVEL)),
        ("id".into(), Value::str(&note.id)),
        ("title".into(), Value::str(&note.title)),
        ("created".into(), Value::str(&note.created)),
        ("updated".into(), Value::str(&note.updated)),
        ("updated_by".into(), Value::str(&note.updated_by)),
        ("rev".into(), Value::Int(note.rev)),
        ("started".into(), Value::opt_str(note.started.as_deref())),
        ("ended".into(), Value::opt_str(note.ended.as_deref())),
        ("duration_s".into(), opt_int(note.duration_s)),
        ("language".into(), Value::opt_str(note.language.as_deref())),
        ("source".into(), Value::str(&note.source)),
        ("mode".into(), Value::str(&note.mode)),
        ("speakers".into(), speakers),
        ("transcript".into(), transcript),
        ("summary".into(), summary),
        ("audio".into(), audio),
    ];
    entries.extend(note.extra.iter().cloned());
    entries
}

pub fn render_note(note: &Note) -> String {
    write_document(&note_entries(note), &note.body)
}

/// The note's canonical content (section 11): everything but `updated`, `updated_by` and `rev`.
/// Equal keys mean nothing changed, so `rev` and `updated` stay.
pub fn note_content_key(note: &Note) -> String {
    let blank = Note { updated: String::new(), updated_by: String::new(), rev: 0, ..note.clone() };
    sha256_hex(render_note(&blank).as_bytes())
}

fn req_str(map: &Value, key: &str) -> Result<String, FormatError> {
    map.get(key).and_then(Value::as_str).map(str::to_string).ok_or_else(|| FormatError(format!("{key} must be a string")))
}

fn opt_str_field(map: &Value, key: &str) -> Result<Option<String>, FormatError> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Str(s)) => Ok(Some(s.clone())),
        _ => bad(format!("{key} must be a string or null")),
    }
}

pub fn parse_note(text: &str) -> Result<Note, FormatError> {
    if text.len() > MAX_NOTE_BYTES {
        return bad("note file too large");
    }
    let (entries, body) = parse_document(text)?;
    let map = Value::Map(entries.clone());
    if map.get("quire").and_then(Value::as_int) != Some(FORMAT_LEVEL) {
        return bad("not a version 1 note");
    }
    let id = req_str(&map, "id")?;
    if !is_ulid(&id) {
        return bad("id is not a ULID");
    }
    let speakers = match map.get("speakers") {
        Some(Value::List(items)) => items
            .iter()
            .map(|s| {
                Ok(Speaker {
                    slot: s.get("slot").and_then(Value::as_int).ok_or_else(|| FormatError("speaker slot".into()))?,
                    name: req_str(s, "name")?,
                    person: opt_str_field(s, "person")?,
                })
            })
            .collect::<Result<Vec<_>, FormatError>>()?,
        _ => return bad("speakers must be a list"),
    };
    let transcript = match map.get("transcript") {
        Some(Value::Null) | None => None,
        Some(t) => Some(TranscriptRef {
            file: req_str(t, "file")?,
            sha256: req_str(t, "sha256")?,
            segments: t.get("segments").and_then(Value::as_int).ok_or_else(|| FormatError("segments".into()))?,
        }),
    };
    let summary = match map.get("summary") {
        Some(Value::Null) | None => None,
        Some(s) => Some(SummaryMeta { status: req_str(s, "status")?, generated_at: opt_str_field(s, "generated_at")?, model: opt_str_field(s, "model")? }),
    };
    let a = map.get("audio").ok_or_else(|| FormatError("audio".into()))?;
    let audio = AudioMeta {
        retention: req_str(a, "retention")?,
        expires: opt_str_field(a, "expires")?,
        state: req_str(a, "state")?,
        deleted_at: opt_str_field(a, "deleted_at")?,
    };
    Ok(Note {
        title: req_str(&map, "title")?,
        created: req_str(&map, "created")?,
        updated: req_str(&map, "updated")?,
        updated_by: req_str(&map, "updated_by")?,
        rev: map.get("rev").and_then(Value::as_int).ok_or_else(|| FormatError("rev".into()))?,
        started: opt_str_field(&map, "started")?,
        ended: opt_str_field(&map, "ended")?,
        duration_s: match map.get("duration_s") {
            None | Some(Value::Null) => None,
            Some(Value::Int(n)) => Some(*n),
            _ => return bad("duration_s must be an integer or null"),
        },
        language: opt_str_field(&map, "language")?,
        source: req_str(&map, "source")?,
        mode: req_str(&map, "mode")?,
        id,
        speakers,
        transcript,
        summary,
        audio,
        extra: entries.into_iter().filter(|(k, _)| !NOTE_KEYS.contains(&k.as_str())).collect(),
        body,
    })
}

// ===== Transcript sidecars (section 7) =====

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Segment {
    pub id: String,
    pub start: f64,
    pub end: f64,
    pub speaker: Option<i64>,
    pub text: String,
}

/// The sidecar bytes: one segment per line, keys in a fixed order, sorted by start.
pub fn render_sidecar(note_id: &str, segments: &[Segment]) -> String {
    let mut sorted: Vec<&Segment> = segments.iter().collect();
    sorted.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal).then_with(|| a.id.cmp(&b.id)));
    let mut out = format!("{{\n  \"quire\": {},\n  \"note_id\": {},\n  \"segments\": [", FORMAT_LEVEL, serde_json::to_string(note_id).unwrap_or_default());
    for (i, s) in sorted.iter().enumerate() {
        let clean = Segment { text: nfc(&s.text), ..(*s).clone() };
        out.push_str(if i == 0 { "\n    " } else { ",\n    " });
        out.push_str(&serde_json::to_string(&clean).unwrap_or_default());
    }
    out.push_str(if sorted.is_empty() { "]\n}\n" } else { "\n  ]\n}\n" });
    out
}

/// `<id>.<first 8 of sha256>.transcript.json`
pub fn sidecar_name(note_id: &str, sha256: &str) -> String {
    format!("{}.{}.transcript.json", note_id, &sha256[..8])
}

#[derive(serde::Deserialize)]
struct SidecarFile {
    quire: i64,
    note_id: String,
    segments: Vec<Segment>,
}

pub fn parse_sidecar(text: &str) -> Result<(String, Vec<Segment>), FormatError> {
    if text.len() > MAX_SIDECAR_BYTES {
        return bad("sidecar too large");
    }
    let file: SidecarFile = serde_json::from_str(text).map_err(|e| FormatError(format!("sidecar: {e}")))?;
    if file.quire != FORMAT_LEVEL || !is_ulid(&file.note_id) {
        return bad("not a version 1 sidecar");
    }
    Ok((file.note_id, file.segments))
}

// ===== People (section 8) =====

#[derive(Debug, Clone, PartialEq)]
pub struct Person {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub created: String,
    pub updated: String,
    pub updated_by: String,
    pub rev: i64,
    pub merged_into: Option<String>,
    pub extra: Vec<(String, Value)>,
    pub body: String,
}

const PERSON_KEYS: &[&str] = &["quire", "id", "name", "aliases", "created", "updated", "updated_by", "rev", "merged_into"];

pub fn render_person(person: &Person) -> String {
    let mut entries = vec![
        ("quire".into(), Value::Int(FORMAT_LEVEL)),
        ("id".into(), Value::str(&person.id)),
        ("name".into(), Value::str(&person.name)),
        ("aliases".into(), Value::List(person.aliases.iter().map(|a| Value::str(a)).collect())),
        ("created".into(), Value::str(&person.created)),
        ("updated".into(), Value::str(&person.updated)),
        ("updated_by".into(), Value::str(&person.updated_by)),
        ("rev".into(), Value::Int(person.rev)),
        ("merged_into".into(), Value::opt_str(person.merged_into.as_deref())),
    ];
    entries.extend(person.extra.iter().cloned());
    write_document(&entries, &person.body)
}

pub fn person_content_key(person: &Person) -> String {
    let blank = Person { updated: String::new(), updated_by: String::new(), rev: 0, ..person.clone() };
    sha256_hex(render_person(&blank).as_bytes())
}

pub fn parse_person(text: &str) -> Result<Person, FormatError> {
    if text.len() > MAX_PERSON_BYTES {
        return bad("person file too large");
    }
    let (entries, body) = parse_document(text)?;
    let map = Value::Map(entries.clone());
    let id = req_str(&map, "id")?;
    if !is_ulid(&id) || map.get("quire").and_then(Value::as_int) != Some(FORMAT_LEVEL) {
        return bad("not a version 1 person");
    }
    let aliases = match map.get("aliases") {
        Some(Value::List(items)) => items.iter().map(|a| a.as_str().map(str::to_string).ok_or_else(|| FormatError("alias".into()))).collect::<Result<_, _>>()?,
        _ => return bad("aliases must be a list"),
    };
    Ok(Person {
        name: req_str(&map, "name")?,
        created: req_str(&map, "created")?,
        updated: req_str(&map, "updated")?,
        updated_by: req_str(&map, "updated_by")?,
        rev: map.get("rev").and_then(Value::as_int).ok_or_else(|| FormatError("rev".into()))?,
        merged_into: opt_str_field(&map, "merged_into")?,
        id,
        aliases,
        extra: entries.into_iter().filter(|(k, _)| !PERSON_KEYS.contains(&k.as_str())).collect(),
        body,
    })
}

// ===== Folders (section 9) and the marker (section 5) =====

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Folder {
    pub quire: i64,
    pub id: String,
    pub name: String,
    pub created: String,
    pub updated: String,
    pub updated_by: String,
}

pub fn render_folder(folder: &Folder) -> String {
    let clean = Folder { name: nfc(&folder.name), ..folder.clone() };
    format!("{}\n", serde_json::to_string(&clean).unwrap_or_default())
}

pub fn parse_folder(text: &str) -> Result<Folder, FormatError> {
    if text.len() > MAX_FOLDER_BYTES {
        return bad("folder file too large");
    }
    let folder: Folder = serde_json::from_str(text).map_err(|e| FormatError(format!("folder: {e}")))?;
    if folder.quire != FORMAT_LEVEL || !is_ulid(&folder.id) {
        return bad("not a version 1 folder");
    }
    Ok(folder)
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Marker {
    pub format: String,
    pub version: i64,
    pub min_reader: i64,
    pub vault_id: String,
    pub created: String,
    pub created_by: String,
}

pub fn render_marker(marker: &Marker) -> String {
    format!("{}\n", serde_json::to_string_pretty(marker).unwrap_or_default())
}

pub fn parse_marker(text: &str) -> Result<Marker, FormatError> {
    let marker: Marker = serde_json::from_str(text).map_err(|e| FormatError(format!("marker: {e}")))?;
    if marker.format != "quire-vault" {
        return bad("not a Quire vault");
    }
    Ok(marker)
}

// ===== Folder directory names (section 9) =====

const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2",
    "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

fn trim_name(name: &str) -> &str {
    name.trim_start_matches(|c| c == '.' || c == '_' || c == ' ').trim_end_matches(|c| c == '.' || c == ' ')
}

/// Steps 1 to 5: the directory name for one folder, before collisions.
pub fn folder_dir_name(name: &str) -> String {
    let kept: String = nfc(name).chars().filter(|c| !c.is_control() && !"<>:\"/\\|?*".contains(*c)).collect();
    let mut cleaned = trim_name(&kept).to_string();
    let mut units = 0;
    let mut cut = cleaned.len();
    for (i, c) in cleaned.char_indices() {
        units += c.len_utf16();
        if units > 80 {
            cut = i;
            break;
        }
    }
    cleaned.truncate(cut);
    let mut cleaned = trim_name(&cleaned).to_string();
    if cleaned.is_empty() {
        cleaned = "Folder".to_string();
    }
    // `nul.txt folder` would still be reserved on Windows (its base is `nul`), so the word goes
    // after the base name: `nul folder.txt` (an amendment to step 5)
    let (base, rest) = cleaned.split_once('.').map_or((cleaned.as_str(), None), |(b, r)| (b, Some(r)));
    if RESERVED.contains(&base.to_ascii_uppercase().as_str()) {
        cleaned = match rest {
            Some(rest) => format!("{base} folder.{rest}"),
            None => format!("{base} folder"),
        };
    }
    cleaned
}

/// Case folding for collisions (`Work` and `work` are one directory on Windows and macOS).
pub fn fold(name: &str) -> String {
    nfc(name).chars().flat_map(char::to_lowercase).collect()
}

/// Step 6: directory names for every folder, `(id, name)`. When two cleaned names collide, the
/// folder with the larger ID gets ` (2)`, then ` (3)`, so every device agrees.
pub fn folder_dirs(folders: &[(String, String)]) -> Vec<(String, String)> {
    folder_dirs_avoiding(folders, &[])
}

/// As `folder_dirs`, never using a name in `reserved`: directories another device's folders
/// already hold in the vault (Phase N review: the phone makes folders too).
pub fn folder_dirs_avoiding(folders: &[(String, String)], reserved: &[String]) -> Vec<(String, String)> {
    let mut sorted: Vec<&(String, String)> = folders.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut taken: Vec<String> = reserved.iter().map(|r| fold(r)).collect();
    let mut out = Vec::new();
    for (id, name) in sorted {
        let base = folder_dir_name(name);
        let mut candidate = base.clone();
        let mut n = 1;
        while taken.contains(&fold(&candidate)) {
            n += 1;
            candidate = format!("{base} ({n})");
        }
        taken.push(fold(&candidate));
        out.push((id.clone(), candidate));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_note() -> Note {
        Note {
            id: "01JC90A2T8N4Y6B3D7F1GHKQ5S".into(),
            title: "Weekly sync \"plan\"".into(),
            created: "2026-11-01T14:30:02.000Z".into(),
            updated: "2026-11-01T15:42:10.123Z".into(),
            updated_by: "dev-7f3a91c2".into(),
            rev: 3,
            started: Some("2026-11-01T14:30:02.000Z".into()),
            ended: Some("2026-11-01T15:22:11.000Z".into()),
            duration_s: Some(3129),
            language: None,
            source: "recorded".into(),
            mode: "audio".into(),
            speakers: vec![
                Speaker { slot: 1, name: "Alex Rivera".into(), person: Some("01JC7M9X2R5T8V1B4D6F3HJKQ0".into()) },
                Speaker { slot: 2, name: "Speaker 2".into(), person: None },
            ],
            transcript: Some(TranscriptRef { file: "01JC90A2T8N4Y6B3D7F1GHKQ5S.3b7e09cd.transcript.json".into(), sha256: "3b7e09cd".to_string() + &"0".repeat(56), segments: 2 }),
            summary: Some(SummaryMeta { status: "done".into(), generated_at: Some("2026-11-01T15:25:40.000Z".into()), model: Some("qwen3.5:4b".into()) }),
            audio: AudioMeta { retention: "d7".into(), expires: Some("2026-11-08T15:22:11.000Z".into()), state: "present".into(), deleted_at: None },
            extra: vec![],
            body: "\n## Summary\n\nDecisions.\n".into(),
        }
    }

    #[test]
    fn ulids_encode_time_first_and_validate() {
        let id = ulid(0x0191_2C40_0000, [0; 10]);
        assert_eq!(id.len(), 26);
        assert!(is_ulid(&id));
        assert!(id.ends_with(&"0".repeat(16)));
        assert!(new_ulid(2) < new_ulid(3), "later times sort later");
        assert!(!is_ulid("01JC90A2T8N4Y6B3D7F1GHKQ5I"), "I is not Crockford");
        assert!(!is_ulid("01jc90a2t8n4y6b3d7f1ghkq5s"));
    }

    #[test]
    fn a_note_matches_the_documented_layout_and_reads_back_byte_identical() {
        let note = sample_note();
        let text = render_note(&note);
        assert!(text.starts_with("---\nquire: 1\nid: \"01JC90A2T8N4Y6B3D7F1GHKQ5S\"\ntitle: \"Weekly sync \\\"plan\\\"\"\n"));
        assert!(text.contains("speakers:\n  - slot: 1\n    name: \"Alex Rivera\"\n    person: \"01JC7M9X2R5T8V1B4D6F3HJKQ0\"\n  - slot: 2\n"));
        assert!(text.contains("audio:\n  retention: \"d7\"\n  expires: \"2026-11-08T15:22:11.000Z\"\n  state: \"present\"\n  deleted_at: null\n  file: null\n"));
        let back = parse_note(&text).unwrap();
        assert_eq!(back, note);
        assert_eq!(render_note(&back), text);
    }

    #[test]
    fn unknown_keys_survive_a_rewrite() {
        let mut text = render_note(&sample_note());
        text = text.replacen("---\n\n## Summary", "phone_only:\n  pinned: true\n  tags: []\n---\n\n## Summary", 1);
        let note = parse_note(&text).unwrap();
        assert_eq!(note.extra.len(), 1);
        assert_eq!(render_note(&note), text);
    }

    #[test]
    fn the_content_key_ignores_bookkeeping_only() {
        let a = sample_note();
        let b = Note { rev: 9, updated: "2027-01-01T00:00:00.000Z".into(), updated_by: "dev-00000000".into(), ..a.clone() };
        assert_eq!(note_content_key(&a), note_content_key(&b));
        let c = Note { title: "Other".into(), ..a.clone() };
        assert_ne!(note_content_key(&a), note_content_key(&c));
    }

    #[test]
    fn the_grammar_rejects_everything_outside_the_subset() {
        let ok = "---\nquire: 1\n---\n";
        assert!(parse_document(ok).is_ok());
        for bad_doc in [
            "---\na: 1\na: 2\n---\n",               // duplicate key
            "---\na: \"x\\/y\"\n---\n",            // escape not allowed
            "---\na: &anchor 1\n---\n",            // anchor
            "---\na: !!str 1\n---\n",              // tag
            "---\na:\t1\n---\n",                   // tab
            "---\na: [1, 2]\n---\n",               // flow style
            "---\na: 1.5\n---\n",                  // float
            "---\na: yes\n---\n",                  // bare word
            "---\na:\n   b: 1\n---\n",             // three spaces
            "---\na: 1\n",                         // not closed
            "---\nA: 1\n---\n",                    // key case
            "---\na: 1 # comment\n---\n",          // comment
            "---\na: |\n  text\n---\n",            // multi-line string
            "---\na: \"unterminated\n---\n",
        ] {
            assert!(parse_document(bad_doc).is_err(), "{bad_doc:?} must be rejected");
        }
    }

    #[test]
    fn a_byte_order_mark_and_crlf_are_accepted() {
        let (entries, body) = parse_document("\u{feff}---\r\nquire: 1\r\n---\r\nbody\r\n").unwrap();
        assert_eq!(entries, vec![("quire".to_string(), Value::Int(1))]);
        assert_eq!(body, "body\r\n", "the body is kept exactly");
    }

    #[test]
    fn sidecars_are_sorted_stable_and_content_addressed() {
        let segs = vec![
            Segment { id: "seg-2".into(), start: 16.2, end: 21.0, speaker: Some(2), text: "Two weeks.".into() },
            Segment { id: "seg-1".into(), start: 12.4, end: 15.9, speaker: None, text: "Let's start.".into() },
        ];
        let text = render_sidecar("01JC90A2T8N4Y6B3D7F1GHKQ5S", &segs);
        assert!(text.contains("\"segments\": [\n    {\"id\":\"seg-1\",\"start\":12.4,\"end\":15.9,\"speaker\":null,\"text\":\"Let's start.\"},\n    {\"id\":\"seg-2\""));
        let (id, back) = parse_sidecar(&text).unwrap();
        assert_eq!(id, "01JC90A2T8N4Y6B3D7F1GHKQ5S");
        assert_eq!(render_sidecar(&id, &back), text);
        let sha = sha256_hex(text.as_bytes());
        assert_eq!(sidecar_name(&id, &sha), format!("01JC90A2T8N4Y6B3D7F1GHKQ5S.{}.transcript.json", &sha[..8]));
        assert_eq!(render_sidecar(&id, &[]), "{\n  \"quire\": 1,\n  \"note_id\": \"01JC90A2T8N4Y6B3D7F1GHKQ5S\",\n  \"segments\": []\n}\n");
    }

    #[test]
    fn people_folders_and_the_marker_round_trip() {
        let person = Person {
            id: "01JC7M9X2R5T8V1B4D6F3HJKQ0".into(),
            name: "Alex Rivera".into(),
            aliases: vec![],
            created: "2026-10-27T18:00:00.000Z".into(),
            updated: "2026-10-31T09:12:00.000Z".into(),
            updated_by: "dev-7f3a91c2".into(),
            rev: 2,
            merged_into: None,
            extra: vec![],
            body: "\nNotes.\n".into(),
        };
        let text = render_person(&person);
        assert!(text.contains("aliases: []\n"));
        assert_eq!(parse_person(&text).unwrap(), person);
        let folder = Folder { quire: 1, id: "01JC5K3N7P9R2T4V6X8Z0BDFGH".into(), name: "Work Meetings".into(), created: "a".into(), updated: "a".into(), updated_by: "dev-1".into() };
        assert_eq!(parse_folder(&render_folder(&folder)).unwrap(), folder);
        let marker = Marker { format: "quire-vault".into(), version: 1, min_reader: 1, vault_id: "01JC8Z0000VAULT00000000000".into(), created: "a".into(), created_by: "dev-1".into() };
        assert_eq!(parse_marker(&render_marker(&marker)).unwrap(), marker);
    }

    #[test]
    fn folder_directory_names_follow_the_windows_rules() {
        assert_eq!(folder_dir_name("  ..Work: Q1/Q2?  "), "Work Q1Q2");
        assert_eq!(folder_dir_name("_private"), "private");
        assert_eq!(folder_dir_name("Notes."), "Notes");
        assert_eq!(folder_dir_name("CON"), "CON folder");
        assert_eq!(folder_dir_name("nul.txt"), "nul folder.txt");
        assert_eq!(folder_dir_name("..."), "Folder");
        assert_eq!(folder_dir_name("e\u{301}t\u{e9}"), "\u{e9}t\u{e9}", "NFC");
        let long = "😀".repeat(50);
        let cut = folder_dir_name(&long);
        assert_eq!(cut.encode_utf16().count(), 80, "80 UTF-16 units at a scalar boundary");
        let dirs = folder_dirs(&[
            ("01JC5K3N7P9R2T4V6X8Z0BDFGJ".into(), "work".into()),
            ("01JC5K3N7P9R2T4V6X8Z0BDFGH".into(), "Work".into()),
            ("01JC5K3N7P9R2T4V6X8Z0BDFGK".into(), "Work?".into()),
        ]);
        assert_eq!(dirs[0], ("01JC5K3N7P9R2T4V6X8Z0BDFGH".to_string(), "Work".to_string()), "smallest ID keeps the name");
        assert_eq!(dirs[1].1, "work (2)");
        assert_eq!(dirs[2].1, "Work (3)");
    }

    // ===== Conformance fixtures (`docs/vault-fixtures`, plan PE6) =====

    fn fixtures_dir() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/vault-fixtures")
    }

    /// The fictional canonical vault: two folders that collide by case, a filed and an unfiled
    /// note, one person, the marker.
    fn canonical_files() -> Vec<(String, String)> {
        let device = "dev-0a1b2c3d";
        let folder_a = Folder { quire: 1, id: "01JC5K3N7P9R2T4V6X8Z0BDFGH".into(), name: "Garden Club".into(), created: "2026-10-01T09:00:00.000Z".into(), updated: "2026-10-01T09:00:00.000Z".into(), updated_by: device.into() };
        let folder_b = Folder { id: "01JC5K3N7P9R2T4V6X8Z0BDFGJ".into(), name: "garden club".into(), ..folder_a.clone() };
        let person = Person {
            id: "01JC7M9X2R5T8V1B4D6F3HJKQ0".into(),
            name: "Alex Rivera".into(),
            aliases: vec!["Alex R".into()],
            created: "2026-10-01T09:00:00.000Z".into(),
            updated: "2026-10-02T10:00:00.000Z".into(),
            updated_by: device.into(),
            rev: 2,
            merged_into: None,
            extra: vec![],
            body: "\nFictional person for the shared fixtures.\n".into(),
        };
        let note_id = "01JC90A2T8N4Y6B3D7F1GHKQ5S";
        let segments = vec![
            Segment { id: "seg-1".into(), start: 0.0, end: 4.5, speaker: Some(1), text: "Welcome to the garden club budget meeting.".into() },
            Segment { id: "seg-2".into(), start: 4.5, end: 9.25, speaker: Some(2), text: "We have \"three\" plots left, and one bench.".into() },
        ];
        let sidecar = render_sidecar(note_id, &segments);
        let sha = sha256_hex(sidecar.as_bytes());
        let filed = Note {
            id: note_id.into(),
            title: "Garden club budget".into(),
            created: "2026-10-03T18:00:00.000Z".into(),
            updated: "2026-10-03T19:00:00.000Z".into(),
            updated_by: device.into(),
            rev: 1,
            started: Some("2026-10-03T18:00:00.000Z".into()),
            ended: Some("2026-10-03T18:00:09.250Z".into()),
            duration_s: Some(9),
            language: None,
            source: "recorded".into(),
            mode: "audio".into(),
            speakers: vec![
                Speaker { slot: 1, name: "Alex Rivera".into(), person: Some(person.id.clone()) },
                Speaker { slot: 2, name: "Speaker 2".into(), person: None },
            ],
            transcript: Some(TranscriptRef { file: sidecar_name(note_id, &sha), sha256: sha.clone(), segments: 2 }),
            summary: Some(SummaryMeta { status: "done".into(), generated_at: Some("2026-10-03T19:00:00.000Z".into()), model: Some("qwen3.5:4b".into()) }),
            audio: AudioMeta { retention: "d7".into(), expires: Some("2026-10-10T18:00:09.250Z".into()), state: "present".into(), deleted_at: None },
            extra: vec![],
            body: "\n## Summary\n\n- Buy one bench.\n\n## Notes\n\nAsk about the water tap.\n".into(),
        };
        let typed = Note {
            id: "01JC9B00000000000000000000".into(),
            title: "Ideas for spring".into(),
            created: "2026-10-04T08:00:00.000Z".into(),
            updated: "2026-10-04T08:05:00.000Z".into(),
            rev: 1,
            started: None,
            ended: None,
            duration_s: None,
            source: "typed".into(),
            mode: "transcript_only".into(),
            speakers: vec![],
            transcript: None,
            summary: None,
            audio: AudioMeta { retention: "keep".into(), expires: None, state: "none".into(), deleted_at: None },
            body: "\n## Notes\n\nPlant tulips.\n".into(),
            ..filed.clone()
        };
        let dirs = folder_dirs(&[(folder_a.id.clone(), folder_a.name.clone()), (folder_b.id.clone(), folder_b.name.clone())]);
        let marker = Marker { format: "quire-vault".into(), version: 1, min_reader: 1, vault_id: "01JC8Z00000000000000000000".into(), created: "2026-10-01T09:00:00.000Z".into(), created_by: device.into() };
        vec![
            ("quire-format.json".into(), render_marker(&marker)),
            (format!("{}/_folder.json", dirs[0].1), render_folder(&folder_a)),
            (format!("{}/_folder.json", dirs[1].1), render_folder(&folder_b)),
            (format!("{}/{}.md", dirs[0].1, note_id), render_note(&filed)),
            (format!("{}/{}", dirs[0].1, sidecar_name(note_id, &sha)), sidecar),
            (format!("{}.md", typed.id), render_note(&typed)),
            (format!("_people/{}.md", person.id), render_person(&person)),
        ]
    }

    /// Run with `QUIRE_WRITE_FIXTURES=1` to regenerate the fixtures after a deliberate change.
    #[test]
    fn the_shared_fixtures_are_canonical_and_read_back_byte_identical() {
        let root = fixtures_dir().join("canonical");
        let files = canonical_files();
        if std::env::var("QUIRE_WRITE_FIXTURES").is_ok() {
            let _ = std::fs::remove_dir_all(&root);
            for (rel, text) in &files {
                let path = root.join(rel);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, text).unwrap();
            }
        }
        for (rel, text) in &files {
            let on_disk = std::fs::read_to_string(root.join(rel)).unwrap_or_else(|_| panic!("fixture {rel} is missing"));
            assert_eq!(&on_disk, text, "{rel} is not what the canonical writer produces");
            let rewritten = if rel.ends_with("transcript.json") {
                let (id, segs) = parse_sidecar(&on_disk).unwrap();
                render_sidecar(&id, &segs)
            } else if rel.ends_with("_folder.json") {
                render_folder(&parse_folder(&on_disk).unwrap())
            } else if rel == "quire-format.json" {
                render_marker(&parse_marker(&on_disk).unwrap())
            } else if rel.starts_with("_people/") {
                render_person(&parse_person(&on_disk).unwrap())
            } else {
                let note = parse_note(&on_disk).unwrap();
                if let Some(t) = &note.transcript {
                    let dir = std::path::Path::new(rel).parent().unwrap();
                    let side = std::fs::read(root.join(dir).join(&t.file)).unwrap();
                    assert_eq!(sha256_hex(&side), t.sha256, "the note's commit point matches its sidecar");
                    assert_eq!(t.file, sidecar_name(&note.id, &t.sha256));
                }
                render_note(&note)
            };
            assert_eq!(&rewritten, &on_disk, "{rel} does not round trip");
        }
    }

    /// Every file in `docs/vault-fixtures/grammar/invalid` must be rejected by the note reader.
    #[test]
    fn the_shared_grammar_fixtures_are_rejected() {
        let dir = fixtures_dir().join("grammar/invalid");
        let mut seen = 0;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let text = std::fs::read_to_string(&path).unwrap();
            assert!(parse_note(&text).is_err(), "{} must be rejected", path.display());
            seen += 1;
        }
        assert!(seen >= 10);
    }

    /// `docs/vault-fixtures/names.json`: display names and the directory names they become.
    #[test]
    fn the_shared_folder_name_fixtures_hold() {
        let text = std::fs::read_to_string(fixtures_dir().join("names.json")).unwrap();
        let cases: Vec<(String, String)> = serde_json::from_str(&text).unwrap();
        assert!(cases.len() >= 10);
        for (name, want) in cases {
            assert_eq!(folder_dir_name(&name), want, "{name:?}");
        }
    }
}
