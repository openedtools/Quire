//! Deleting a meeting's audio, and only its audio (spec F-RET-5, F-RET-6, F-RET-7).
//!
//! Unlike `api::recording_files::delete_recording_files`, which removes a whole recording when a
//! meeting is deleted, this keeps `transcripts.json` and `metadata.json`, never stops at the first
//! error, and reports a result for every file so a locked file can be retried later.

use std::io;
use std::path::{Path, PathBuf};

use crate::audio::constants::AUDIO_EXTENSIONS;

/// At least one of these must be present for a folder to count as one of ours.
pub const MARKER_FILES: &[&str] = &["metadata.json", "transcripts.json"];
/// Working files the recorder leaves while recording.
pub const CHECKPOINT_DIR: &str = ".checkpoints";
/// Prefixes of temporary decode files.
pub const TEMP_PREFIXES: [&str; 1] = [crate::brand_generated::TEMP_PREFIX];

/// Why a folder was not touched at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The folder does not exist or is not a folder.
    Missing,
    /// The folder cannot be reached right now (a drive that is not attached, an offline share, a
    /// permission error). Nothing is decided until it can be.
    Unreachable,
    /// The folder, or something in its place, is a link or a Windows junction.
    Link,
    /// The folder has none of the app's marker files.
    NotOurs,
    /// The folder is not directly inside a known recordings folder.
    OutsideRoots,
    /// Another meeting points at the same folder.
    Shared,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileOutcome {
    /// Removed, and checked to be gone.
    Deleted,
    /// Could not be removed (a lock, permissions); kept for a later retry.
    Failed(io::ErrorKind),
}

/// One file's result. `name` is the file name only, never a full path (paths carry titles).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileResult {
    pub name: String,
    pub outcome: FileOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AudioDeletion {
    pub refused: Option<Refusal>,
    pub results: Vec<FileResult>,
}

impl AudioDeletion {
    /// True when no audio is left: nothing was refused and every file was removed.
    pub fn complete(&self) -> bool {
        self.refused.is_none() && self.results.iter().all(|r| r.outcome == FileOutcome::Deleted)
    }

    pub fn failed(&self) -> usize {
        self.results.iter().filter(|r| r.outcome != FileOutcome::Deleted).count()
    }
}

/// A link, or on Windows any reparse point (junctions included), except cloud placeholders.
pub fn is_link(path: &Path) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
        let attributes = meta.file_attributes();
        return attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 && attributes & FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS == 0;
    }
    #[allow(unreachable_code)]
    false
}

fn is_audio_name(name: &str) -> bool {
    Path::new(name)
        .extension()
        .map(|e| AUDIO_EXTENSIONS.contains(&e.to_string_lossy().to_lowercase().as_str()))
        .unwrap_or(false)
}

fn is_temp_name(name: &str) -> bool {
    TEMP_PREFIXES.iter().any(|p| name.starts_with(p))
}

/// Checks a meeting folder before anything is deleted from it. `roots` empty means the caller
/// already decided where the folder may be (a meeting the person is deleting themselves).
/// Whether a folder is there, gone (its parent is there but it is not), or cannot be reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    Present,
    Gone,
    Unreachable,
}

pub fn reach(folder: &Path) -> Reach {
    match std::fs::symlink_metadata(folder) {
        Ok(_) => Reach::Present,
        Err(e) if e.kind() == io::ErrorKind::NotFound && folder.parent().map_or(false, |p| p.is_dir()) => Reach::Gone,
        Err(_) => Reach::Unreachable,
    }
}

pub fn check_folder(folder: &Path, roots: &[PathBuf], shared: bool) -> Result<(), Refusal> {
    let meta = match reach(folder) {
        Reach::Present => std::fs::symlink_metadata(folder).map_err(|_| Refusal::Unreachable)?,
        Reach::Gone => return Err(Refusal::Missing),
        Reach::Unreachable => return Err(Refusal::Unreachable),
    };
    if is_link(folder) {
        return Err(Refusal::Link);
    }
    if !meta.is_dir() {
        return Err(Refusal::Missing);
    }
    let ours = MARKER_FILES
        .iter()
        .any(|m| std::fs::symlink_metadata(folder.join(m)).map(|x| x.is_file()).unwrap_or(false));
    if !ours {
        return Err(Refusal::NotOurs);
    }
    if !roots.is_empty() {
        let parent = folder.parent().and_then(|p| std::fs::canonicalize(p).ok());
        let inside = parent.map_or(false, |p| roots.iter().filter_map(|r| std::fs::canonicalize(r).ok()).any(|r| r == p));
        if !inside {
            return Err(Refusal::OutsideRoots);
        }
    }
    if shared {
        return Err(Refusal::Shared);
    }
    Ok(())
}

