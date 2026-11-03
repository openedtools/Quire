use std::sync::{Arc, Mutex};
use tokio::sync::Mutex as AsyncMutex;
use anyhow::Result;
use log::{info, warn, error};
use tauri::{AppHandle, Runtime, Emitter};
use tokio::sync::mpsc;
use serde::{Serialize, Deserialize};
use std::path::PathBuf;

use super::recording_state::AudioChunk;
use super::audio_processing::create_meeting_folder;
use super::incremental_saver::IncrementalAudioSaver;

/// Structured transcript segment for JSON export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub id: String,
    pub text: String,
    pub audio_start_time: f64, // Seconds from recording start
    pub audio_end_time: f64,   // Seconds from recording start
    pub duration: f64,          // Segment duration in seconds
    pub display_time: String,   // Formatted time for display like "[02:15]"
    pub confidence: f32,
    pub sequence_id: u64,
}

/// Meeting metadata structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingMetadata {
    pub version: String,
    pub meeting_id: Option<String>,
    pub meeting_name: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub duration_seconds: Option<f64>,
    pub devices: DeviceInfo,
    pub audio_file: String,
    pub transcript_file: String,
    pub sample_rate: u32,
    pub status: String,  // "recording", "completed", "error"
    /// `audio` or `transcript_only`: whether audio was ever written (spec F-TO-2)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub microphone: Option<String>,
    pub system_audio: Option<String>,
}

/// What stopping a recording produced.
pub(crate) struct Saved {
    pub audio_path: Option<String>,
    pub event: serde_json::Value,
}

/// New recording saver using incremental saving strategy
pub struct RecordingSaver {
    incremental_saver: Option<Arc<AsyncMutex<IncrementalAudioSaver>>>,
    meeting_folder: Option<PathBuf>,
    meeting_name: Option<String>,
    metadata: Option<MeetingMetadata>,
    transcript_segments: Arc<Mutex<Vec<TranscriptSegment>>>,
    is_saving: Arc<Mutex<bool>>,
    /// Folder that holds one sub-folder per meeting (the saved recordings preference)
    recordings_root: Option<PathBuf>,
}

impl RecordingSaver {
    pub fn new() -> Self {
        Self {
            incremental_saver: None,
            meeting_folder: None,
            meeting_name: None,
            metadata: None,
            transcript_segments: Arc::new(Mutex::new(Vec::new())),
            is_saving: Arc::new(Mutex::new(false)),
            recordings_root: None,
        }
    }

    /// Set the folder new meeting folders are created in (from the recordings preference)
    pub fn set_recordings_root(&mut self, root: PathBuf) {
        self.recordings_root = Some(root);
    }

    /// Set the meeting name for this recording session
    pub fn set_meeting_name(&mut self, name: Option<String>) {
        self.meeting_name = name;
    }

    /// Set device information in metadata
    pub fn set_device_info(&mut self, mic_name: Option<String>, sys_name: Option<String>) {
        if let Some(ref mut metadata) = self.metadata {
            metadata.devices.microphone = mic_name;
            metadata.devices.system_audio = sys_name;

            // Write updated metadata to disk if folder exists
            if let Some(folder) = &self.meeting_folder {
                let metadata_clone = metadata.clone();
                if let Err(e) = self.write_metadata(folder, &metadata_clone) {
                    warn!("Failed to update metadata with device info: {}", e);
                }
            }
        }
    }

    /// Add or update a structured transcript segment (upserts based on sequence_id)
    /// Also saves incrementally to disk
    pub fn add_transcript_segment(&self, segment: TranscriptSegment) {
        if let Ok(mut segments) = self.transcript_segments.lock() {
            // Check if segment with same sequence_id exists (update it)
            if let Some(existing) = segments.iter_mut().find(|s| s.sequence_id == segment.sequence_id) {
                *existing = segment.clone();
                // log-ok: segment ids and counts only
                info!("Updated transcript segment {} (seq: {}) - total segments: {}",
                      segment.id, segment.sequence_id, segments.len());
            } else {
                // New segment, add it
                segments.push(segment.clone());
                // log-ok: segment ids and counts only
                info!("Added new transcript segment {} (seq: {}) - total segments: {}",
                      segment.id, segment.sequence_id, segments.len());
            }
        } else {
            // log-ok: segment id only
            error!("Failed to lock transcript segments for adding segment {}", segment.id);
        }

        // NEW: Save incrementally to disk
        if let Some(folder) = &self.meeting_folder {
            if let Err(e) = self.write_transcripts_json(folder) {
                warn!("Failed to write incremental transcript update: {}", e);
            }
        }
    }

