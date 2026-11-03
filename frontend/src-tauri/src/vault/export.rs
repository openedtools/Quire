//! The one-way export from the database to the vault (spec F-SYNC-4, `sync-format.md` section
//! 16). Plans every file from the database, writes only what changed (sidecar before note,
//! atomically, staged in `_tmp`), never overwrites a file that changed in the vault, records
//! deletions as tombstones, then reads the vault back and records what was written only when
//! every file matches.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use chrono::Utc;
use log::warn;
use serde::Serialize;
use sqlx::SqlitePool;

use super::format::{self, AudioMeta, Folder, Marker, Note, Person, Segment, Speaker, SummaryMeta, TranscriptRef};
use super::store::{self, NoteSource, StateRow};

/// Longest vault root path the format allows, and the point where Settings warns (section 15).
pub const ROOT_REFUSE_CHARS: usize = 120;
pub const ROOT_WARN_CHARS: usize = 100;
/// Unreferenced sidecars older than this may be removed (section 7).
const SIDECAR_GRACE_DAYS: i64 = 7;
const MARKER: &str = "quire-format.json";

/// Counts from one run; never titles or paths, so it is safe to log and show.
#[derive(Debug, Default, Clone, PartialEq, Serialize, serde::Deserialize)]
pub struct ExportReport {
    pub written: usize,
    pub unchanged: usize,
    /// Files changed in the vault since the app wrote them: left alone
    pub changed_in_vault: usize,
    pub moved: usize,
    pub deleted: usize,
    pub failed: usize,
    pub notes: usize,
    pub people: usize,
    pub folders: usize,
    /// The vault read back exactly as planned, so the run was recorded
    pub verified: bool,
}

#[derive(Debug, Clone)]
struct Planned {
    kind: &'static str,
    sync_id: String,
    rel: String,
    bytes: String,
    content_key: Option<String>,
    rev: Option<i64>,
    updated: Option<String>,
}

/// Why a vault folder cannot be used. The text is shown in Settings and holds no path.
pub fn check_root(folder: &str) -> Result<PathBuf, String> {
    let length = folder.trim().chars().count();
    if length > ROOT_REFUSE_CHARS {
        return Err(format!(
            "That folder's path is {length} characters long. Choose one under {ROOT_REFUSE_CHARS}, so every file stays within the Windows path limit."
        ));
    }
    crate::markdown_export::writer::open_root(folder).map_err(|e| e.to_string())
}

// ===== Building the files from the database =====

/// Speaker slots in order of first appearance, then speakers with no lines (section 6).
fn slots(source: &NoteSource) -> Vec<(String, i64)> {
    let mut keys: Vec<String> = Vec::new();
    for (_, key) in &source.lines {
        if let Some(k) = key {
            if !keys.contains(k) {
                keys.push(k.clone());
            }
        }
    }
    for s in &source.speakers {
        if !keys.contains(&s.key) {
            keys.push(s.key.clone());
        }
    }
    keys.into_iter().enumerate().map(|(i, k)| (k, i as i64 + 1)).collect()
}

fn with_newline(text: &str) -> String {
    if text.ends_with('\n') {
        text.to_string()
    } else {
        format!("{text}\n")
    }
}

/// `## Summary` with the generated text, then `## Notes` with the person's own notes, each
/// exactly as stored (section 6).
pub fn note_body(summary: Option<&str>, notes: Option<&str>) -> String {
    let mut body = String::new();
    if let Some(s) = summary.filter(|s| !s.trim().is_empty()) {
        body.push_str(&format!("\n## Summary\n\n{}", with_newline(s)));
    }
    if let Some(n) = notes.filter(|n| !n.trim().is_empty()) {
        body.push_str(&format!("\n## Notes\n\n{}", with_newline(n)));
    }
    body
}

const RETENTIONS: &[&str] = &["off", "h24", "d7", "d14", "d30", "keep"];