fn remove_checked(path: &Path, name: String, results: &mut Vec<FileResult>) {
    let outcome = match std::fs::remove_file(path) {
        Ok(()) if std::fs::symlink_metadata(path).is_err() => FileOutcome::Deleted,
        Ok(()) => FileOutcome::Failed(io::ErrorKind::Other),
        Err(e) if e.kind() == io::ErrorKind::NotFound => FileOutcome::Deleted,
        Err(e) => FileOutcome::Failed(e.kind()),
    };
    results.push(FileResult { name, outcome });
}

/// The audio artefacts in a folder, as (path, name) pairs: top-level audio files in any case,
/// temporary decode files, and the files in `.checkpoints`.
pub fn audio_artifacts(folder: &Path) -> Vec<(PathBuf, String)> {
    let mut found = Vec::new();
    if let Ok(entries) = std::fs::read_dir(folder) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();
            let plain_file = std::fs::symlink_metadata(&path).map(|m| m.is_file() || m.file_type().is_symlink()).unwrap_or(false);
            if plain_file && (is_audio_name(&name) || is_temp_name(&name)) {
                found.push((path, name));
            }
        }
    }
    let checkpoints = folder.join(CHECKPOINT_DIR);
    if std::fs::symlink_metadata(&checkpoints).map(|m| m.is_dir()).unwrap_or(false) && !is_link(&checkpoints) {
        if let Ok(entries) = std::fs::read_dir(&checkpoints) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let path = entry.path();
                if std::fs::symlink_metadata(&path).map(|m| m.is_file() || m.file_type().is_symlink()).unwrap_or(false) {
                    found.push((path, format!("{CHECKPOINT_DIR}/{name}")));
                }
            }
        }
    }
    found.sort_by(|a, b| a.1.cmp(&b.1));
    found
}

/// Deletes the audio in a meeting folder, file by file. Transcript and metadata files stay.
/// A link named like an audio file is removed itself; its target is never touched.
pub fn delete_audio(folder: &Path, roots: &[PathBuf], shared: bool) -> AudioDeletion {
    if let Err(refusal) = check_folder(folder, roots, shared) {
        return AudioDeletion { refused: Some(refusal), results: Vec::new() };
    }
    let mut results = Vec::new();
    for (path, name) in audio_artifacts(folder) {
        remove_checked(&path, name, &mut results);
    }
    let checkpoints = folder.join(CHECKPOINT_DIR);
    if !is_link(&checkpoints) {
        let _ = std::fs::remove_dir(&checkpoints);
    }
    AudioDeletion { refused: None, results }
}