    /// Legacy method for backward compatibility - converts text to basic segment
    pub fn add_transcript_chunk(&self, text: String) {
        let segment = TranscriptSegment {
            id: format!("seg_{}", chrono::Utc::now().timestamp_millis()),
            text,
            audio_start_time: 0.0,
            audio_end_time: 0.0,
            duration: 0.0,
            display_time: "[00:00]".to_string(),
            confidence: 1.0,
            sequence_id: 0,
        };
        self.add_transcript_segment(segment);
    }

    /// Start accumulation with optional incremental saving
    ///
    /// # Arguments
    /// * `auto_save` - If true, creates checkpoints and enables saving. If false, audio chunks are discarded.
    pub fn start_accumulation(
        &mut self,
        auto_save: bool,
        mut receiver: mpsc::UnboundedReceiver<AudioChunk>,
    ) {
        if auto_save {
            info!("Initializing incremental audio saver for recording (auto-save ENABLED)");
        } else {
            info!("Starting recording without audio saving (auto-save DISABLED - transcripts only)");
        }

        // Initialize meeting folder and incremental saver ONLY if auto_save is enabled
        if auto_save {
            if let Some(name) = self.meeting_name.clone() {
                match self.initialize_meeting_folder(&name, true) {
                    Ok(()) => info!("Successfully initialized meeting folder with checkpoints"),
                    Err(e) => {
                        error!("Failed to initialize meeting folder: {}", e);
                        // Continue anyway - will use fallback flat structure
                    }
                }
            }
        } else {
            // When auto_save is false, still create meeting folder for transcripts/metadata
            // but skip .checkpoints directory
            if let Some(name) = self.meeting_name.clone() {
                match self.initialize_meeting_folder(&name, false) {
                    Ok(()) => info!("Successfully initialized meeting folder (transcripts only)"),
                    Err(e) => {
                        error!("Failed to initialize meeting folder: {}", e);
                    }
                }
            }
        }

        // Start accumulation task
        let is_saving_clone = self.is_saving.clone();
        let incremental_saver_arc = self.incremental_saver.clone();
        let save_audio = auto_save;

        tokio::spawn(async move {
            info!("Recording saver accumulation task started (save_audio: {})", save_audio);

            while let Some(chunk) = receiver.recv().await {
                // Check if we should continue
                let should_continue = if let Ok(is_saving) = is_saving_clone.lock() {
                    *is_saving
                } else {
                    false
                };

                if !should_continue {
                    break;
                }

                // Only process audio chunks if auto_save is enabled
                if save_audio {
                    // Add chunk to incremental saver
                    if let Some(saver_arc) = &incremental_saver_arc {
                        let mut saver_guard = saver_arc.lock().await;
                        if let Err(e) = saver_guard.add_chunk(chunk) {
                            error!("Failed to add chunk to incremental saver: {}", e);
                        }
                    } else {
                        error!("Incremental saver not available while accumulating");
                    }
                } else {
                    // auto_save is false: discard audio chunk (no-op)
                    // Transcription already happened in the pipeline before this point
                }
            }

            info!("Recording saver accumulation task ended");
        });

        // Set saving flag
        if let Ok(mut is_saving) = self.is_saving.lock() {
            *is_saving = true;
        }
    }

