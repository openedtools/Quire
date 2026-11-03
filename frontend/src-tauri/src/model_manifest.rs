//! The model manifest (`models.json`): every model file Quire downloads, pinned to a source
//! revision and by SHA-256 (spec F-MOD-8, plan PC4). A downloaded file that does not match is
//! deleted and never loaded; files downloaded before the manifest are checked once.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use log::{info, warn};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ManifestFile {
    /// `parakeet`, `whisper`, `summary` or `speaker`
    pub kind: String,
    pub model: String,
    /// The file name on disk
    pub file: String,
    pub url: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParakeetLanguages {
    pub model: String,
    pub codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub parakeet_languages: ParakeetLanguages,
    pub files: Vec<ManifestFile>,
}

pub static MANIFEST: Lazy<Manifest> =
    Lazy::new(|| serde_json::from_str(include_str!("../models.json")).expect("models.json is valid"));

/// The pinned file of a model, by kind, model name and file name.
pub fn pinned(kind: &str, model: &str, file: &str) -> Option<&'static ManifestFile> {
    MANIFEST.files.iter().find(|f| f.kind == kind && f.model == model && f.file == file)
}

/// The files of one model.
pub fn files_of(kind: &str, model: &str) -> Vec<&'static ManifestFile> {
    MANIFEST.files.iter().filter(|f| f.kind == kind && f.model == model).collect()
}

/// SHA-256 of a file as lowercase hex, on a blocking thread.
pub async fn sha256_of(path: &Path) -> Result<String> {
    let file = path.to_path_buf();
    tokio::task::spawn_blocking(move || crate::diarization::model::sha256_file(&file))
        .await
        .map_err(|e| anyhow!("Could not check the file: {}", e))?
}

/// Checks a finished download against its pin; a wrong size or hash deletes the file and is an error.
pub async fn verify_or_delete(path: &Path, label: &str, pin: &ManifestFile) -> Result<()> {
    let bytes = tokio::fs::metadata(path).await.map(|m| m.len()).map_err(|e| anyhow!("Could not read {}: {}", label, e))?;
    let problem = match classify(bytes, pin) {
        Existing::Partial => Some(format!("Downloaded model file is too small: {} bytes (expected {} bytes)", bytes, pin.bytes)),
        Existing::Wrong => Some(format!("Downloaded model file is too large: {} bytes (expected {} bytes)", bytes, pin.bytes)),
        Existing::Hash if sha256_of(path).await? != pin.sha256 => {
            Some(format!("{} is not the expected file (SHA-256 mismatch)", label))
        }
        Existing::Hash => None,
    };
    if let Some(problem) = problem {
        let _ = tokio::fs::remove_file(path).await;
        return Err(anyhow!("{}; it was deleted and will not be used", problem));
    }
    Ok(())
}

/// Checks a finished download of a catalogue model: `Ok` when it matches its pin, an error (and the
/// file deleted) when it does not or when the model has no pin.
pub async fn verify_download(kind: &str, model: &str, path: &Path) -> Result<()> {
    let file = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    let Some(pin) = pinned(kind, model, file) else {
        let _ = tokio::fs::remove_file(path).await;
        return Err(anyhow!("{} has no pinned SHA-256 in models.json; it was deleted and will not be used", model));
    };
    verify_or_delete(path, model, pin).await
}

/// Where a pinned file lives under the app's `models` folder.
pub fn installed_path(models_root: &Path, pin: &ManifestFile) -> Option<PathBuf> {
    match pin.kind.as_str() {
        "whisper" => Some(models_root.join(&pin.file)),
        "summary" => Some(models_root.join("summary").join(&pin.file)),
        "parakeet" => Some(models_root.join("parakeet").join(&pin.model).join(&pin.file)),
        // The speaker model is checked before every load already
        _ => None,
    }
}

/// What the one-time check does with a file found on disk.
#[derive(Debug, PartialEq)]
pub enum Existing {
    /// Smaller than the pin: a download that can still resume
    Partial,
    /// Exactly the pinned size: hash it
    Hash,
    /// Larger than the pin: not the expected file
    Wrong,
}

pub fn classify(bytes: u64, pin: &ManifestFile) -> Existing {
    if bytes < pin.bytes {
        Existing::Partial
    } else if bytes == pin.bytes {
        Existing::Hash
    } else {
        Existing::Wrong
    }
}

const CHECKED_FILE: &str = ".checked-models.json";