/// The note and its sidecar for one meeting, with `rev` and `updated` still to be decided.
fn build_note(source: &NoteSource, device: &str) -> (Note, String) {
    let slots = slots(source);
    let slot_of = |key: &Option<String>| key.as_ref().and_then(|k| slots.iter().find(|(s, _)| s == k).map(|(_, n)| *n));
    let segments: Vec<Segment> = source.lines.iter().map(|(seg, key)| Segment { speaker: slot_of(key), ..seg.clone() }).collect();
    let sidecar = format::render_sidecar(&source.sync_id, &segments);
    let sha = format::sha256_hex(sidecar.as_bytes());
    let speakers = slots
        .iter()
        .map(|(key, slot)| {
            let known = source.speakers.iter().find(|s| &s.key == key);
            Speaker {
                slot: *slot,
                name: known.map(|s| s.name.clone()).unwrap_or_else(|| format!("Speaker {slot}")),
                person: known.and_then(|s| s.person_sync_id.clone()),
            }
        })
        .collect();
    let recorded = !segments.is_empty();
    let end = segments.iter().map(|s| s.end).fold(0.0_f64, f64::max);
    let ended = recorded
        .then(|| store::stored_time(&source.created).map(|t| format::timestamp(t + chrono::Duration::milliseconds((end * 1000.0).round() as i64))))
        .flatten();
    let retention = source.retention.as_deref().filter(|r| RETENTIONS.contains(r)).unwrap_or("keep").to_string();
    let state = match source.audio_state.as_str() {
        "present" | "deleted" => source.audio_state.clone(),
        _ => "none".to_string(),
    };
    let expires = (retention != "keep" && state == "present").then(|| source.expires.clone()).flatten();
    let note = Note {
        id: source.sync_id.clone(),
        title: source.title.clone(),
        created: source.created.clone(),
        updated: source.updated.clone(),
        updated_by: device.to_string(),
        rev: 1,
        started: recorded.then(|| source.created.clone()),
        ended,
        duration_s: recorded.then(|| end.round() as i64),
        language: None,
        source: if recorded { "recorded" } else { "typed" }.to_string(),
        mode: if source.mode == "transcript_only" { "transcript_only" } else { "audio" }.to_string(),
        speakers,
        transcript: recorded.then(|| TranscriptRef { file: format::sidecar_name(&source.sync_id, &sha), sha256: sha.clone(), segments: segments.len() as i64 }),
        summary: source.summary_status.as_ref().map(|status| SummaryMeta {
            status: status.clone(),
            generated_at: source.summary_at.clone(),
            model: source.summary_model.clone(),
        }),
        audio: AudioMeta { retention, expires, state: state.clone(), deleted_at: (state == "deleted").then(|| source.audio_deleted_at.clone()).flatten() },
        // Written after `audio`, where an unknown key goes, so the phone reads it the same way
        extra: source.kind.clone().map(|k| vec![("kind".to_string(), format::Value::Str(k))]).unwrap_or_default(),
        body: note_body(source.summary_markdown.as_deref(), source.notes_markdown.as_deref()),
    };
    (note, if recorded { sidecar } else { String::new() })
}

/// What the app last knew of a file: a pending write from a run that did not finish (newer),
/// otherwise the verified record. Paths compare after case folding, as Windows and macOS do.
struct Records {
    rows: Vec<StateRow>,
}

impl Records {
    fn for_id(&self, kind: &str, id: &str) -> Option<&StateRow> {
        self.rows.iter().find(|r| r.kind == kind && r.sync_id == id)
    }
    fn at(&self, rel: &str) -> Vec<&StateRow> {
        let folded = format::fold(rel);
        self.rows.iter().filter(|r| format::fold(&r.rel_path) == folded).collect()
    }
}

/// `(rev, updated, content key)` of a note or person file this device wrote, for when the app
/// has no record of it (a folder chosen again, say), so `rev` never goes back.
fn self_described(root: &Path, rel: &str, kind: &str, id: &str, device: &str) -> Option<(i64, String, String)> {
    let text = String::from_utf8(read_owned(root, rel)?).ok()?;
    match kind {
        "note" => format::parse_note(&text).ok().filter(|n| n.id == id && n.updated_by == device).map(|n| (n.rev, n.updated.clone(), format::note_content_key(&n))),
        "person" => format::parse_person(&text).ok().filter(|p| p.id == id && p.updated_by == device).map(|p| (p.rev, p.updated.clone(), format::person_content_key(&p))),
        "folder" => format::parse_folder(&text).ok().filter(|f| f.id == id && f.updated_by == device).map(|_| (0, String::new(), String::new())),
        _ => None,
    }
}

