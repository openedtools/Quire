use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_store::StoreExt;

use anyhow::Result;
#[cfg(target_os = "macos")]
use log::error;

#[cfg(target_os = "macos")]
use crate::audio::capture::AudioCaptureBackend;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct RecordingPreferences {
    pub save_folder: PathBuf,
    #[serde(default = "default_auto_save")]
    pub auto_save: bool,
    #[serde(default = "default_file_format")]
    pub file_format: String,
    #[serde(default)]
    pub preferred_mic_device: Option<String>,
    #[serde(default)]
    pub preferred_system_device: Option<String>,
    /// "Transcript only: do not save audio" for the next recordings (spec F-TO-1)
    #[serde(default)]
    pub transcript_only: bool,
    /// Keep Parakeet loaded while the app is open, so recordings start transcribing at once (PL3)
    #[serde(default = "default_keep_speech_model_ready")]
    pub keep_speech_model_ready: bool,
    #[cfg(target_os = "macos")]
    #[serde(default)]
    pub system_audio_backend: Option<String>,
}

fn default_auto_save() -> bool {
    true
}

fn default_keep_speech_model_ready() -> bool {
    true
}

fn default_file_format() -> String {
    "mp4".to_string()
}

impl Default for RecordingPreferences {
    fn default() -> Self {
        Self {
            save_folder: fallback_recordings_folder(),
            auto_save: true,
            file_format: "mp4".to_string(),
            preferred_mic_device: None,
            preferred_system_device: None,
            transcript_only: false,
            keep_speech_model_ready: true,
            #[cfg(target_os = "macos")]
            system_audio_backend: Some("coreaudio".to_string()),
        }
    }
}

/// Where recordings go when nothing else is known: this app's own data folder. (The app passes its
/// data folder where it can; this is the same place, found without it.)
pub fn fallback_recordings_folder() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(crate::brand_generated::BUNDLE_ID)
        .join("recordings")
}

/// The recordings folder the earlier app used by default. Adopted installs keep it as their
/// default and as a folder retention may act in (plan PR3).
pub fn earlier_default_folder() -> PathBuf {
    const EARLIER_FOLDER: &str = "meetily-recordings";
    #[cfg(target_os = "windows")]
    let base = dirs::audio_dir().or_else(dirs::document_dir);
    #[cfg(target_os = "macos")]
    let base = dirs::video_dir().or_else(dirs::document_dir);
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let base = dirs::document_dir();
    base.unwrap_or_else(|| PathBuf::from(".")).join(EARLIER_FOLDER)
}

/// Marker left by the data adoption of earlier builds (plan PR3).
const ADOPTED_MARKER: &str = ".adopted";

/// An install that adopted the earlier app's data and never chose a folder has been recording into
/// the earlier app's default folder. Returns the preferences with that folder written in, so nothing
/// moves when that default goes (plan PR3); None when there is nothing to do.
pub fn pinned_for_adopted(stored: Option<&RecordingPreferences>, adopted: bool, earlier: PathBuf) -> Option<RecordingPreferences> {
    if !adopted {
        return None;
    }
    match stored {
        Some(p) if !p.save_folder.as_os_str().is_empty() => None,
        Some(p) => Some(RecordingPreferences { save_folder: earlier, ..p.clone() }),
        None => Some(RecordingPreferences { save_folder: earlier, ..RecordingPreferences::default() }),
    }
}

/// True when this install adopted the earlier app's data.
pub fn is_adopted<R: Runtime>(app: &AppHandle<R>) -> bool {
    app_data_dir(app).map_or(false, |dir| dir.join(ADOPTED_MARKER).is_file())
}

/// What the preferences file on disk says, read directly: the store hides a read or parse error
/// behind an empty store, which must never be mistaken for "no preferences saved".
#[derive(Debug, PartialEq)]
pub enum StoredOnDisk {
    /// No file, or a file with no saved preferences
    Nothing,
    Saved(RecordingPreferences),
    /// The file exists but could not be read or understood: leave it alone
    Unreadable,
}