/// The name a file that does not match its pin is set aside under (plan PC4).
pub fn set_aside_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".unverified");
    path.with_file_name(name)
}

/// Hashes every installed model file once (remembered by size and modification time). One that
/// does not match its pin is set aside (renamed with `.unverified`), so it shows as needing a
/// download and is never loaded; the window is told. A set-aside copy is removed once the model
/// has been downloaded again and matches. Returns the models whose files were set aside.
pub async fn check_installed(models_root: &Path) -> Vec<String> {
    let record_path = models_root.join(CHECKED_FILE);
    let mut checked: HashMap<String, String> = tokio::fs::read_to_string(&record_path)
        .await
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let mut removed = Vec::new();
    for pin in &MANIFEST.files {
        let Some(path) = installed_path(models_root, pin) else { continue };
        let Ok(meta) = tokio::fs::metadata(&path).await else { continue };
        let aside = set_aside_path(&path);
        let stamp = format!(
            "{}:{}",
            meta.len(),
            meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs())
        );
        let key = format!("{}/{}/{}", pin.kind, pin.model, pin.file);
        if checked.get(&key) == Some(&stamp) {
            if aside.exists() {
                let _ = tokio::fs::remove_file(&aside).await;
            }
            continue;
        }
        let good = match classify(meta.len(), pin) {
            Existing::Partial => continue,
            Existing::Wrong => false,
            Existing::Hash => match sha256_of(&path).await {
                Ok(sha) => sha == pin.sha256,
                Err(e) => {
                    warn!("Could not check model {} {}: {}", pin.model, pin.file, e);
                    continue;
                }
            },
        };
        if good {
            checked.insert(key, stamp);
            if aside.exists() {
                let _ = tokio::fs::remove_file(&aside).await;
            }
        } else {
            warn!("Model {} {} does not match its pinned SHA-256; setting it aside", pin.model, pin.file);
            let _ = tokio::fs::remove_file(&aside).await;
            match tokio::fs::rename(&path, &aside).await {
                Ok(()) => removed.push(pin.model.clone()),
                Err(e) => warn!("Could not set aside model {} {}: {}", pin.model, pin.file, e),
            }
            checked.remove(&key);
        }
    }
    if let Ok(text) = serde_json::to_string_pretty(&checked) {
        let _ = tokio::fs::write(&record_path, text).await;
    }
    removed.dedup();
    if !removed.is_empty() {
        info!("Set aside {} model(s) that did not match their pins", removed.len());
    }
    removed
}