/// `rev` and `updated` stay while the canonical content stays (section 11); a change bumps
/// `rev` and stamps `updated` with now.
fn bookkeeping(previous: Option<(i64, String, String)>, key: &str, first_updated: &str) -> (i64, String) {
    match previous {
        Some((rev, updated, k)) if k == key && !updated.is_empty() => (rev.max(1), updated),
        Some((rev, _, _)) => (rev + 1, format::timestamp(Utc::now())),
        None => (1, first_updated.to_string()),
    }
}

/// Top-level directories holding another device's folder (its `_folder.json` has an ID this app
/// does not know), so this app's folders never write into them.
fn foreign_dirs(root: &Path, ours: &HashSet<&str>) -> Vec<String> {
    let Ok(entries) = fs::read_dir(root) else { return vec![] };
    entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir() && !t.is_symlink()))
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('_') || name.starts_with('.') {
                return None;
            }
            let text = fs::read_to_string(e.path().join("_folder.json")).ok()?;
            let folder = format::parse_folder(&text).ok()?;
            (!ours.contains(folder.id.as_str())).then_some(name)
        })
        .collect()
}

fn previous_of(records: &Records, root: &Path, kind: &str, id: &str, rel: &str, device: &str) -> Option<(i64, String, String)> {
    match records.for_id(kind, id) {
        Some(r) => Some((r.rev.unwrap_or(0), r.updated.clone().unwrap_or_default(), r.content_key.clone().unwrap_or_default())),
        None => self_described(root, rel, kind, id, device),
    }
}

async fn plan(pool: &SqlitePool, root: &Path, device: &str, records: &Records) -> Result<Vec<Planned>, sqlx::Error> {
    let mut out = Vec::new();

    let folders = store::folders(pool).await?;
    let ours: HashSet<&str> = folders.iter().map(|f| f.sync_id.as_str()).collect();
    let dirs: HashMap<String, String> = format::folder_dirs_avoiding(
        &folders.iter().map(|f| (f.sync_id.clone(), f.name.clone())).collect::<Vec<_>>(),
        &foreign_dirs(root, &ours),
    )
    .into_iter()
    .collect();
    for f in &folders {
        let folder = Folder { quire: format::FORMAT_LEVEL, id: f.sync_id.clone(), name: f.name.clone(), created: f.created.clone(), updated: f.updated.clone(), updated_by: device.into() };
        out.push(Planned { kind: "folder", sync_id: f.sync_id.clone(), rel: format!("{}/_folder.json", dirs[&f.sync_id]), bytes: format::render_folder(&folder), content_key: None, rev: None, updated: None });
    }

    for p in store::people(pool).await? {
        let mut person = Person {
            id: p.sync_id.clone(),
            name: p.name.clone(),
            aliases: vec![],
            created: p.created.clone(),
            updated: String::new(),
            updated_by: device.into(),
            rev: 0,
            merged_into: None,
            extra: vec![],
            body: p.notes.as_deref().filter(|n| !n.trim().is_empty()).map(|n| format!("\n{}", with_newline(n))).unwrap_or_default(),
        };
        let rel = format!("_people/{}.md", p.sync_id);
        let key = format::person_content_key(&person);
        let (rev, updated) = bookkeeping(previous_of(records, root, "person", &p.sync_id, &rel, device), &key, &p.updated);
        person.rev = rev;
        person.updated = updated.clone();
        out.push(Planned { kind: "person", sync_id: p.sync_id.clone(), rel, bytes: format::render_person(&person), content_key: Some(key), rev: Some(rev), updated: Some(updated) });
    }

    for meeting_id in store::meeting_ids(pool).await? {
        let Some(source) = store::note_source(pool, &meeting_id).await? else { continue };
        let (mut note, sidecar) = build_note(&source, device);
        // The desktop's own ID, kept for the way back (sync-format.md 16.2)
        note.extra.push(("legacy_id".into(), format::Value::str(&meeting_id)));
        let dir = source.folder_sync_id.as_ref().and_then(|f| dirs.get(f)).map(|d| format!("{d}/")).unwrap_or_default();
        if let Some(t) = &note.transcript {
            out.push(Planned { kind: "sidecar", sync_id: note.id.clone(), rel: format!("{dir}{}", t.file), bytes: sidecar, content_key: None, rev: None, updated: None });
        }
        let rel = format!("{dir}{}.md", note.id);
        let key = format::note_content_key(&note);
        let (rev, updated) = bookkeeping(previous_of(records, root, "note", &note.id, &rel, device), &key, &source.updated);
        note.rev = rev;
        note.updated = updated.clone();
        out.push(Planned { kind: "note", sync_id: note.id.clone(), rel, bytes: format::render_note(&note), content_key: Some(key), rev: Some(rev), updated: Some(updated) });
    }
    Ok(out)
}