    /// Initialize meeting folder structure and metadata
    ///
    /// # Arguments
    /// * `meeting_name` - Name of the meeting
    /// * `create_checkpoints` - Whether to create .checkpoints/ directory and IncrementalAudioSaver
    fn initialize_meeting_folder(&mut self, meeting_name: &str, create_checkpoints: bool) -> Result<()> {
        // The recordings preference, set by the recording command; the old default only as a last resort
        let base_folder = self
            .recordings_root
            .clone()
            .unwrap_or_else(super::recording_preferences::fallback_recordings_folder);

        // Create meeting folder structure (with or without .checkpoints/ subdirectory)
        let meeting_folder = create_meeting_folder(&base_folder, meeting_name, create_checkpoints)?;

        // Only initialize incremental saver if checkpoints are needed (auto_save is true)
        if create_checkpoints {
            let incremental_saver = IncrementalAudioSaver::new(meeting_folder.clone(), 48000)?;
            self.incremental_saver = Some(Arc::new(AsyncMutex::new(incremental_saver)));
            info!("Incremental audio saver initialized");
        } else {
            info!("⚠️  Skipped incremental audio saver (auto-save disabled)");
        }

        // Create initial metadata
        let metadata = MeetingMetadata {
            version: "1.0".to_string(),
            meeting_id: None,  // Will be set by backend
            meeting_name: Some(meeting_name.to_string()),
            created_at: chrono::Utc::now().to_rfc3339(),
            completed_at: None,
            duration_seconds: None,
            devices: DeviceInfo {
                microphone: None,  // Could be enhanced to store actual device names
                system_audio: None,
            },
            audio_file: if create_checkpoints { "audio.mp4".to_string() } else { "".to_string() },
            transcript_file: "transcripts.json".to_string(),
            sample_rate: 48000,
            status: "recording".to_string(),
            mode: Some(if create_checkpoints { "audio" } else { "transcript_only" }.to_string()),
        };

        // Write initial metadata.json
        self.write_metadata(&meeting_folder, &metadata)?;

        self.meeting_folder = Some(meeting_folder);
        self.metadata = Some(metadata);

        Ok(())
    }

    /// Write metadata.json to disk (atomic write with temp file)
    fn write_metadata(&self, folder: &PathBuf, metadata: &MeetingMetadata) -> Result<()> {
        let metadata_path = folder.join("metadata.json");
        let temp_path = folder.join(".metadata.json.tmp");

        let json_string = serde_json::to_string_pretty(metadata)?;
        std::fs::write(&temp_path, json_string)?;
        std::fs::rename(&temp_path, &metadata_path)?;  // Atomic

        Ok(())
    }

    /// Write transcripts.json to disk (atomic write with temp file and validation)
    fn write_transcripts_json(&self, folder: &PathBuf) -> Result<()> {
        // Clone segments to avoid holding lock during I/O
        let segments_clone = if let Ok(segments) = self.transcript_segments.lock() {
            segments.clone()
        } else {
            error!("Failed to lock transcript segments for writing");
            return Err(anyhow::anyhow!("Failed to lock transcript segments"));
        };

        info!("Writing {} transcript segments to JSON", segments_clone.len());

        let transcript_path = folder.join("transcripts.json");
        let temp_path = folder.join(".transcripts.json.tmp");

        // Create JSON structure
        let json = serde_json::json!({
            "version": "1.0",
            "segments": segments_clone,
            "last_updated": chrono::Utc::now().to_rfc3339(),
            "total_segments": segments_clone.len()
        });

        // Serialize to pretty JSON string
        let json_string = serde_json::to_string_pretty(&json)
            .map_err(|e| {
                error!("Failed to serialize transcripts to JSON: {}", e);
                anyhow::anyhow!("JSON serialization failed: {}", e)
            })?;

        // Write to temp file with error handling
        std::fs::write(&temp_path, &json_string)
            .map_err(|e| {
                error!("Failed to write the transcript temp file: {}", e);
                anyhow::anyhow!("Failed to write temp file: {}", e)
            })?;

        // Verify temp file was written correctly
        if !temp_path.exists() {
            error!("The transcript temp file does not exist after writing");
            return Err(anyhow::anyhow!("Temp file verification failed"));
        }

        // Atomic rename
        std::fs::rename(&temp_path, &transcript_path)
            .map_err(|e| {
                error!("Failed to move the transcript file into place: {}", e);
                anyhow::anyhow!("Failed to rename transcript file: {}", e)
            })?;

        info!("✅ Successfully wrote transcripts.json with {} segments", segments_clone.len());
        Ok(())
    }

    // in frontend/src-tauri/src/audio/recording_saver.rs
    pub fn get_stats(&self) -> (usize, u32) {
        if let Some(ref saver) = self.incremental_saver {
            if let Ok(guard) = saver.try_lock() {
                (guard.get_checkpoint_count() as usize, 48000)
            } else {
                (0, 48000)
            }
        } else {
            (0, 48000)
        }
    }