/// The manifest for the window (Models screen).
#[tauri::command]
pub fn api_model_manifest() -> Manifest {
    MANIFEST.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin_for(data: &[u8]) -> ManifestFile {
        use sha2::{Digest, Sha256};
        ManifestFile {
            kind: "whisper".into(),
            model: "test".into(),
            file: "ggml-test.bin".into(),
            url: "https://huggingface.co/x/resolve/abc/ggml-test.bin".into(),
            bytes: data.len() as u64,
            sha256: Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect(),
        }
    }

    #[test]
    fn the_manifest_pins_every_file_to_a_revision_and_a_hash() {
        assert!(!MANIFEST.files.is_empty());
        for f in &MANIFEST.files {
            assert_eq!(f.sha256.len(), 64, "{} {}", f.model, f.file);
            assert!(f.sha256.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
            assert!(f.bytes > 0);
            assert!(f.url.starts_with("https://"), "{}", f.url);
            assert!(!f.url.contains("/resolve/main/"), "{} is not pinned to a revision", f.url);
        }
        assert_eq!(MANIFEST.parakeet_languages.codes.len(), 25);
    }

    #[test]
    fn every_whisper_model_in_the_catalogue_is_pinned() {
        for (name, file, ..) in crate::config::WHISPER_MODEL_CATALOG {
            assert!(pinned("whisper", name, file).is_some(), "{} has no pin", name);
        }
    }

    #[test]
    fn every_built_in_summary_model_is_pinned_to_a_revision_of_its_source() {
        for model in crate::summary::summary_engine::models::get_available_models() {
            let pin = pinned("summary", &model.name, &model.gguf_file).unwrap_or_else(|| panic!("{} has no pin", model.name));
            // The catalogue names the source; the download uses the same file at a fixed revision
            let (repo, file) = model.download_url.split_once("/resolve/main/").expect("catalogue address");
            assert!(pin.url.starts_with(&format!("{}/resolve/", repo)), "{}", model.name);
            assert!(pin.url.ends_with(&format!("/{}", file)), "{}", model.name);
        }
    }

    #[test]
    fn the_speaker_model_pin_matches_its_check() {
        let pin = pinned("speaker", "wespeaker-resnet34", crate::diarization::model::MODEL_FILE).expect("pinned");
        assert_eq!(pin.sha256, crate::diarization::model::MODEL_SHA256);
        assert_eq!(pin.bytes, crate::diarization::model::MODEL_SIZE_BYTES);
        assert_eq!(pin.url, crate::diarization::model::MODEL_URL);
    }

    #[tokio::test]
    async fn a_corrupted_download_is_deleted_and_refused() {
        let dir = tempfile::tempdir().unwrap();
        let pin = pin_for(b"the real model");
        let good = dir.path().join("good.bin");
        std::fs::write(&good, b"the real model").unwrap();
        assert!(verify_or_delete(&good, "good", &pin).await.is_ok());
        assert!(good.exists());

        let bad = dir.path().join("bad.bin");
        std::fs::write(&bad, b"the real modeX").unwrap();
        assert!(verify_or_delete(&bad, "bad", &pin).await.is_err());
        assert!(!bad.exists(), "same size, wrong hash: deleted");

        let short = dir.path().join("short.bin");
        std::fs::write(&short, b"the real").unwrap();
        assert!(verify_or_delete(&short, "short", &pin).await.is_err());
        assert!(!short.exists());
    }

    #[tokio::test]
    async fn a_download_of_a_model_without_a_pin_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("ggml-unknown.bin");
        std::fs::write(&file, b"x").unwrap();
        assert!(verify_download("whisper", "unknown", &file).await.is_err());
        assert!(!file.exists());
    }

    #[test]
    fn existing_files_are_classified_by_size_first() {
        let pin = pin_for(b"12345");
        assert_eq!(classify(3, &pin), Existing::Partial);
        assert_eq!(classify(5, &pin), Existing::Hash);
        assert_eq!(classify(9, &pin), Existing::Wrong);
    }

    #[tokio::test]
    async fn models_already_on_disk_are_checked_once_and_a_wrong_one_is_deleted() {
        let root = tempfile::tempdir().unwrap();
        let pin = pinned("whisper", "tiny-q5_1", "ggml-tiny-q5_1.bin").unwrap();
        let sized = |name: &str, len: u64| {
            let path = root.path().join(name);
            std::fs::File::create(&path).unwrap().set_len(len).unwrap();
            path
        };
        // Right size, wrong content: hashed and deleted
        let wrong = sized("ggml-tiny-q5_1.bin", pin.bytes);
        // A partial download of another model is left to resume
        let other = pinned("whisper", "base-q5_1", "ggml-base-q5_1.bin").unwrap();
        let partial = sized("ggml-base-q5_1.bin", other.bytes / 2);
        let removed = check_installed(root.path()).await;
        assert_eq!(removed, vec!["tiny-q5_1".to_string()]);
        assert!(!wrong.exists(), "never loaded");
        assert!(set_aside_path(&wrong).exists(), "set aside, not deleted");
        assert!(partial.exists());

        // A larger file is not the expected one either
        let big = sized("ggml-tiny-q5_1.bin", pin.bytes + 1);
        assert_eq!(check_installed(root.path()).await, vec!["tiny-q5_1".to_string()]);
        assert!(!big.exists());
        assert_eq!(std::fs::metadata(set_aside_path(&big)).unwrap().len(), pin.bytes + 1, "the newest copy is the one set aside");
    }

    #[test]
    fn installed_paths_follow_each_engine_s_folder() {
        let root = Path::new("/m");
        let whisper = pinned("whisper", "tiny", "ggml-tiny.bin").unwrap();
        assert_eq!(installed_path(root, whisper), Some(root.join("ggml-tiny.bin")));
        let parakeet = pinned("parakeet", "parakeet-tdt-0.6b-v3-int8", "vocab.txt").unwrap();
        assert_eq!(installed_path(root, parakeet), Some(root.join("parakeet/parakeet-tdt-0.6b-v3-int8/vocab.txt")));
        let summary = pinned("summary", "qwen3.5:4b", "Qwen3.5-4B-Q4_K_M.gguf").unwrap();
        assert_eq!(installed_path(root, summary), Some(root.join("summary/Qwen3.5-4B-Q4_K_M.gguf")));
    }
}