// ===== Files =====

fn components(rel: &str) -> Option<Vec<&str>> {
    let parts: Vec<&str> = rel.split('/').collect();
    let ok = parts.iter().all(|p| !p.is_empty() && *p != "." && *p != ".." && !p.contains('\\') && !p.contains(':'));
    ok.then_some(parts)
}

/// A real file (not a link) at `rel` inside the vault, read with a size cap.
pub(crate) fn read_owned(root: &Path, rel: &str) -> Option<Vec<u8>> {
    let parts = components(rel)?;
    let mut path = root.to_path_buf();
    for (i, part) in parts.iter().enumerate() {
        path.push(part);
        let meta = fs::symlink_metadata(&path).ok()?;
        let last = i + 1 == parts.len();
        if meta.file_type().is_symlink() || (last && !meta.is_file()) || (!last && !meta.is_dir()) {
            return None;
        }
    }
    let meta = fs::symlink_metadata(&path).ok()?;
    if meta.len() as usize > format::MAX_SIDECAR_BYTES {
        return None;
    }
    fs::read(&path).ok()
}

fn exists(root: &Path, rel: &str) -> bool {
    components(rel).map_or(false, |parts| fs::symlink_metadata(parts.iter().fold(root.to_path_buf(), |p, c| p.join(c))).is_ok())
}

/// Creates the folders for `rel` inside the vault; refuses a link in place of a folder.
fn ensure_parent(root: &Path, rel: &str) -> io::Result<PathBuf> {
    let parts = components(rel).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "bad path"))?;
    let mut dir = root.to_path_buf();
    for part in &parts[..parts.len() - 1] {
        dir.push(part);
        match fs::symlink_metadata(&dir) {
            Ok(m) if m.is_dir() && !m.file_type().is_symlink() => {}
            Ok(_) => return Err(io::Error::new(io::ErrorKind::AlreadyExists, "not a folder")),
            Err(e) if e.kind() == io::ErrorKind::NotFound => fs::create_dir(&dir)?,
            Err(e) => return Err(e),
        }
    }
    Ok(dir.join(parts[parts.len() - 1]))
}

/// Writes through `_tmp/<random>.part` and renames into place, so a reader never sees half a
/// file and the staging name is one every reader ignores (section 15).
fn write_atomic(root: &Path, rel: &str, bytes: &[u8]) -> io::Result<()> {
    let target = ensure_parent(root, rel)?;
    let staging_rel = format!("_tmp/{:016x}.part", rand::random::<u64>());
    let staging = ensure_parent(root, &staging_rel)?;
    let result = (|| {
        let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&staging)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&staging, &target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&staging);
    }
    result
}

fn remove_owned(root: &Path, rel: &str) -> io::Result<()> {
    if read_owned(root, rel).is_none() {
        return Ok(());
    }
    let parts = components(rel).unwrap_or_default();
    fs::remove_file(parts.iter().fold(root.to_path_buf(), |p, c| p.join(c)))
}

/// Removes an app folder once nothing but its `_folder.json` is gone from it (section 9).
fn remove_empty_dir(root: &Path, dir: &str) {
    let Some(parts) = components(dir) else { return };
    let path = parts.iter().fold(root.to_path_buf(), |p, c| p.join(c));
    if let Ok(meta) = fs::symlink_metadata(&path) {
        if meta.is_dir() && !meta.file_type().is_symlink() {
            let _ = fs::remove_dir(&path);
        }
    }
}

#[derive(Serialize)]
struct Tombstone<'a> {
    id: &'a str,
    deleted_at: String,
    deleted_by: &'a str,
    /// The vault ID of the folder the note was in, or null at the root
    folder_at_deletion: Option<String>,
}

