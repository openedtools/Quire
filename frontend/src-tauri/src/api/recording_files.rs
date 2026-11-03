//! Removing a meeting's recording from disk when the meeting is deleted.
//!
//! Deleting a meeting used to remove only its database rows, leaving the audio and the full
//! transcript behind in the recordings folder. This removes them too, but deliberately narrowly:
//! only files the app itself creates, and only from a folder that carries the app's own marker
//! files, so a wrong or hostile folder path can never delete anything else. Deleting only the audio
//! (retention) is `crate::retention::files::delete_audio`.

use std::path::Path;

use crate::retention::files::{audio_artifacts, check_folder, is_link, CHECKPOINT_DIR, MARKER_FILES};

/// Counts from removing a recording.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RemovalReport {
    /// Files and folders removed.
    pub removed: usize,
    /// Files that could not be removed (a lock, permissions); the folder is then kept.
    pub failed: usize,
}

/// Remove the app's files in a meeting's recording folder (audio in any case and extension,
/// checkpoints, temp files, the transcript and metadata), then the folder itself if that leaves it
/// empty. Every file is attempted even when one fails. Nothing happens unless the folder is a real
/// folder (not a link or junction) carrying the app's marker files.
pub(crate) fn remove_recording(folder: &Path) -> RemovalReport {
    let mut report = RemovalReport::default();
    if check_folder(folder, &[], false).is_err() {
        return report;
    }
    let mut targets: Vec<std::path::PathBuf> = audio_artifacts(folder).into_iter().map(|(path, _)| path).collect();
    targets.extend(MARKER_FILES.iter().map(|name| folder.join(name)));
    for path in targets {
        // symlink_metadata: a link named like ours is removed itself, never followed
        let present = std::fs::symlink_metadata(&path).map_or(false, |m| m.is_file() || m.file_type().is_symlink());
        if !present {
            continue;
        }
        match std::fs::remove_file(&path) {
            Ok(()) => report.removed += 1,
            Err(_) => report.failed += 1,
        }
    }
    let checkpoints = folder.join(CHECKPOINT_DIR);
    if !is_link(&checkpoints) {
        let _ = std::fs::remove_dir(&checkpoints);
    }
    // Remove the folder only if nothing else is in it (never recursive)
    if report.failed == 0 && std::fs::remove_dir(folder).is_ok() {
        report.removed += 1;
    }
    report
}

/// Delete the app's recording files in `folder`, then the folder itself if that leaves it empty.
/// Returns how many files and folders were removed; `Ok(0)` if there was nothing to remove or the
/// folder is not recognisably one of the app's. An error means some files are still there.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn delete_recording_files(folder: &Path) -> std::io::Result<usize> {
    let report = remove_recording(folder);
    if report.failed > 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("{} recording file(s) could not be removed", report.failed),
        ));
    }
    Ok(report.removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(dir: &Path, name: &str) {
        std::fs::write(dir.join(name), b"x").unwrap();
    }

    #[test]
    fn removes_the_recording_and_then_the_empty_folder() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("Meeting_2026");
        std::fs::create_dir_all(folder.join(".checkpoints")).unwrap();
        touch(&folder.join(".checkpoints"), "chunk_0.mp4");
        let temp_name = format!("{}abc.wav", crate::brand_generated::TEMP_PREFIX);
        for name in ["audio.mp4", "transcripts.json", "metadata.json", temp_name.as_str()] {
            touch(&folder, name);
        }
        let removed = delete_recording_files(&folder).unwrap();
        assert_eq!(removed, 6, "3 files + checkpoints dir + temp file + the folder");
        assert!(!folder.exists());
    }

    #[test]
    fn leaves_anything_the_app_did_not_create_and_keeps_the_folder_then() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("Meeting");
        std::fs::create_dir_all(&folder).unwrap();
        for name in ["audio.mp4", "metadata.json", "my-own-notes.txt", "photo.jpg"] {
            touch(&folder, name);
        }
        delete_recording_files(&folder).unwrap();
        assert!(!folder.join("audio.mp4").exists() && !folder.join("metadata.json").exists());
        assert!(folder.join("my-own-notes.txt").exists() && folder.join("photo.jpg").exists(), "user files survive");
        assert!(folder.exists(), "a folder that is not empty is not removed");
    }

    #[test]
    fn a_folder_without_the_apps_marker_files_is_left_completely_alone() {
        // e.g. a database row pointing at the user's own Music folder
        let root = tempfile::tempdir().unwrap();
        let music = root.path().join("Music");
        std::fs::create_dir_all(&music).unwrap();
        touch(&music, "audio.mp3");
        touch(&music, "recording.mp4");
        assert_eq!(delete_recording_files(&music).unwrap(), 0);
        assert!(music.join("audio.mp3").exists() && music.join("recording.mp4").exists());
    }

    #[test]
    fn missing_folders_files_and_odd_paths_are_not_errors() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(delete_recording_files(&root.path().join("nope")).unwrap(), 0);
        let file = root.path().join("a-file");
        touch(root.path(), "a-file");
        assert_eq!(delete_recording_files(&file).unwrap(), 0, "a file is not a recording folder");
        assert!(file.exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_never_followed() {
        let root = tempfile::tempdir().unwrap();
        let precious = root.path().join("precious");
        std::fs::create_dir_all(&precious).unwrap();
        touch(&precious, "metadata.json");
        touch(&precious, "audio.mp4");
        // a "meeting folder" that is really a link to somewhere else
        let link = root.path().join("link");
        std::os::unix::fs::symlink(&precious, &link).unwrap();
        assert_eq!(delete_recording_files(&link).unwrap(), 0);
        assert!(precious.join("audio.mp4").exists());

        // a real folder whose audio.mp4 is a link: the link goes, the target stays
        let folder = root.path().join("Meeting");
        std::fs::create_dir_all(&folder).unwrap();
        touch(&folder, "metadata.json");
        let target = root.path().join("outside.dat");
        std::fs::write(&target, b"keep").unwrap();
        std::os::unix::fs::symlink(&target, folder.join("audio.mp4")).unwrap();
        delete_recording_files(&folder).unwrap();
        assert!(target.exists(), "the file the link pointed to is untouched");
    }
}