    /// Stop and save using incremental saving approach
    ///
    /// # Arguments
    /// * `app` - Tauri app handle for emitting events
    /// * `recording_duration` - Actual recording duration in seconds (from RecordingState)
    pub async fn stop_and_save<R: Runtime>(
        &mut self,
        app: &AppHandle<R>,
        recording_duration: Option<f64>
    ) -> Result<Option<String>, String> {
        let saved = self.finish(recording_duration).await?;
        if let Err(e) = app.emit("recording-saved", &saved.event) {
            warn!("Failed to emit recording-saved event: {}", e);
        }
        Ok(saved.audio_path)
    }

    /// Stops accumulating and completes the recording folder: the final audio (when audio is
    /// saved), the final transcripts.json, and metadata.json marked completed with the duration.
    /// A transcript-only recording is completed the same way, with no audio (spec F-TO-2).
    pub(crate) async fn finish(&mut self, recording_duration: Option<f64>) -> Result<Saved, String> {
        info!("Stopping recording saver");

        // Stop accumulation
        if let Ok(mut is_saving) = self.is_saving.lock() {
            *is_saving = false;
        }

        // Give time for final chunks
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // Finalize incremental saver (merge checkpoints into final audio.mp4); none in transcript-only mode
        let final_audio_path = match &self.incremental_saver {
            Some(saver_arc) => {
                let mut saver = saver_arc.lock().await;
                match saver.finalize().await {
                    Ok(path) => {
                        info!("Finalized the recording's audio");
                        Some(path)
                    }
                    Err(e) => {
                        error!("❌ Failed to finalize incremental saver: {}", e);
                        return Err(format!("Failed to finalize audio: {}", e));
                    }
                }
            }
            None => {
                info!("No audio saved for this recording (transcript only)");
                None
            }
        };

        // Save final transcripts.json with validation
        if let Some(folder) = &self.meeting_folder {
            if let Err(e) = self.write_transcripts_json(folder) {
                error!("❌ Failed to write final transcripts: {}", e);
                return Err(format!("Failed to save transcripts: {}", e));
            }

            // Verify transcripts were written correctly
            let transcript_path = folder.join("transcripts.json");
            if !transcript_path.exists() {
                error!("Transcript file was not created");
                return Err("Transcript file verification failed".to_string());
            }
            info!("Transcripts saved and verified");
        }

        // Update metadata to completed status with actual recording duration
        if let (Some(folder), Some(mut metadata)) = (&self.meeting_folder, self.metadata.clone()) {
            metadata.status = "completed".to_string();
            metadata.completed_at = Some(chrono::Utc::now().to_rfc3339());

            // Use actual recording duration from RecordingState (more accurate than transcript segments)
            // Falls back to last transcript segment if duration not provided
            metadata.duration_seconds = recording_duration.or_else(|| {
                if let Ok(segments) = self.transcript_segments.lock() {
                    segments.last().map(|seg| seg.audio_end_time)
                } else {
                    None
                }
            });

            if let Err(e) = self.write_metadata(folder, &metadata) {
                error!("❌ Failed to update metadata to completed: {}", e);
                return Err(format!("Failed to update metadata: {}", e));
            }
            self.metadata = Some(metadata.clone());

            info!("✅ Metadata updated with duration: {:?}s", metadata.duration_seconds);
        }

        // Save event with audio and transcript paths
        let event = serde_json::json!({
            "audio_file": final_audio_path.as_ref().map(|p| p.to_string_lossy().to_string()),
            "transcript_file": self.meeting_folder.as_ref()
                .map(|f| f.join("transcripts.json").to_string_lossy().to_string()),
            "meeting_name": self.meeting_name,
            "meeting_folder": self.meeting_folder.as_ref()
                .map(|f| f.to_string_lossy().to_string()),
            "mode": if final_audio_path.is_some() { "audio" } else { "transcript_only" }
        });

        // Clean up transcript segments
        if let Ok(mut segments) = self.transcript_segments.lock() {
            segments.clear();
        }

        Ok(Saved { audio_path: final_audio_path.map(|p| p.to_string_lossy().to_string()), event })
    }

    /// Get the meeting folder path (for passing to backend)
    pub fn get_meeting_folder(&self) -> Option<&PathBuf> {
        self.meeting_folder.as_ref()
    }

    /// Get accumulated transcript segments (for reload sync)
    pub fn get_transcript_segments(&self) -> Vec<TranscriptSegment> {
        if let Ok(segments) = self.transcript_segments.lock() {
            segments.clone()
        } else {
            Vec::new()
        }
    }