fn row(kind: &str, sync_id: &str, rel: &str, bytes: &[u8], now: &str) -> StateRow {
    StateRow { sync_id: sync_id.into(), kind: kind.into(), rel_path: rel.into(), sha256: format::sha256_hex(bytes), content_key: None, rev: None, updated: None, exported_at: now.into() }
}

/// Moves a deleted note or person into `_trash/<id>/` with its tombstone (section 12). Returns
/// the records of the two new files, or None when the file is not the app's to move.
async fn tombstone(pool: &SqlitePool, root: &Path, old: &StateRow, folder_id: Option<String>, device: &str, now: &str) -> io::Result<Option<Vec<StateRow>>> {
    let Some(bytes) = read_owned(root, &old.rel_path) else { return Ok(None) };
    if format::sha256_hex(&bytes) != old.sha256 {
        return Ok(None);
    }
    let name = old.rel_path.rsplit('/').next().unwrap_or_default();
    let content_rel = format!("_trash/{}/{}", old.sync_id, name);
    let stone_rel = format!("_trash/{}/_deleted.json", old.sync_id);
    // Never write over something already in _trash that is not this same file
    if exists(root, &content_rel) && read_owned(root, &content_rel).as_deref() != Some(&bytes[..]) {
        return Ok(None);
    }
    let stone = Tombstone { id: &old.sync_id, deleted_at: now.to_string(), deleted_by: device, folder_at_deletion: folder_id };
    let stone_bytes = format!("{}\n", serde_json::to_string(&stone).unwrap_or_default());
    let content = row("trash", &old.sync_id, &content_rel, &bytes, now);
    let marker = row("tombstone", &old.sync_id, &stone_rel, stone_bytes.as_bytes(), now);
    store::put_pending(pool, &content).await.map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
    store::put_pending(pool, &marker).await.map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
    write_atomic(root, &content_rel, &bytes)?;
    if !exists(root, &stone_rel) {
        write_atomic(root, &stone_rel, stone_bytes.as_bytes())?;
    }
    remove_owned(root, &old.rel_path)?;
    Ok(Some(vec![content, marker]))
}