pub fn stored_on_disk(file: &Path) -> StoredOnDisk {
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return StoredOnDisk::Nothing,
        Err(_) => return StoredOnDisk::Unreadable,
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else { return StoredOnDisk::Unreadable };
    match json.get("preferences") {
        None => StoredOnDisk::Nothing,
        Some(value) => match serde_json::from_value::<RecordingPreferences>(value.clone()) {
            Ok(p) => StoredOnDisk::Saved(p),
            Err(_) => StoredOnDisk::Unreadable,
        },
    }
}

/// Runs once at start-up, before anything reads the recordings folder (plan PR3). Preferences that
/// exist but cannot be read are left alone, and the store must agree with the file before it is
/// written, so a store that failed to load never overwrites the person's settings.
pub fn pin_adopted_recordings_folder<R: Runtime>(app: &AppHandle<R>) {
    let Some(dir) = app_data_dir(app) else { return };
    if !dir.join(ADOPTED_MARKER).is_file() {
        return;
    }
    let stored = match stored_on_disk(&dir.join("recording_preferences.json")) {
        StoredOnDisk::Unreadable => return,
        StoredOnDisk::Nothing => None,
        StoredOnDisk::Saved(p) => Some(p),
    };
    let Ok(store) = app.store("recording_preferences.json") else { return };
    let in_store = store.get("preferences").map(serde_json::from_value::<RecordingPreferences>);
    let agrees = match (&stored, in_store) {
        (None, None) => true,
        (Some(file), Some(Ok(loaded))) => *file == loaded,
        _ => false,
    };
    if !agrees {
        return;
    }
    if let Some(pinned) = pinned_for_adopted(stored.as_ref(), true, earlier_default_folder()) {
        if let Ok(value) = serde_json::to_value(&pinned) {
            store.set("preferences", value);
            if store.save().is_ok() {
                info!("Kept the recordings folder this install already uses as its setting");
            }
        }
    }
}

/// Where recordings go: the saved choice when there is one, otherwise `<app data>/recordings`.
pub fn resolve_recordings_folder(stored: Option<&RecordingPreferences>, app_data_dir: &Path) -> PathBuf {
    match stored {
        Some(prefs) if !prefs.save_folder.as_os_str().is_empty() => prefs.save_folder.clone(),
        _ => app_data_dir.join("recordings"),
    }
}

/// A folder recordings can be written to: `preferred` when it can be created and written,
/// otherwise `fallback`. The flag is true when the fallback was used.
pub fn usable_recordings_folder(preferred: &Path, fallback: &Path) -> Result<(PathBuf, bool)> {
    if writable_folder(preferred).is_ok() {
        return Ok((preferred.to_path_buf(), false));
    }
    writable_folder(fallback)?;
    Ok((fallback.to_path_buf(), true))
}

/// Creates `folder` if needed and proves a file can be written there (the probe is removed).
fn writable_folder(folder: &Path) -> Result<()> {
    std::fs::create_dir_all(folder)?;
    let probe = folder.join(format!(".write-check-{}", std::process::id()));
    std::fs::write(&probe, b"")?;
    std::fs::remove_file(&probe)?;
    Ok(())
}

/// The folder for a recording that is starting: the saved one, or `<app data>/recordings` when the
/// saved one cannot be written (a drive that is not attached, for example). The flag reports a fallback.
pub async fn recordings_folder_for_new_recording<R: Runtime>(app: &AppHandle<R>) -> Result<(PathBuf, bool)> {
    let prefs = load_recording_preferences(app).await?;
    let fallback = app_data_dir(app)
        .map(|d| d.join("recordings"))
        .unwrap_or_else(fallback_recordings_folder);
    usable_recordings_folder(&prefs.save_folder, &fallback)
}

