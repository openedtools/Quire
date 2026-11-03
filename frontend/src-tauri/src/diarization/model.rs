//! The speaker-embedding model file: location, download and integrity check.

use anyhow::{anyhow, bail, Context, Result};
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

pub const MODEL_FILE: &str = "wespeaker_en_voxceleb_resnet34.onnx";
/// WeSpeaker ResNet34 trained on VoxCeleb (Apache-2.0), exported to ONNX by the sherpa-onnx project.
pub const MODEL_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/wespeaker_en_voxceleb_resnet34.onnx";
/// Checked after download so a truncated, corrupted or swapped file is never used.
pub const MODEL_SHA256: &str = "5ef208a9da1453335308a6b6f4e6dfbd7e183a38b604de0a57664f45d257fe94";
pub const MODEL_SIZE_BYTES: u64 = 26_534_365;

pub fn model_path(models_dir: &Path) -> PathBuf {
    models_dir.join(MODEL_FILE)
}

/// Cheap presence check: right name and right size. Not proof the file is the right one; use
/// [`is_verified`] before loading it.
pub fn is_downloaded(models_dir: &Path) -> bool {
    std::fs::metadata(model_path(models_dir)).map_or(false, |m| m.len() == MODEL_SIZE_BYTES)
}

/// SHA-256 of a file as lowercase hex, read in chunks.
pub fn sha256_file(path: &Path) -> Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).with_context(|| format!("Could not read {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// True only when the model file is present and is exactly the expected file (size and SHA-256).
///
/// Checked every time before the model is loaded, not just after download: a model file is parsed by
/// a native library, so a swapped, corrupted or hand-copied wrong file must never reach it. Takes a
/// fraction of a second for ~26 MB.
pub fn is_verified(models_dir: &Path) -> bool {
    is_downloaded(models_dir) && sha256_file(&model_path(models_dir)).map_or(false, |hash| hash == MODEL_SHA256)
}

/// Download the model into `models_dir`, reporting `(bytes_so_far, total_bytes)`.
/// The file only appears under its final name once the checksum matches.
pub async fn download(models_dir: &Path, mut on_progress: impl FnMut(u64, u64)) -> Result<PathBuf> {
    tokio::fs::create_dir_all(models_dir).await.context("Could not create the models folder")?;
    let final_path = model_path(models_dir);
    let part_path = models_dir.join(format!("{MODEL_FILE}.part"));

    let result = download_to(&part_path, &mut on_progress).await;
    match result {
        Ok(()) => {
            tokio::fs::rename(&part_path, &final_path).await.context("Could not save the model")?;
            Ok(final_path)
        }
        Err(e) => {
            let _ = tokio::fs::remove_file(&part_path).await;
            Err(e)
        }
    }
}

async fn download_to(part_path: &Path, on_progress: &mut impl FnMut(u64, u64)) -> Result<()> {
    // net: model download the person starts (speaker model, pinned)
    let response = reqwest::Client::new()
        .get(MODEL_URL)
        .send()
        .await
        .context("Could not reach the model download server")?;
    if !response.status().is_success() {
        bail!("Model download failed: HTTP {}", response.status());
    }
    let total = response.content_length().unwrap_or(MODEL_SIZE_BYTES);

    let mut file = tokio::fs::File::create(part_path).await.context("Could not write the model file")?;
    let mut hasher = Sha256::new();
    let mut downloaded = 0u64;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("Model download was interrupted")?;
        // Never write more than the real file's size, whatever the server keeps sending
        if downloaded + chunk.len() as u64 > MODEL_SIZE_BYTES {
            return Err(anyhow!("The download is larger than expected, so it was stopped."));
        }
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        on_progress(downloaded, total);
    }
    file.flush().await?;
    drop(file);

    let digest = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect::<String>();
    if digest != MODEL_SHA256 {
        return Err(anyhow!("Downloaded model failed its integrity check, so it was discarded. Please try again."));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presence_check_requires_the_exact_size() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_downloaded(dir.path()));
        std::fs::write(model_path(dir.path()), b"truncated").unwrap();
        assert!(!is_downloaded(dir.path()), "a partial file does not count");
    }

    #[test]
    fn sha256_matches_the_standard_test_vector() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("abc.bin");
        std::fs::write(&file, b"abc").unwrap();
        assert_eq!(sha256_file(&file).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert!(sha256_file(&dir.path().join("missing")).is_err());
    }

    #[test]
    fn a_wrong_file_of_the_right_size_is_not_accepted() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(model_path(dir.path()), vec![0u8; MODEL_SIZE_BYTES as usize]).unwrap();
        assert!(is_downloaded(dir.path()), "size alone passes the cheap check");
        assert!(!is_verified(dir.path()), "but the checksum catches it");
    }

    #[test]
    fn checksum_constant_is_a_sha256_hex_digest() {
        assert_eq!(MODEL_SHA256.len(), 64);
        assert!(MODEL_SHA256.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