    /// Get meeting name (for reload sync)
    pub fn get_meeting_name(&self) -> Option<String> {
        self.meeting_name.clone()
    }
}

impl Default for RecordingSaver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meeting_folders_are_created_in_the_chosen_recordings_folder() {
        let root = tempfile::tempdir().unwrap();
        let mut saver = RecordingSaver::new();
        saver.set_recordings_root(root.path().to_path_buf());
        saver.initialize_meeting_folder("Weekly sync", false).unwrap();
        let folder = saver.meeting_folder.clone().expect("a meeting folder is set");
        assert!(folder.starts_with(root.path()), "{} is not inside {}", folder.display(), root.path().display());
        assert!(folder.join("metadata.json").is_file());
    }

    /// AC-TO-1: a transcript-only recording never creates an audio, checkpoint or temp audio file,
    /// watched with a real file watcher (not polling) on the recordings folder and the temp folder.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn transcript_only_never_writes_audio_and_still_completes_the_recording() {
        use notify::{RecursiveMode, Watcher};
        let root = tempfile::tempdir().unwrap();
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::<std::path::PathBuf>::new()));
        let sink = events.clone();
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res {
                if matches!(event.kind, notify::EventKind::Create(_) | notify::EventKind::Modify(_)) {
                    sink.lock().unwrap().extend(event.paths);
                }
            }
        })
        .unwrap();
        watcher.watch(root.path(), RecursiveMode::Recursive).unwrap();
        watcher.watch(&std::env::temp_dir(), RecursiveMode::NonRecursive).unwrap();

        let mut saver = RecordingSaver::new();
        saver.set_recordings_root(root.path().to_path_buf());
        saver.set_meeting_name(Some("Planning".to_string()));
        let (tx, rx) = mpsc::unbounded_channel();
        saver.start_accumulation(false, rx);
        assert!(saver.incremental_saver.is_none(), "the audio saver is never built");

        // Two minutes of synthetic audio in 100 ms chunks, plus transcript lines
        for i in 0..1200u64 {
            let data: Vec<f32> = (0..4800).map(|n| ((n as f32) * 0.05).sin() * 0.2).collect();
            use crate::audio::recording_state::DeviceType;
            let device_type = if i % 2 == 0 { DeviceType::Microphone } else { DeviceType::System };
            tx.send(AudioChunk { data, sample_rate: 48000, timestamp: i as f64 * 0.1, chunk_id: i, device_type }).unwrap();
        }
        for i in 0..3u64 {
            saver.add_transcript_segment(TranscriptSegment {
                id: format!("s{i}"),
                text: "fictional words".to_string(),
                audio_start_time: i as f64 * 10.0,
                audio_end_time: i as f64 * 10.0 + 5.0,
                duration: 5.0,
                display_time: "[00:00]".to_string(),
                confidence: 0.9,
                sequence_id: i,
            });
        }
        let saved = saver.finish(Some(120.0)).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        drop(watcher);

        let folder = saver.get_meeting_folder().cloned().unwrap();
        let audio_like: Vec<_> = events
            .lock()
            .unwrap()
            .iter()
            .filter(|p| p.starts_with(root.path()) || p.parent() == Some(std::env::temp_dir().as_path()))
            .filter(|p| {
                let name = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
                let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                crate::audio::constants::AUDIO_EXTENSIONS.contains(&ext.as_str())
                    || name.starts_with(crate::brand_generated::TEMP_PREFIX)
                    || name == ".checkpoints"
                    || p.components().any(|c| c.as_os_str() == ".checkpoints")
            })
            .cloned()
            .collect();
        assert!(audio_like.is_empty(), "audio-like files appeared: {audio_like:?}");
        let seen_transcript = events.lock().unwrap().iter().any(|p| p.ends_with("transcripts.json"));
        assert!(seen_transcript, "the watcher saw the transcript being written, so it was really watching");
        assert!(saved.audio_path.is_none());
        assert_eq!(saved.event["mode"], "transcript_only");

        // The recording is still completed: transcript, duration, status and mode
        assert!(folder.join("transcripts.json").is_file());
        let metadata: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(folder.join("metadata.json")).unwrap()).unwrap();
        assert_eq!(metadata["status"], "completed");
        assert_eq!(metadata["duration_seconds"], 120.0);
        assert_eq!(metadata["mode"], "transcript_only");
        assert!(!crate::retention::files::has_audio(&folder));
    }
}