/// The cloud sync service whose folder contains `path`, if any (OneDrive, iCloud Drive, Dropbox,
/// Google Drive). On macOS iCloud Drive is `~/Library/Mobile Documents`, and the other services
/// live under `~/Library/CloudStorage`.
pub fn cloud_sync_provider(path: &Path) -> Option<&'static str> {
    let names: Vec<String> = path.components().map(|p| p.as_os_str().to_string_lossy().to_lowercase()).collect();
    names.iter().enumerate().find_map(|(i, name)| {
        let in_cloud_storage = i > 0 && names[i - 1] == "cloudstorage";
        if name == "onedrive" || name.starts_with("onedrive - ") || (in_cloud_storage && name.starts_with("onedrive")) {
            Some("OneDrive")
        } else if name == "iclouddrive" || name == "icloud drive" || name == "mobile documents" {
            Some("iCloud Drive")
        } else if name == "dropbox" || name.starts_with("dropbox (") || (in_cloud_storage && name.starts_with("dropbox")) {
            Some("Dropbox")
        } else if in_cloud_storage && name.starts_with("googledrive") {
            Some("Google Drive")
        } else {
            None
        }
    })
}

/// The app's data folder, or None when Tauri cannot resolve it.
fn app_data_dir<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path().app_data_dir().ok()
}

/// Preferences for an install that has saved none: recordings go to `<app data>/recordings`, or,
/// for an adopted install whose settings could not be read, to the folder it has always used.
fn defaults_for<R: Runtime>(app: &AppHandle<R>) -> RecordingPreferences {
    let mut prefs = RecordingPreferences::default();
    if is_adopted(app) {
        prefs.save_folder = earlier_default_folder();
    } else if let Some(dir) = app_data_dir(app) {
        prefs.save_folder = resolve_recordings_folder(None, &dir);
    }
    prefs
}

/// Ensure the recordings directory exists
pub fn ensure_recordings_directory(path: &PathBuf) -> Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path)?;
        // log-ok: the recordings folder, not a meeting folder
        info!("Created recordings directory: {:?}", path);
    }
    Ok(())
}

/// Generate a unique filename for a recording
pub fn generate_recording_filename(format: &str) -> String {
    let now = chrono::Utc::now();
    let timestamp = now.format("%Y%m%d_%H%M%S");
    format!("recording_{}.{}", timestamp, format)
}

/// Whether a recording that is starting saves audio. Transcript only, the old "save audio" switch
/// set off, or preferences that could not be read all mean no audio (spec F-TO-2).
pub fn saves_audio(prefs: Option<&RecordingPreferences>) -> bool {
    prefs.map_or(false, |p| p.auto_save && !p.transcript_only)
}

/// Load recording preferences from store
pub async fn load_recording_preferences<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<RecordingPreferences> {
    // Try to load from Tauri store
    let store = match app.store("recording_preferences.json") {
        Ok(store) => store,
        Err(e) => {
            warn!("Failed to access store: {}, using defaults", e);
            return Ok(defaults_for(app));
        }
    };

    // Try to get the preferences from store
    let prefs = if let Some(value) = store.get("preferences") {
        match serde_json::from_value::<RecordingPreferences>(value.clone()) {
            Ok(mut p) => {
                info!("Loaded recording preferences from store");
                // Update macOS backend to current value if needed
                #[cfg(target_os = "macos")]
                {
                    let backend = crate::audio::capture::get_current_backend();
                    p.system_audio_backend = Some(backend.to_string());
                }
                p
            }
            Err(e) => {
                warn!("Failed to deserialize preferences: {}, using defaults", e);
                // Keep a transcript-only choice even when the rest cannot be read (spec F-TO-2)
                let mut p = defaults_for(app);
                p.transcript_only = value.get("transcript_only").and_then(|v| v.as_bool()).unwrap_or(false);
                p
            }
        }
    } else {
        info!("No stored preferences found, using defaults");
        defaults_for(app)
    };
    // An empty saved folder means "not chosen": use the app's own folder
    let prefs = match app_data_dir(app) {
        Some(dir) => RecordingPreferences { save_folder: resolve_recordings_folder(Some(&prefs), &dir), ..prefs },
        None => prefs,
    };

    // log-ok: the recordings folder the person chose, not a meeting folder
    info!("Loaded recording preferences: save_folder={:?}, auto_save={}, format={}, mic={:?}, system={:?}",
          prefs.save_folder, prefs.auto_save, prefs.file_format,
          prefs.preferred_mic_device, prefs.preferred_system_device);
    Ok(prefs)
}