/// The mode a recording was made in, from its `metadata.json` (`audio` or `transcript_only`).
pub fn recorded_mode(folder: &Path) -> Option<String> {
    let text = std::fs::read_to_string(folder.join("metadata.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    json.get("mode").and_then(|m| m.as_str()).map(str::to_string)
}

/// Whether a folder still holds any audio.
pub fn has_audio(folder: &Path) -> bool {
    !audio_artifacts(folder).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(dir: &Path, name: &str) {
        std::fs::write(dir.join(name), b"x").unwrap();
    }

    fn meeting(root: &Path, name: &str) -> PathBuf {
        let folder = root.join(name);
        std::fs::create_dir_all(folder.join(CHECKPOINT_DIR)).unwrap();
        touch(&folder, "metadata.json");
        touch(&folder, "transcripts.json");
        folder
    }

    #[test]
    fn deletes_every_audio_artefact_and_keeps_the_transcript() {
        let root = tempfile::tempdir().unwrap();
        let folder = meeting(root.path(), "Weekly_2026-11-02_14-05-07_ab12");
        let temp = format!("{}x.wav", crate::brand_generated::TEMP_PREFIX);
        for name in ["audio.mp4", "audio.MP3", "imported.aac", temp.as_str(), "notes.txt"] {
            touch(&folder, name);
        }
        touch(&folder.join(CHECKPOINT_DIR), "audio_chunk_000.mp4");

        let deletion = delete_audio(&folder, &[root.path().to_path_buf()], false);
        assert!(deletion.complete(), "{deletion:?}");
        assert_eq!(deletion.results.len(), 5);
        assert!(deletion.results.iter().all(|r| !r.name.contains('/') || r.name.starts_with(".checkpoints/")));
        for gone in ["audio.mp4", "audio.MP3", "imported.aac", temp.as_str()] {
            assert!(!folder.join(gone).exists(), "{gone}");
        }
        assert!(!folder.join(CHECKPOINT_DIR).exists());
        for kept in ["metadata.json", "transcripts.json", "notes.txt"] {
            assert!(folder.join(kept).exists(), "{kept}");
        }
        assert!(!has_audio(&folder));
    }

    #[test]
    fn folders_that_are_not_safe_are_refused_whole() {
        let root = tempfile::tempdir().unwrap();
        let roots = [root.path().to_path_buf()];

        let foreign = root.path().join("Music");
        std::fs::create_dir_all(&foreign).unwrap();
        touch(&foreign, "song.mp3");
        assert_eq!(delete_audio(&foreign, &roots, false).refused, Some(Refusal::NotOurs));
        assert!(foreign.join("song.mp3").exists());

        let elsewhere = tempfile::tempdir().unwrap();
        let outside = meeting(elsewhere.path(), "M");
        touch(&outside, "audio.mp4");
        assert_eq!(delete_audio(&outside, &roots, false).refused, Some(Refusal::OutsideRoots));
        assert!(outside.join("audio.mp4").exists());

        let nested = meeting(&root.path().join("deeper"), "M");
        touch(&nested, "audio.mp4");
        assert_eq!(delete_audio(&nested, &roots, false).refused, Some(Refusal::OutsideRoots), "only direct children of a root");

        let shared = meeting(root.path(), "Shared");
        touch(&shared, "audio.mp4");
        assert_eq!(delete_audio(&shared, &roots, true).refused, Some(Refusal::Shared));
        assert!(shared.join("audio.mp4").exists());

        assert_eq!(delete_audio(&root.path().join("gone"), &roots, false).refused, Some(Refusal::Missing));

        // No roots given: the caller (deleting a meeting) already decided the place
        assert!(delete_audio(&outside, &[], false).complete());
        assert!(!outside.join("audio.mp4").exists());
    }

    #[cfg(unix)]
    #[test]
    fn links_are_refused_or_removed_without_following() {
        let root = tempfile::tempdir().unwrap();
        let roots = [root.path().to_path_buf()];
        let precious = meeting(root.path(), "Precious");
        touch(&precious, "audio.mp4");
        let link = root.path().join("Link");
        std::os::unix::fs::symlink(&precious, &link).unwrap();
        assert_eq!(delete_audio(&link, &roots, false).refused, Some(Refusal::Link));
        assert!(precious.join("audio.mp4").exists());

        let folder = meeting(root.path(), "M");
        let target = root.path().join("outside.wav");
        std::fs::write(&target, b"keep").unwrap();
        std::os::unix::fs::symlink(&target, folder.join("audio.wav")).unwrap();
        assert!(delete_audio(&folder, &roots, false).complete());
        assert!(target.exists(), "the link goes, its target stays");

        // A linked .checkpoints folder is never entered
        let other = tempfile::tempdir().unwrap();
        touch(other.path(), "chunk.mp4");
        let with_link = root.path().join("W");
        std::fs::create_dir_all(&with_link).unwrap();
        touch(&with_link, "metadata.json");
        std::os::unix::fs::symlink(other.path(), with_link.join(CHECKPOINT_DIR)).unwrap();
        delete_audio(&with_link, &roots, false);
        assert!(other.path().join("chunk.mp4").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_file_that_cannot_be_removed_is_reported_and_the_rest_still_go() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let folder = meeting(root.path(), "M");
        touch(&folder, "audio.mp4");
        touch(&folder.join(CHECKPOINT_DIR), "audio_chunk_000.mp4");
        // A read-only checkpoints folder: its file cannot be unlinked
        std::fs::set_permissions(folder.join(CHECKPOINT_DIR), std::fs::Permissions::from_mode(0o500)).unwrap();
        let deletion = delete_audio(&folder, &[root.path().to_path_buf()], false);
        std::fs::set_permissions(folder.join(CHECKPOINT_DIR), std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(!deletion.complete());
        assert_eq!(deletion.failed(), 1);
        assert!(!folder.join("audio.mp4").exists(), "one failure does not stop the others");
        assert!(has_audio(&folder));
        // A later retry succeeds once the cause is gone
        assert!(delete_audio(&folder, &[root.path().to_path_buf()], false).complete());
    }
}