/// Reads every planned file back: its bytes, its grammar, and each note's link to its sidecar.
pub(crate) fn verify(root: &Path, files: &[(String, String, &'static str)]) -> Result<(), String> {
    let bytes_of: HashMap<&str, &str> = files.iter().map(|(rel, bytes, _)| (rel.as_str(), bytes.as_str())).collect();
    for (rel, bytes, kind) in files {
        let on_disk = read_owned(root, rel).ok_or_else(|| format!("a {kind} file could not be read back"))?;
        if on_disk != bytes.as_bytes() {
            return Err(format!("a {kind} file does not match what was written"));
        }
        let text = String::from_utf8(on_disk).map_err(|_| format!("a {kind} file is not UTF-8"))?;
        // Errors name the kind only: a parser message can quote a value
        let parsed = match *kind {
            "note" => format::parse_note(&text).map(|note| {
                note.transcript.map_or(true, |t| {
                    let dir = rel.rsplit_once('/').map(|(d, _)| format!("{d}/")).unwrap_or_default();
                    bytes_of.get(format!("{dir}{}", t.file).as_str()).map(|s| format::sha256_hex(s.as_bytes())) == Some(t.sha256)
                })
            }),
            "sidecar" => format::parse_sidecar(&text).map(|_| true),
            "person" => format::parse_person(&text).map(|_| true),
            "folder" => format::parse_folder(&text).map(|_| true),
            _ => format::parse_marker(&text).map(|_| true),
        };
        match parsed {
            Ok(true) => {}
            Ok(false) => return Err("a note does not point at its transcript".into()),
            Err(_) => return Err(format!("a {kind} file does not parse")),
        }
    }
    Ok(())
}

/// A folder renamed only in letter case: rename its directory, through `_tmp`, because on
/// Windows and macOS the two names are the same directory (review finding B2).
fn rename_case_only(root: &Path, from_dir: &str, to_dir: &str) -> io::Result<()> {
    let (Some(from), Some(to)) = (components(from_dir), components(to_dir)) else { return Ok(()) };
    let from = from.iter().fold(root.to_path_buf(), |p, c| p.join(c));
    let to = to.iter().fold(root.to_path_buf(), |p, c| p.join(c));
    match fs::symlink_metadata(&from) {
        Ok(m) if m.is_dir() && !m.file_type().is_symlink() => {}
        _ => return Ok(()),
    }
    let staging = ensure_parent(root, &format!("_tmp/{:016x}", rand::random::<u64>()))?;
    fs::rename(&from, &staging)?;
    fs::rename(&staging, &to)
}

// ===== The run =====

async fn still_on(pool: &SqlitePool, folder: &str) -> bool {
    store::settings(pool).await.map_or(false, |s| s.enabled && s.folder.as_deref() == Some(folder))
}

/// Exports the whole database to the vault at `folder`. Only what changed is written.
pub async fn export(pool: &SqlitePool, folder: &str) -> Result<ExportReport, String> {
    let root = check_root(folder)?;
    let db = |e: sqlx::Error| {
        warn!("Vault export: database error: {}", e);
        "Could not read the meetings to export".to_string()
    };
    let device = store::device_id(pool).await.map_err(db)?;
    let settings = store::settings(pool).await.map_err(db)?;
    let now = format::timestamp(Utc::now());
    // A run for the vault in Settings stops if it is turned off or moved meanwhile
    let configured = settings.enabled && settings.folder.as_deref() == Some(folder);

    // The marker: adopt an existing vault, refuse a folder that is something else (section 5)
    let mut verified_files: Vec<(String, String, &'static str)> = Vec::new();
    match read_owned(&root, MARKER) {
        Some(bytes) => {
            let marker = String::from_utf8(bytes).ok().and_then(|t| format::parse_marker(&t).ok());
            let Some(marker) = marker else {
                return Err("That folder holds a quire-format.json that is not a Quire vault. Choose another folder.".into());
            };
            if marker.min_reader > format::FORMAT_LEVEL {
                return Err("This vault was made by a newer version of Quire. Update the app to write to it.".into());
            }
            if settings.vault_id.as_deref() != Some(marker.vault_id.as_str()) {
                store::set_vault_id(pool, &marker.vault_id).await.map_err(db)?;
            }
        }
        None if exists(&root, MARKER) => return Err("That folder's quire-format.json cannot be read. Choose another folder.".into()),
        None => {
            let vault_id = settings.vault_id.clone().unwrap_or_else(|| format::new_ulid(Utc::now().timestamp_millis() as u64));
            let marker = Marker { format: "quire-vault".into(), version: 1, min_reader: 1, vault_id: vault_id.clone(), created: now.clone(), created_by: device.clone() };
            let text = format::render_marker(&marker);
            write_atomic(&root, MARKER, text.as_bytes()).map_err(|e| format!("Could not write to the vault folder ({:?})", e.kind()))?;
            store::set_vault_id(pool, &vault_id).await.map_err(db)?;
            verified_files.push((MARKER.to_string(), text, "marker"));
        }
    }

    store::assign_sync_ids(pool).await.map_err(db)?;
    let state = store::state(pool).await.map_err(db)?;
    let mut rows = store::pending(pool).await.map_err(db)?;
    rows.extend(state.iter().cloned());
    let records = Records { rows };
    let planned = plan(pool, &root, &device, &records).await.map_err(db)?;
    let mut report = ExportReport::default();
    let mut next_state: Vec<StateRow> = Vec::new();
    let keep = |next: &mut Vec<StateRow>, rel: &str| next.extend(state.iter().filter(|r| r.rel_path == rel).cloned());

    // Folders renamed only in letter case keep their directory, renamed in place
    for p in planned.iter().filter(|p| p.kind == "folder") {
        if let Some(old) = records.for_id("folder", &p.sync_id) {
            let (old_dir, new_dir) = (old.rel_path.rsplit_once('/').map(|(d, _)| d), p.rel.rsplit_once('/').map(|(d, _)| d));
            if let (Some(a), Some(b)) = (old_dir, new_dir) {
                if a != b && format::fold(a) == format::fold(b) {
                    if let Err(e) = rename_case_only(&root, a, b) {
                        warn!("Vault export: a folder could not be renamed ({:?})", e.kind());
                    }
                }
            }
        }
    }

    // Write what changed: folders, people, then each sidecar before its note
    let mut in_place: HashSet<(String, String)> = HashSet::new();
    let mut stopped = false;
    for (n, p) in planned.iter().enumerate() {
        // Turned off or moved to another folder meanwhile: stop at once, record nothing
        if configured && n % 25 == 0 && !still_on(pool, folder).await {
            stopped = true;
            break;
        }
        match p.kind {
            "note" => report.notes += 1,
            "person" => report.people += 1,
            "folder" => report.folders += 1,
            _ => {}
        }
        // The note's sidecar is its commit point: no note without its sidecar in place
        if p.kind == "note" && planned.iter().any(|s| s.kind == "sidecar" && s.sync_id == p.sync_id) && !in_place.contains(&("sidecar".into(), p.sync_id.clone())) {
            report.failed += 1;
            keep(&mut next_state, &p.rel);
            continue;
        }
        let current = read_owned(&root, &p.rel);
        let ours = |bytes: &[u8]| {
            let sha = format::sha256_hex(bytes);
            let recorded = records.at(&p.rel);
            recorded.iter().any(|r| r.sha256 == sha) || (recorded.is_empty() && self_described(&root, &p.rel, p.kind, &p.sync_id, &device).is_some())
        };
        let outcome: io::Result<bool> = match &current {
            Some(bytes) if bytes == p.bytes.as_bytes() => {
                report.unchanged += 1;
                Ok(true)
            }
            Some(bytes) if ours(bytes) => {
                let mut pending = row(p.kind, &p.sync_id, &p.rel, p.bytes.as_bytes(), &now);
                (pending.content_key, pending.rev, pending.updated) = (p.content_key.clone(), p.rev, p.updated.clone());
                // Recorded first: without the record, a stopped run could not recognise the file
                match store::put_pending(pool, &pending).await {
                    Ok(()) => write_atomic(&root, &p.rel, p.bytes.as_bytes()).map(|_| {
                        report.written += 1;
                        true
                    }),
                    Err(e) => Err(io::Error::new(io::ErrorKind::Other, e.to_string())),
                }
            }
            Some(_) => {
                report.changed_in_vault += 1;
                Ok(false)
            }
            // There but unreadable now (locked, or still downloading from iCloud): try later
            None if exists(&root, &p.rel) => {
                report.failed += 1;
                Ok(false)
            }
            None => {
                let mut pending = row(p.kind, &p.sync_id, &p.rel, p.bytes.as_bytes(), &now);
                (pending.content_key, pending.rev, pending.updated) = (p.content_key.clone(), p.rev, p.updated.clone());
                // Recorded first: without the record, a stopped run could not recognise the file
                match store::put_pending(pool, &pending).await {
                    Ok(()) => write_atomic(&root, &p.rel, p.bytes.as_bytes()).map(|_| {
                        report.written += 1;
                        true
                    }),
                    Err(e) => Err(io::Error::new(io::ErrorKind::Other, e.to_string())),
                }
            }
        };
        match outcome {
            Ok(true) => {
                in_place.insert((p.kind.to_string(), p.sync_id.clone()));
                verified_files.push((p.rel.clone(), p.bytes.clone(), p.kind));
                let sha = format::sha256_hex(p.bytes.as_bytes());
                let since = state.iter().find(|r| r.rel_path == p.rel && r.sha256 == sha).map_or_else(|| now.clone(), |r| r.exported_at.clone());
                next_state.push(StateRow { content_key: p.content_key.clone(), rev: p.rev, updated: p.updated.clone(), exported_at: since, ..row(p.kind, &p.sync_id, &p.rel, p.bytes.as_bytes(), &now) });
            }
            // Left alone: keep the old record, so a later run still knows the file was ours
            Ok(false) => keep(&mut next_state, &p.rel),
            Err(e) => {
                report.failed += 1;
                warn!("Vault export: a {} file could not be written ({:?})", p.kind, e.kind());
                keep(&mut next_state, &p.rel);
            }
        }
    }

    // Turned off or moved meanwhile: no clean-up in that folder, and nothing recorded (review finding)
    if stopped || (configured && !still_on(pool, folder).await) {
        report.failed += 1;
        return Ok(report);
    }

    // What the app wrote before and no longer plans: moves, deletions, old sidecars, trash
    let planned_rels: HashSet<String> = planned.iter().map(|p| format::fold(&p.rel)).collect();
    let planned_ids: HashSet<(&str, &str)> = planned.iter().map(|p| (p.kind, p.sync_id.as_str())).collect();
    let folder_ids: HashMap<String, String> = records
        .rows
        .iter()
        .filter(|r| r.kind == "folder")
        .filter_map(|r| r.rel_path.rsplit_once('/').map(|(d, _)| (d.to_string(), r.sync_id.clone())))
        .collect();
    let mut seen: HashSet<String> = HashSet::new();
    let mut emptied_dirs: Vec<String> = Vec::new();
    for old in &records.rows {
        let folded = format::fold(&old.rel_path);
        if planned_rels.contains(&folded) || !seen.insert(format!("{}|{}", old.kind, folded)) {
            continue;
        }
        let present = exists(&root, &old.rel_path);
        let readable = read_owned(&root, &old.rel_path);
        let ours = readable.as_ref().map_or(false, |b| records.at(&old.rel_path).iter().any(|r| r.sha256 == format::sha256_hex(b)));
        let age_days = store::stored_time(&old.exported_at).map_or(0, |t| (Utc::now() - t).num_days());
        let mut kept = false;
        match old.kind.as_str() {
            "sidecar" => {
                if age_days >= SIDECAR_GRACE_DAYS && ours {
                    let _ = remove_owned(&root, &old.rel_path);
                } else {
                    kept = present;
                }
            }
            "note" | "person" if planned_ids.contains(&(old.kind.as_str(), old.sync_id.as_str())) => {
                // Moved: the old file goes only once the new one is in place
                if ours && in_place.contains(&(old.kind.clone(), old.sync_id.clone())) && remove_owned(&root, &old.rel_path).is_ok() {
                    report.moved += 1;
                } else {
                    kept = present;
                    if readable.is_some() && !ours {
                        report.changed_in_vault += 1;
                    }
                }
            }
            "note" | "person" => {
                let folder = old.rel_path.rsplit_once('/').and_then(|(d, _)| folder_ids.get(d).cloned()).filter(|_| old.kind == "note");
                match tombstone(pool, &root, old, folder, &device, &now).await {
                    Ok(Some(new_rows)) => {
                        report.deleted += 1;
                        next_state.extend(new_rows);
                    }
                    Ok(None) => {
                        kept = present;
                        if readable.is_some() {
                            report.changed_in_vault += 1;
                        }
                    }
                    Err(e) => {
                        report.failed += 1;
                        warn!("Vault export: a deleted {} could not be moved to _trash ({:?})", old.kind, e.kind());
                        kept = present;
                    }
                }
            }
            "folder" => {
                if ours && remove_owned(&root, &old.rel_path).is_ok() {
                    if let Some((dir, _)) = old.rel_path.rsplit_once('/') {
                        emptied_dirs.push(dir.to_string());
                    }
                } else {
                    kept = present;
                }
            }
            // A deleted note's content goes after 30 days, its tombstone after a year (section 12)
            "trash" | "tombstone" => {
                let limit = if old.kind == "trash" { 30 } else { 365 };
                if age_days >= limit && ours {
                    let _ = remove_owned(&root, &old.rel_path);
                    if let Some((dir, _)) = old.rel_path.rsplit_once('/') {
                        emptied_dirs.push(dir.to_string());
                    }
                } else {
                    kept = present;
                }
            }
            _ => {}
        }
        if kept {
            next_state.push(old.clone());
        }
    }
    // A directory goes only when nothing is left in it (section 9)
    for dir in emptied_dirs {
        remove_empty_dir(&root, &dir);
    }
    let _ = fs::remove_dir(root.join("_tmp"));

    report.verified = report.failed == 0 && verify(&root, &verified_files).map_err(|e| warn!("Vault export: verification failed: {}", e)).is_ok();
    if report.verified {
        next_state.sort_by(|a, b| (&a.kind, &a.rel_path).cmp(&(&b.kind, &b.rel_path)));
        next_state.dedup_by(|a, b| a.kind == b.kind && a.rel_path == b.rel_path);
        store::replace_state(pool, &next_state).await.map_err(db)?;
    }
    store::record_run(pool, &serde_json::to_string(&report).unwrap_or_default()).await.map_err(db)?;
    Ok(report)
}