/// Save recording preferences to store
pub async fn save_recording_preferences<R: Runtime>(
    app: &AppHandle<R>,
    preferences: &RecordingPreferences,
) -> Result<()> {
    // log-ok: the recordings folder the person chose, not a meeting folder
    info!("Saving recording preferences: save_folder={:?}, auto_save={}, format={}, mic={:?}, system={:?}",
          preferences.save_folder, preferences.auto_save, preferences.file_format,
          preferences.preferred_mic_device, preferences.preferred_system_device);

    // A folder that cannot be written is refused before anything is stored
    writable_folder(&preferences.save_folder)
        .map_err(|e| anyhow::anyhow!("Recordings cannot be saved in {}: {}", preferences.save_folder.display(), e))?;

    // Get or create store
    let store = app
        .store("recording_preferences.json")
        .map_err(|e| anyhow::anyhow!("Failed to access store: {}", e))?;

    // Serialize preferences to JSON value
    let prefs_value = serde_json::to_value(preferences)
        .map_err(|e| anyhow::anyhow!("Failed to serialize preferences: {}", e))?;

    // Save to store
    store.set("preferences", prefs_value);

    // Persist to disk
    store
        .save()
        .map_err(|e| anyhow::anyhow!("Failed to save store to disk: {}", e))?;

    info!("Successfully persisted recording preferences to disk");

    // Save backend preference to global config
    #[cfg(target_os = "macos")]
    if let Some(backend_str) = &preferences.system_audio_backend {
        if let Some(backend) = AudioCaptureBackend::from_string(backend_str) {
            info!("Setting audio capture backend to: {:?}", backend);
            crate::audio::capture::set_current_backend(backend);
        }
    }

    Ok(())
}

/// Tauri commands for recording preferences
#[tauri::command]
pub async fn get_recording_preferences<R: Runtime>(
    app: AppHandle<R>,
) -> Result<RecordingPreferences, String> {
    load_recording_preferences(&app)
        .await
        .map_err(|e| format!("Failed to load recording preferences: {}", e))
}

#[tauri::command]
pub async fn set_recording_preferences<R: Runtime>(
    app: AppHandle<R>,
    preferences: RecordingPreferences,
) -> Result<(), String> {
    save_recording_preferences(&app, &preferences)
        .await
        .map_err(|e| format!("Failed to save recording preferences: {}", e))
}

#[tauri::command]
pub async fn get_default_recordings_folder_path<R: Runtime>(app: AppHandle<R>) -> Result<String, String> {
    Ok(defaults_for(&app).save_folder.to_string_lossy().to_string())
}

/// A folder the person picked, and the cloud sync service it lives in (for a warning).
#[derive(Debug, Serialize)]
pub struct RecordingFolderChoice {
    pub path: String,
    pub cloud_provider: Option<String>,
}

#[tauri::command]
pub async fn open_recordings_folder<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let preferences = load_recording_preferences(&app)
        .await
        .map_err(|e| format!("Failed to load preferences: {}", e))?;

    // Ensure directory exists before trying to open it
    ensure_recordings_directory(&preferences.save_folder)
        .map_err(|e| format!("Failed to create directory: {}", e))?;

    let folder_path = preferences.save_folder.to_string_lossy().to_string();

    #[cfg(target_os = "windows")]
    {
        // net: none: opens the recordings folder in the file manager
        std::process::Command::new("explorer")
            .arg(&folder_path)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        // net: none: opens the recordings folder in the file manager
        std::process::Command::new("open")
            .arg(&folder_path)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        // net: none: opens the recordings folder in the file manager
        std::process::Command::new("xdg-open")
            .arg(&folder_path)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    // log-ok: the recordings folder, not a meeting folder
    info!("Opened recordings folder: {}", folder_path);
    Ok(())
}

/// Opens a folder picker. Returns None when the person cancels. Saving is a separate step,
/// so the UI can show the cloud sync warning first.
#[tauri::command]
pub async fn select_recording_folder<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Option<RecordingFolderChoice>, String> {
    let current = load_recording_preferences(&app).await.ok().map(|p| p.save_folder);
    let app_clone = app.clone();
    let picked = tokio::task::spawn_blocking(move || {
        let mut dialog = app_clone.dialog().file().set_title("Choose where recordings are saved");
        if let Some(dir) = current.filter(|d| d.is_dir()) {
            dialog = dialog.set_directory(dir);
        }
        dialog.blocking_pick_folder()
    })
    .await
    .map_err(|e| format!("Folder dialog failed: {}", e))?;

    let Some(picked) = picked else { return Ok(None) };
    let path = picked
        .into_path()
        .map_err(|e| format!("The chosen folder is not a local folder: {}", e))?;
    Ok(Some(RecordingFolderChoice {
        cloud_provider: cloud_sync_provider(&path).map(str::to_string),
        path: path.to_string_lossy().to_string(),
    }))
}

// Backend selection commands

/// Get available audio capture backends for the current platform
#[tauri::command]
pub async fn get_available_audio_backends() -> Result<Vec<String>, String> {
    #[cfg(target_os = "macos")]
    {
        let backends = crate::audio::capture::get_available_backends();
        Ok(backends.iter().map(|b| b.to_string()).collect())
    }

    #[cfg(not(target_os = "macos"))]
    {
        // Only ScreenCaptureKit available on non-macOS
        Ok(vec!["screencapturekit".to_string()])
    }
}

/// Get current audio capture backend
#[tauri::command]
pub async fn get_current_audio_backend() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        let backend = crate::audio::capture::get_current_backend();
        Ok(backend.to_string())
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok("screencapturekit".to_string())
    }
}

/// Set audio capture backend
#[tauri::command]
pub async fn set_audio_backend(backend: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use crate::audio::capture::AudioCaptureBackend;
        use crate::audio::permissions::{
            check_screen_recording_permission, request_screen_recording_permission,
        };

        let backend_enum = AudioCaptureBackend::from_string(&backend)
            .ok_or_else(|| format!("Invalid backend: {}", backend))?;

        // If switching to Core Audio, log information about Audio Capture permission
        if backend_enum == AudioCaptureBackend::CoreAudio {
            info!("🔐 Core Audio backend requires Audio Capture permission (macOS 14.4+)");
            info!("📍 Permission dialog will appear automatically when recording starts");

            // Check if permission is already granted (this is informational only)
            if !check_screen_recording_permission() {
                warn!("⚠️  Audio Capture permission may not be granted");

                // Attempt to open System Settings (opens System Settings)
                if let Err(e) = request_screen_recording_permission() {
                    error!("Failed to open System Settings: {}", e);
                }

                return Err(
                    "Core Audio requires Audio Capture permission. \
                    The permission dialog will appear when you start recording. \
                    If already denied, enable it in System Settings → Privacy & Security → Audio Capture, \
                    then restart the app.".to_string()
                );
            }

            info!(
                "✅ Core Audio backend selected - permission check will occur at recording start"
            );
        }

        info!("Setting audio backend to: {:?}", backend_enum);
        crate::audio::capture::set_current_backend(backend_enum);
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    {
        if backend != "screencapturekit" {
            return Err(format!(
                "Backend {} not available on this platform",
                backend
            ));
        }
        Ok(())
    }
}

/// Get backend information (name and description)
#[derive(Serialize)]
pub struct BackendInfo {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[tauri::command]
pub async fn get_audio_backend_info() -> Result<Vec<BackendInfo>, String> {
    #[cfg(target_os = "macos")]
    {
        use crate::audio::capture::AudioCaptureBackend;

        let backends = vec![
            BackendInfo {
                id: AudioCaptureBackend::ScreenCaptureKit.to_string(),
                name: AudioCaptureBackend::ScreenCaptureKit.name().to_string(),
                description: AudioCaptureBackend::ScreenCaptureKit
                    .description()
                    .to_string(),
            },
            BackendInfo {
                id: AudioCaptureBackend::CoreAudio.to_string(),
                name: AudioCaptureBackend::CoreAudio.name().to_string(),
                description: AudioCaptureBackend::CoreAudio.description().to_string(),
            },
        ];
        Ok(backends)
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(vec![BackendInfo {
            id: "screencapturekit".to_string(),
            name: "ScreenCaptureKit".to_string(),
            description: "Default system audio capture".to_string(),
        }])
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_speech_model_is_kept_ready_unless_turned_off() {
        // Preferences saved before PL3 have no such field
        let old: super::RecordingPreferences = serde_json::from_str(r#"{"save_folder":"/tmp/r","auto_save":true}"#).unwrap();
        assert!(old.keep_speech_model_ready);
        assert!(super::RecordingPreferences::default().keep_speech_model_ready);
        let off: super::RecordingPreferences =
            serde_json::from_str(r#"{"save_folder":"/tmp/r","keep_speech_model_ready":false}"#).unwrap();
        assert!(!off.keep_speech_model_ready);
    }

    #[test]
    fn the_pin_reads_the_file_itself_and_leaves_an_unreadable_one_alone() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("recording_preferences.json");
        assert_eq!(stored_on_disk(&file), StoredOnDisk::Nothing, "no file yet");

        std::fs::write(&file, "{}").unwrap();
        assert_eq!(stored_on_disk(&file), StoredOnDisk::Nothing, "a file with no preferences saved");

        let mut chosen = prefs("D:/recs");
        chosen.transcript_only = true;
        std::fs::write(&file, serde_json::json!({ "preferences": chosen }).to_string()).unwrap();
        assert_eq!(stored_on_disk(&file), StoredOnDisk::Saved(chosen));

        std::fs::write(&file, "{ not json").unwrap();
        assert_eq!(stored_on_disk(&file), StoredOnDisk::Unreadable, "a damaged file is never treated as empty");
        std::fs::write(&file, r#"{ "preferences": 5 }"#).unwrap();
        assert_eq!(stored_on_disk(&file), StoredOnDisk::Unreadable, "preferences that do not parse are left alone");
    }

    use super::*;

    fn prefs(folder: &str) -> RecordingPreferences {
        RecordingPreferences { save_folder: PathBuf::from(folder), ..RecordingPreferences::default() }
    }

    #[test]
    fn a_new_install_records_into_the_app_data_folder() {
        let app_data = Path::new("C:/Users/me/AppData/Roaming/com.openedtools.notes");
        assert_eq!(resolve_recordings_folder(None, app_data), app_data.join("recordings"));
    }

    #[test]
    fn a_saved_folder_is_used_as_it_is() {
        let app_data = Path::new("C:/data");
        let saved = prefs("D:/Meetings/Recordings");
        assert_eq!(resolve_recordings_folder(Some(&saved), app_data), PathBuf::from("D:/Meetings/Recordings"));
    }

    #[test]
    fn an_empty_saved_folder_falls_back_to_the_app_data_folder() {
        let app_data = Path::new("C:/data");
        let saved = prefs("");
        assert_eq!(resolve_recordings_folder(Some(&saved), app_data), app_data.join("recordings"));
    }

    #[test]
    fn an_adopted_install_without_a_saved_folder_gets_its_folder_pinned_once() {
        let earlier = PathBuf::from("C:/Users/me/Music/old-recordings");
        let pinned = pinned_for_adopted(None, true, earlier.clone()).expect("pinned");
        assert_eq!(pinned.save_folder, earlier);
        let mut empty = prefs("");
        empty.auto_save = false;
        let pinned = pinned_for_adopted(Some(&empty), true, earlier.clone()).expect("pinned");
        assert_eq!(pinned.save_folder, earlier);
        assert!(!pinned.auto_save, "the other settings are kept");
        assert_eq!(pinned_for_adopted(Some(&prefs("D:/recs")), true, earlier.clone()), None, "a chosen folder is never changed");
        assert_eq!(pinned_for_adopted(None, false, earlier), None, "a fresh install is not touched");
    }

    #[test]
    fn a_saved_store_with_only_a_folder_still_loads() {
        let prefs: RecordingPreferences = serde_json::from_str(r#"{"save_folder":"D:\\recs"}"#).unwrap();
        assert_eq!(prefs.save_folder, PathBuf::from("D:\\recs"));
        assert!(prefs.auto_save);
        assert_eq!(prefs.file_format, "mp4");
    }

    #[test]
    fn an_unusable_folder_falls_back_and_says_so() {
        let root = tempfile::tempdir().unwrap();
        let blocker = root.path().join("not-a-folder");
        std::fs::write(&blocker, b"a file where the folder should be").unwrap();
        let fallback = root.path().join("fallback");
        let (folder, fell_back) = usable_recordings_folder(&blocker.join("recordings"), &fallback).unwrap();
        assert_eq!(folder, fallback);
        assert!(fell_back);
        assert!(fallback.is_dir());
    }

    #[test]
    fn a_usable_folder_is_created_and_kept() {
        let root = tempfile::tempdir().unwrap();
        let preferred = root.path().join("D").join("Meetings");
        let (folder, fell_back) = usable_recordings_folder(&preferred, &root.path().join("fallback")).unwrap();
        assert_eq!(folder, preferred);
        assert!(!fell_back);
        assert!(preferred.is_dir());
        assert_eq!(std::fs::read_dir(&preferred).unwrap().count(), 0, "the write check leaves nothing behind");
    }

    #[test]
    fn cloud_synced_folders_are_recognised() {
        assert_eq!(cloud_sync_provider(Path::new("C:/Users/me/OneDrive/Recordings")), Some("OneDrive"));
        assert_eq!(cloud_sync_provider(Path::new("C:/Users/me/OneDrive - Contoso/Recordings")), Some("OneDrive"));
        assert_eq!(cloud_sync_provider(Path::new("C:/Users/me/onedrive/x")), Some("OneDrive"));
        assert_eq!(cloud_sync_provider(Path::new("C:/Users/me/iCloudDrive/x")), Some("iCloud Drive"));
        assert_eq!(cloud_sync_provider(Path::new("C:/Users/me/iCloud Drive/x")), Some("iCloud Drive"));
        assert_eq!(cloud_sync_provider(Path::new("C:/Users/me/Dropbox/Meetings")), Some("Dropbox"));
    }

    #[test]
    fn audio_is_saved_only_when_the_preferences_say_so() {
        let mut p = RecordingPreferences::default();
        assert!(saves_audio(Some(&p)));
        p.transcript_only = true;
        assert!(!saves_audio(Some(&p)));
        p.transcript_only = false;
        p.auto_save = false;
        assert!(!saves_audio(Some(&p)));
        assert!(!saves_audio(None), "unreadable preferences fail closed: no audio");
    }

    #[test]
    fn macos_cloud_folders_are_recognised() {
        assert_eq!(
            cloud_sync_provider(Path::new("/Users/me/Library/Mobile Documents/iCloud~md~obsidian/Documents/Vault")),
            Some("iCloud Drive")
        );
        assert_eq!(cloud_sync_provider(Path::new("/Users/me/Library/CloudStorage/OneDrive-Personal/x")), Some("OneDrive"));
        assert_eq!(cloud_sync_provider(Path::new("/Users/me/Library/CloudStorage/Dropbox/x")), Some("Dropbox"));
        assert_eq!(cloud_sync_provider(Path::new("/Users/me/Library/CloudStorage/GoogleDrive-me@example.com/x")), Some("Google Drive"));
        assert_eq!(cloud_sync_provider(Path::new("/Users/me/Documents/Obsidian/Vault")), None);
    }

    #[test]
    fn ordinary_folders_and_look_alike_names_are_not_cloud_synced() {
        assert_eq!(cloud_sync_provider(Path::new("C:/Users/me/Documents/Recordings")), None);
        assert_eq!(cloud_sync_provider(Path::new("D:/onedrive-notes/x")), None);
        assert_eq!(cloud_sync_provider(Path::new("C:/Users/me/Documents/Dropboxes")), None);
    }
}
