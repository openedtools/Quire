# Spec: Quire desktop and shared vault (from intent.md 2026-11-01)

Status: accepted by the owner on 2026-11-01 (draft 2, revised after an independent review, section 10). Date: 2026-11-01. Mode: solo.
Working name: Quire (backup: Marginalia). Companion file: `sync-format.md` (the shared vault contract).
Use limit: personal use only.

## 0. How this spec relates to the intent

* `intent.md` is centred on the iPhone app and lists the desktop as "parity" for three features (live Q&A, transcript-only, retention). The chat brief adds desktop work that intent.md does not mention: a rename, a visual re-skin, a consent reminder redesign and a model-choice screen.
* This spec covers the desktop app and the shared vault format. The iPhone app is a separate project built on a Mac and is not specified here. `sync-format.md` is the contract it will implement.
* Not carried by this spec because they belong to the iPhone project: Swift and TestFlight distribution, background audio recording, the phone-call audio limit, FluidAudio, and the on-device battery and heat thresholds (open question 3 is split accordingly).
* Requirements this spec added on its own judgment, not from intent.md or your brief: F-ID-4, F-RET-11, F-RET-13, F-RET-14, F-CON-6, F-MOD-6, F-MOD-7, F-MOD-8, the retrieval design in F-QA-3, F-QA-5, F-QA-7, F-HK-1 to F-HK-9, the numbers in NF-3, and the safety rules in `sync-format.md` sections 10 to 14. Each can be cut at the gate.
* Addendum to intent.md (accepted, and appended to intent.md on 2026-11-01 without changing its original text): add "Desktop identity, design and model-choice screen" to Proposed outcome, add "iPhone implementation (separate repository and spec)" to Out of scope, and add open question 7 "Files as the source of truth or SQLite as the source of truth" (decision D1 below).

## 1. Current state of the fork (answers open question 1)

Inspection of branch `feature/speakers-folders-people` at commit `818ef52`.

| Area | Today |
|---|---|
| Notes, transcripts, folders, people | One SQLite database, `meeting_minutes.sqlite`, in the app data folder (`%APPDATA%\com.meetily.ai\`). Not files. |
| Tables in use | `meetings` (id `meeting-<uuid>`, title, timestamps, `folder_path` = on-disk recording folder, `folder_id`), `transcripts` (one row per line: text, timestamp, `audio_start_time`, `audio_end_time`, `duration`, `speaker` = slot key), `summary_processes` (status plus a JSON `result` whose `markdown` field is the summary text), `transcript_chunks`, `settings` and `transcript_settings` (provider, model, and API keys in plaintext columns), `people` (with notes), `meeting_speakers` (slot, person link, local voiceprint), `folders`. |
| Folders | Flat. Unique by name ignoring case. `meetings.folder_id` points at one; deleting a folder returns its meetings to unfiled. |
| Unused or dead | `meeting_notes` table (created, never referenced by Rust code). `licensing` table and `api_get_profile`, `api_save_profile`, `api_update_profile` commands carrying a `license_key` (upstream Pro scaffolding). `lib_old_complex.rs`, `core-old.rs`, `recording_saver_old.rs` and `recording_commands.rs.backup` (old files). The profile commands call a local app server at `http://localhost:5167` (`APP_SERVER_URL`), which belongs to the archived backend. |
| Audio | Files, not database. Each meeting has a folder under the recordings folder (default `%USERPROFILE%\Music\meetily-recordings\<name>\`) holding the audio file, `metadata.json`, `transcripts.json` and incremental checkpoints. `meetings.folder_path` points to it. `auto_save` is one global preference (already a switch in Settings), not a per-recording choice. When it is false the recorder discards audio chunks and still writes the transcript and metadata files, but `stop_and_save` then returns early: `metadata.json` stays at status `recording` with no duration and no saved event is sent. The recorder and the importer always write to the default recordings folder. The saved folder preference is not used, and there is no folder picker. |
| Crash recovery | A WebView IndexedDB store (`MeetilyRecoveryDB`) holds live transcript lines until the meeting is saved to SQLite. |
| Transcription pipeline | Mic and system audio go to the audio pipeline (`audio/pipeline.rs`): a recording path (professional mixing, ducking) and a transcription path (voice activity detection, then Whisper via whisper-rs or Parakeet via ort). Defaults: `large-v3-turbo` and `parakeet-tdt-0.6b-v3-int8`. Speaker detection runs after the fact on the saved audio (WeSpeaker ONNX, clustering) and keeps voiceprints locally. |
| Summary pipeline | Transcript is chunked (default 40,000 characters), each chunk summarised, then combined with a template into markdown, optionally in a chosen language. Providers: OpenAI, Claude, Groq, Ollama, OpenRouter, CustomOpenAI, and BuiltInAI (a bundled `llama-helper` sidecar running GGUF models). |
| Model selection | Transcription provider and model are in `transcript_settings`, summary provider and model in `settings`. Built-in summary models: Qwen 3.5 2B (1,221 MB), Qwen 3.5 4B (2,614 MB), Gemma 3 4B (2,374 MB), Gemma 3 1B (1,019 MB), all with a 32,768 token context. The transcription settings screen and database already have slots for Deepgram, ElevenLabs, Groq and OpenAI transcription. I have not verified that these work end to end. |
| Network posture | The app opens no listening sockets (only tests bind to localhost). Telemetry code (PostHog) exists but the key is empty in this fork, so nothing is sent. The updater config points at this fork's releases but carries the upstream project's signing public key. The updater plugin is registered in `lib.rs` and used by the update dialog, About and the tray. |
| Consent notice | `ComplianceNotification.tsx`: a small popup saying "Recording Notice", "Inform participants about recording." and "US compliance required", with Later and Done buttons. No "never show again". |
| Branding in code | Name `meetily` in `tauri.conf.json` (productName, window title), `Cargo.toml`, `package.json`; identifier `com.meetily.ai`; 86 case-insensitive mentions in `src` and `src-tauri/src`; default folder `meetily-recordings`; temp prefix `.meetily_decode_`; IndexedDB name `MeetilyRecoveryDB`. |
| Styling | Tailwind with shadcn variables in `globals.css` (grey scale). 1,283 uses of raw palette classes (`bg-gray-100`, `text-blue-600` and so on) in 81 `.tsx` files, plus 5 hard-coded hex colours. Body font Source Sans 3 loaded through `next/font/google`. |
| Em dashes | 119 in `.ts`, `.tsx` and `.rs` source (29 and 90). None in any other tracked text file, including `LICENSE.md`, `docs/`, `README.md` and `CLAUDE.md` (checked with `git grep`). |
| Repo guidance | `CLAUDE.md` is upstream's 409-line document. It describes a legacy backend and is far over the one-page guidance. |

Answer to open question 4: yes. The desktop needs (a) a vault module that reads and writes the shared format, (b) new meeting columns for retention and mode, (c) a retention sweep, (d) a Sync settings screen, and (e) a Retention setting. Windows needs the same retention setting as every other platform.

## 2. Requirements

Priority: M = must for this spec, S = should (deferrable at the gate), C = could.

### 2.1 Identity (F-ID)
1. **F-ID-1 (M)** The app name, slug, bundle identifier, window title, installer name, default recordings folder name, vault folder name and About text come from one file, `brand/brand.json`. Changing the name is a one-line edit plus one command.
2. **F-ID-2 (M)** A generator, `pnpm brand:apply`, writes the brand values into `tauri.conf.json` and a generated frontend module. `pnpm brand:check` fails if the generated files drift from `brand.json`.
3. **F-ID-3 (M)** No user-visible string hard-codes the app name. They read it from the brand module.
4. **F-ID-4 (M)** On first launch under a new identifier, if the old data folder (`com.meetily.ai`) exists and the new one does not, the app offers to copy the database, settings and downloaded models into the new folder. It copies, never moves, and leaves the old folder untouched. The offer runs before any app state exists. The database is read through a consistent snapshot (`VACUUM INTO`), model files are copied so the two apps never share a file, and the stores that record onboarding and the recordings folder are copied, so the new app does not repeat onboarding or re-download the models. The hard-coded `Meetily/templates` and `meetily/notifications.json` locations move into the new data folder. Legacy names stay recognised: the `.meetily_decode_` temp prefix in the deletion allowlist. The WebView profile (IndexedDB `MeetilyRecoveryDB`, local storage flags) also lives under the identifier and is not copied, so unsaved-meeting recovery data in the old app does not carry over. The adoption screen says so and asks you to open the old app and finish or recover any unsaved meeting first (to be verified on Windows).
5. **F-ID-5 (M)** An About screen shows the name, version, "Based on the upstream project, MIT licence", a link to the full MIT text, and third-party notices. The upstream marketing block and the analytics switch are removed from it.
6. **F-ID-6 (M)** The upstream MIT copyright and permission notice stays in `LICENSE.md`. A second copyright line for your modifications may be added (your choice of holder name).
7. **F-ID-7 (M)** No the upstream app logo, icon or name is used as branding. All icons (app, tray, installer images) are replaced by the original icon (section 3.2).
8. **F-ID-8 (M)** No code, text or assets from the upstream app's paid Pro tier. The Pro scaffolding found in section 1 is removed (F-HK-1).
9. **F-ID-9 (M)** The remaining allowed mentions of the upstream name are listed in `brand/allowed-mentions.txt`: `LICENSE.md`, the About credit, third-party notices, `legacy.rs` and `legacy.ts`, documents that describe upstream, applied migrations under `migrations/` (they must not be edited), and the crate and package names in `Cargo.toml` and `package.json`. A lint fails on any other mention.

### 2.2 Visual design (F-UI)
1. **F-UI-1 (M)** Colours, type, radii and spacing are design tokens in one file, `frontend/src/styles/tokens.css`. Tailwind and the shadcn variables read from it. No component sets a raw colour.
2. **F-UI-2 (M)** Light and dark themes. Default follows the operating system. The user can force light or dark in Settings. The forced `"theme": "Light"` window setting in `tauri.conf.json` is removed so the window can follow the system.
3. **F-UI-3 (M)** Every text and control colour pair meets WCAG 2.2 AA: 4.5:1 for normal text, 3:1 for large text, icons and control borders. Each token pair carries a role in `tokens.contrast.json` (text, large text or icon or border, decorative, or exempt such as disabled). A script computes the ratios from `tokens.css` and fails the build when a pair is below its role's threshold. Hover, pressed, selected, placeholder and disabled states have their own tokens.
4. **F-UI-4 (M)** Serif for the wordmark and headings, sans for body text. Both are open-licence fonts shipped through pinned fontsource packages, so the app and its build need no network after the packages are installed.
5. **F-UI-5 (M)** Generous spacing on an 8 px grid, soft corners (10 px cards, 8 px controls), no drop shadows except a visible focus ring.
6. **F-UI-6 (M)** Few controls: each main screen has one primary action, and secondary actions sit in an overflow menu or settings.
7. **F-UI-7 (M)** The 1,283 raw palette class uses are replaced with semantic token classes by a codemod, screen by screen. A lint rule then forbids raw palette classes and hex colours outside `tokens.css`.
8. **F-UI-8 (M)** Reduced-motion and keyboard use work: visible focus on every control, no interaction needs a pointer, animations stop when the system asks.
9. **F-UI-9 (S)** An original icon concept (section 3.2) is finalised as an SVG master, and all raster sizes and the `.ico` are generated from it by a script.
10. **F-UI-10 (C)** An optional soft landscape illustration at the foot of the sidebar, in a light and a dark version. Each is an SVG or an optimised WebP of at most 60 KB, contains no text, is decorative (empty alt text), and ships inside the app so no network is needed.

### 2.3 Audio retention (F-RET)
1. **F-RET-1 (M)** A Retention setting with six values: Off, 24 hours, 7 days, 14 days, 30 days, Keep. Plain-language description under each. The default for new installs is 7 days (decision D7).
2. **F-RET-2 (M)** The app saves the policy on the meeting: `audio_retention`, `audio_expires_at` (UTC, null for Keep), `audio_state` (`none`, `present`, `deleted`), `audio_deleted_at`, `finalized_at` and `mode` (`audio` or `transcript_only`). For 24 hours to 30 days, `audio_expires_at` is the recording end plus the period. For Off it is set when finalization completes (F-RET-3). A meeting is never swept before `finalized_at` is set. The same fields are assigned when a crashed recording is recovered and when audio is imported, using the time of recovery or import as the start. Recording folders on disk with no matching meeting row are listed in Settings under Storage as unmanaged. They are never deleted automatically.
3. **F-RET-3 (M)** Off means: delete the audio as soon as the transcript is safely saved. "Safely saved" means the transcript rows are committed to SQLite, the vault copy is written if sync is on, and, if the detect-on-save setting is on (F-RET-14), speaker detection has finished. If any of these fails, the audio is kept and the user is told. Audio is never deleted before the save is confirmed. The deadline for Off is set when this finalization completes, so the hourly sweep cannot race it.
4. **F-RET-4 (M)** A cleanup sweep runs at app launch (after the database is ready), every 60 minutes while the app runs, and when the window regains focus after being hidden. It compares `audio_expires_at` with the current UTC time, so a sleeping computer catches up on wake.
5. **F-RET-5 (M)** Deleting audio removes every audio artefact for the meeting: the final audio file, checkpoints, temporary decode files, and any vault copy `_audio/<id>.*`, whether or not audio sync is turned on on this computer (a copy uploaded by the phone is removed too).
6. **F-RET-6 (M)** Deletion is verified. After removing a file the sweep checks that it no longer exists. Only then is `audio_state` set to `deleted`. A locked or failed deletion stays `present`, is retried on the next sweep, shows as "deletion pending" in the meeting, and is logged without any transcript text. Windows file locks from antivirus scanners, the audio player or a sync client are expected, so retry is the normal path, not an error.
7. **F-RET-7 (M)** Safety: deletion only ever touches files inside a meeting folder that carries the app's marker files, only known audio and temp extensions, never follows symbolic links or reparse points, and never leaves the recordings root. The existing `delete_recording_files` cannot be reused for this, because it also removes `transcripts.json`, `metadata.json` and the folder, and stops at the first error. A new audio-only function reuses the allowlist helpers in `api/recording_files.rs`, deletes file by file, and returns a result per file.
8. **F-RET-8 (M)** Changing the setting affects future recordings. "Apply to existing recordings" is a separate button that shows how many recordings and how much space it affects and asks for confirmation. A single meeting can be set to "Keep this one" or "Delete audio now" from its menu. Extending is possible only before expiry.
9. **F-RET-9 (M)** Every meeting shows its audio status in words: "Audio kept", "Audio deleted on 8 Nov", "No audio saved".
10. **F-RET-10 (M)** The settings text says plainly what deletion cannot do: it removes the file from this computer and from the vault folder, but cloud recovery areas such as iCloud's 30-day "Recently Deleted", backups and low-level disk recovery are outside the app's control. It also says the sweep only runs while the app runs.
11. **F-RET-11 (S)** An opt-in Windows Scheduled Task runs `quire.exe --retention-sweep` once a day with no window, so deadlines are enforced even if the app is not opened. The setting explains what it installs and has a one-click remove.
12. **F-RET-12 (M)** Deleting a meeting never leaves audio behind. Today the database row is removed first and a failed file deletion orphans the folder with no retry. Instead the meeting is marked as deleting, its audio is removed with per-file results, and the row is removed only when the files are gone. A locked file keeps the meeting listed as "deleting" and is retried by the sweep. The vault is updated (tombstone written, `_audio` copy removed).
13. **F-RET-13 (M)** Mass-expiry guard: a sweep that would delete audio for more than 20 recordings, or more than a quarter of all recordings, stops and asks for confirmation. This limits the damage of a wrong system clock or a hostile vault. A clock earlier than the last recorded app run pauses deletion with a notice.
14. **F-RET-14 (S)** A setting "Detect speakers when a recording ends", off by default (the speaker model must be downloaded first). Today detection is started by hand, so without this setting Off would delete the audio before detection could ever run. When on, the finalizer runs detection (F-RET-3). When off, the retention setting says: "With Off, speakers cannot be detected afterwards."

### 2.4 Transcript-only mode (F-TO)
1. **F-TO-1 (M)** A "Transcript only: do not save audio" switch on the record screen, remembered between sessions.
2. **F-TO-2 (M)** In this mode no audio file, checkpoint or temporary audio file is created at any point during or after the recording. Audio is processed in memory for transcription and then discarded. The meeting is saved with `mode: transcript_only` and `audio_state: none`. Transcript-only is a per-recording choice. Today `auto_save` is one global preference and its disabled path never finalizes the meeting (metadata stays `recording`, no duration, no saved event), so the work includes a finalization path that completes the metadata and sends the saved event.
3. **F-TO-3 (M)** Features that need audio are disabled with a reason: detecting speakers after the fact, Enhance or retranscription, and audio playback. Manual speaker labels, renaming, folders, people and summaries all still work.
4. **F-TO-4 (M)** Wording, in the app: "Transcript only saves no audio file. Whether you need people's consent depends on where you and they are, and it does not change with this setting." No claim that it avoids legal risk, and no statement that a duty does or does not exist.
5. **F-TO-5 (M)** Transcript-only is not identical to retention Off (decision D4). Off briefly stores audio so speaker detection and voice memory can run before deletion. Transcript-only never stores it. The UI shows them as two clear options and explains the difference in one sentence.

### 2.5 Live Q&A (F-QA)
1. **F-QA-1 (M)** An "Ask" box, collapsed by default, on the live recording screen and on saved meetings. A question returns an answer based on the transcript so far (live) or the whole transcript (saved).
2. **F-QA-2 (M)** The default model is local (BuiltInAI, or Ollama on this computer). With local models no network request is made. A cloud provider is used only when the cloud switch (F-MOD-5) is on and that provider is selected, and the Ask box then shows "Sends the transcript to <provider>".
3. **F-QA-3 (M)** The prompt contains the question, the most recent part of the transcript that fits a recent-window budget, and the best keyword matches (BM25) from earlier in the transcript that fit a retrieval budget. Both budgets are parameters in config. Starting values are 8,000 and 3,000 tokens, small because the installer build is CPU-only, and they are tuned after the benchmark (AC-QA-3). The answer says which span it used when the transcript is longer than the window ("Answered from the last 25 minutes and 4 earlier passages").
4. **F-QA-4 (M)** Answers cite timestamps. The app supplies the segments with their times, and keeps only citations that fall inside a supplied segment. Clicking a citation scrolls to that line.
5. **F-QA-5 (M)** A recording is never interrupted by Q&A. Q&A is cancellable, runs at lower priority than transcription, and is refused with a clear message if the transcription backlog grows past a limit set after benchmarking (section 4, AC-QA-3).
6. **F-QA-6 (M)** Question and answer history is kept in memory for the session only. "Add to note" appends a labelled, AI-generated Q and A to the note body.
7. **F-QA-7 (M)** The model has no tools and cannot take actions. Transcript text is treated as untrusted content. Answers are rendered through the sanitizing markdown path and labelled as AI generated.
8. **F-QA-8 (M)** With no model installed, the box says "Download a model in Settings" and links to it.

### 2.6 Consent reminder (F-CON)
1. **F-CON-1 (M)** When a recording starts, a small non-blocking reminder appears near the record controls. It never blocks or delays the start.
2. **F-CON-2 (M)** It has a primary "Got it", a "Never show this again" checkbox, and a "Read more" link. Closing it without choosing just dismisses it for this recording.
3. **F-CON-3 (M)** "Never show this again" is stored locally and can be reversed in Settings under Privacy.
4. **F-CON-4 (M)** "Read more" opens an in-app page with the plain-language guidance and copy-ready wording (section 3.5). It also links to an external state-by-state reference, through the existing external-link allowlist.
5. **F-CON-5 (M)** The old text "US compliance required" is removed. No sentence in the app says the user is or is not compliant with any law.
6. **F-CON-6 (M)** A one-time first-run screen says: "For personal use. Do not record anything you are not allowed to record or keep on a personal computer." It asks for one tap to continue and stays readable in Settings under Privacy. This is a reminder, not enforcement.

### 2.7 Model choice (F-MOD)
1. **F-MOD-1 (M)** A Models screen with two parts: Transcription and Summaries and Q&A. Each model row shows name, download size, languages, a source link, and a status (installed, download, remove). Parakeet is listed first, then Whisper models.
2. **F-MOD-2 (M)** Language guidance: Parakeet v3 covers 25 European languages. When the chosen transcription language is outside that list, the screen recommends a Whisper model. The language list lives in a config file, not in code paths.
3. **F-MOD-3 (S)** API transcription (Deepgram, ElevenLabs, Groq, OpenAI slots already exist in the settings screen) is verified end to end. Each provider is labelled "Sends audio to <provider>" and is unavailable until the cloud switch is on. If verification shows a provider does not work, that provider is hidden rather than repaired (decision D8).
4. **F-MOD-4 (M)** Summaries and Q&A: local models first (the four built-in models with sizes, and Ollama), then cloud providers with a user-supplied key (OpenAI, Claude, Groq, OpenRouter, CustomOpenAI).
5. **F-MOD-5 (M)** A master "Allow sending data to cloud services" switch, off by default. Turning it on shows one confirmation that names what is sent (audio for transcription, transcript text for summaries and Q&A). Cloud providers are not selectable while it is off. Upgrade rule: if an existing install already has a cloud provider selected with a key, the switch starts on, a one-time notice lists what is enabled, and one click turns it off. Otherwise it starts off.
6. **F-MOD-6 (M)** API keys move from plaintext database columns to Windows Credential Manager. All nine API key columns in `settings` and `transcript_settings` (Anthropic, Deepgram, ElevenLabs, Gemini, Groq, Ollama, OpenRouter, OpenAI, Whisper API) and the key inside the CustomOpenAI config JSON are migrated on first launch and then cleared from the database. If the credential store is unavailable the app says so and does not fall back to plaintext.
7. **F-MOD-7 (M)** The Ollama and CustomOpenAI endpoints default to localhost. Saving a non-local host asks for confirmation, because the transcript then crosses the network.
8. **F-MOD-8 (S)** Model downloads verify a SHA-256 from an in-repository manifest (`models.json`), as the speaker model and ffmpeg already do. A mismatch deletes the file and refuses to load it.

### 2.8 Shared vault and sync (F-SYNC)
1. **F-SYNC-1 (M)** A Sync screen. Nothing is written anywhere until the user chooses a vault folder and confirms. On Windows the screen offers the iCloud Drive folder if it exists (`%USERPROFILE%\iCloudDrive`).
2. **F-SYNC-2 (M)** First enabling shows a plain disclosure: files in the vault are uploaded to iCloud by Apple's software, not by this app, and without Apple's Advanced Data Protection Apple holds the encryption keys. The user can cancel.
3. **F-SYNC-3 (M)** Once on, transcripts, notes, people and folders sync by default. Audio sync is a separate switch, off by default, with the limits in `sync-format.md` section 10 stated beside it. Audio is never uploaded for recordings with retention Off or in transcript-only mode, even when audio sync is on.
4. **F-SYNC-4 (M)** Phase E: one-way export (database to vault) with the verification step in `sync-format.md` section 16. The database is not modified except for the new `sync_id` columns.
5. **F-SYNC-5 (M)** Phase F: two-way sync with the conflict, deletion and versioning rules in `sync-format.md` sections 11 to 13. Phase F does not start until the unverified items in `sync-format.md` section 15 are tested on the real PC and iPhone.
6. **F-SYNC-6 (M)** A Conflicts count and viewer. A conflict never discards data (the losing copy lands in `_conflicts`).
7. **F-SYNC-7 (M)** Files from the vault are untrusted input. The rules in `sync-format.md` section 14 are implemented and tested. Vault content reaches the interface only through Tauri commands that return validated data, and every markdown renderer in the app (the summary editor and the viewer) is tested with hostile bodies.
8. **F-SYNC-8 (M)** Retention is enforced on synced audio (F-RET-5) and by deadline on every device (`sync-format.md` section 10).
9. **F-SYNC-9 (M)** Turning sync off stops watching and writing and leaves the files in place. A separate, confirmed action removes the app's files from the vault.
10. **F-SYNC-10 (M)** Voiceprints, keys, models and settings never enter the vault.

### 2.9 Housekeeping (F-HK)
1. **F-HK-1 (M)** Remove Pro scaffolding: the `licensing` table (new migration that drops it), the profile commands and `license_key` structures, and the profile calls to the local app server on port 5167. Applied migrations are not edited. The two backend-connection test commands, the `localhost:5167` CSP entry and the sidebar's server address, which only served the archived backend, go too.
2. **F-HK-2 (M)** Remove the telemetry code, its dependency and its settings toggles, so that "nothing is sent" is provable from the dependency list.
3. **F-HK-3 (M)** Remove files confirmed unreferenced (`lib_old_complex.rs`, `core-old.rs`, `recording_saver_old.rs`, `recording_commands.rs.backup`, `audio/stt.rs`, `audio/system_audio_stream.rs`, `audio/system_audio_types.ts`, `whisper_engine/_stderr_suppressor.rs`, `audio_v2/`, `CLEANUP_PLAN.md`, `build_backup.bat`, `app/metadata.ts` and `.tsx`, the empty `AudioPlayer.tsx`), each only after the compiler and typechecker confirm nothing uses it. The unused `meeting_notes` table is dropped in the same new migration as `licensing`.
4. **F-HK-4 (M)** Disable the updater until the fork has its own signing key. The upstream public key in `tauri.conf.json` means updates from this fork could not verify anyway. This touches the plugin registration in `lib.rs`, the update dialog, About and the tray.
5. **F-HK-5 (M)** Remove the 119 em dashes from source (code comments and a few user-visible strings) and add a lint that fails on U+2014 in any tracked text file. No exemptions are needed today, because no other tracked file has one.
6. **F-HK-6 (M)** Replace `CLAUDE.md` with a one-page version: build, test and lint commands, conventions, folder map, things Claude gets wrong, and a verify-before-done block. Move upstream's long document to `docs/UPSTREAM_NOTES.md` (not `docs/ARCHITECTURE.md`, which would collide with the existing `docs/architecture.md` on case-insensitive Windows).
7. **F-HK-7 (S)** Make the recordings folder a real setting: the recorder and the importer honour the saved preference (today they ignore it) and Settings gets a folder picker. New installs default to the app data folder (`recordings`). The picker warns when the chosen folder is inside OneDrive, iCloud Drive or Dropbox. Existing installs keep recording to `Music\meetily-recordings` until they choose otherwise (D3a).
8. **F-HK-8 (S)** Verify the licence of the bundled Windows ffmpeg build (section 7, concern 17).
9. **F-HK-9 (M)** Before the first run of a newer migration set, the app copies `meeting_minutes.sqlite` to a dated backup next to it, and keeps the last three backups. Migrations are one way, and a backup is the only downgrade path.

### 2.11 My notes (F-NOTE)
Added after the A3 review on 2026-11-01, at the owner's request (modelled on how Granola combines your notes with the transcript).
1. **F-NOTE-1 (M)** A "My notes" pane in the meeting view beside the summary, on the live recording screen and on saved meetings. It is a plain rich-text editor that saves on its own; there is no Save button.
2. **F-NOTE-2 (M)** Notes are stored on this computer in SQLite with their meeting, are included in the database backup (F-HK-9), and are deleted with the meeting. Notes typed during a recording are saved within 2 seconds of the last keystroke and survive a crash: they are recovered with the meeting.
3. **F-NOTE-3 (M)** When notes exist, summary generation uses them as the person's own notes: their points come first and set the outline, the transcript fills in detail, their wording is kept for their points, and a disagreement between the notes and the transcript is flagged rather than silently resolved. With no notes the summary is generated exactly as before.
4. **F-NOTE-4 (M)** Generating or regenerating a summary never changes the notes. Notes and summary stay separate documents.
5. **F-NOTE-5 (M)** Ask (F-QA) reads the notes as well as the transcript. Note text is labelled as the person's notes in the model context, and an answer that relies on it says "your notes" instead of a timestamp.
6. **F-NOTE-6 (S)** In the summary, points that came from the notes are visually distinct from points the model added.
7. **F-NOTE-7 (M)** Notes follow the same privacy rules as the transcript: they leave the computer only when a cloud provider is chosen for summary or Ask, and the same disclosure applies. Vault export of notes is part of Phase E.

### 2.12 Notes-first meetings (F-NM)
Added on 2026-11-02 at the owner's request: a meeting starts when you open it, not when you record.
1. **F-NM-1 (M)** "New meeting" in the sidebar and on the home screen creates a meeting at once and opens it with My notes focused, ready to type. No recording starts and no microphone is opened.
2. **F-NM-2 (M)** The new meeting is a normal meeting: it appears in the list, can be renamed, filed in a folder, searched, deleted, and Ask works on its notes alone.
3. **F-NM-3 (M)** The meeting has a Record button. Recording from it adds the audio and transcript to that same meeting; it never creates a second meeting. Notes typed before, during and after the recording all stay in the one meeting.
4. **F-NM-4 (M)** One recording per meeting in this phase. After a recording is saved the Record button is replaced by the recording's details.
5. **F-NM-5 (M)** A new meeting left with no notes, no recording and its default title is removed when you leave it, so the list does not fill with empty meetings. Only the database row is removed; no file is touched.
6. **F-NM-6 (M)** The existing Record button keeps working as today (record first, take notes during), as a shortcut that creates the meeting and starts recording in one click.
7. **F-NM-7 (S)** A meeting with notes and no transcript can be summarised from the notes alone ("Tidy my notes").

### 2.13 Markdown export for Obsidian (F-MD)
Status: accepted by the owner on 2026-11-02 ("Accept as recommended"). Requested on 2026-11-02 so an Obsidian vault can show and organise meetings. It is one way and separate from the shared vault (`sync-format.md`), which it does not change: that format names files by ID and keeps the transcript in a JSON sidecar, so Obsidian cannot show it well.
1. **F-MD-1 (M)** A Markdown export setting. Nothing is written until the person chooses a folder and confirms. Whether a folder is synced (iCloud Drive, OneDrive, Dropbox, Obsidian Sync) cannot always be detected, so the confirmation always says that files in a synced folder leave this computer through that service, not through this app.
2. **F-MD-2 (M)** Each meeting becomes one readable `.md` file named from its date and title (`2026-11-02 1405 Weekly planning.md`). Body order: My notes, Summary, Transcript (one line per segment, `**Speaker** [mm:ss] text`). Sections with no content are left out.
3. **F-MD-3 (M)** Front matter is Obsidian properties: `title`, `date`, `duration`, `folder`, `speakers`, `tags` (`meeting`) and `quire_id`. It is written by the strict writer from `sync-format.md` section 4, so values are always quoted and safe.
4. **F-MD-4 (M)** Quire folders become subfolders of the export folder; unfiled meetings go to its root. Moving a meeting or renaming its title or folder moves or renames its file.
5. **F-MD-5 (M)** The export runs after a meeting is saved, after a summary is generated, and 5 seconds after notes stop changing. "Export all now" re-exports every meeting.
6. **F-MD-6 (M)** The app never overwrites or deletes a file that was edited outside Quire. It keeps the hash of what it last wrote; a file whose bytes differ is left alone and listed as "edited in another app", with a per-file "Replace with Quire's version". Deleting a meeting deletes its file only when it is unchanged.
7. **F-MD-7 (M)** Safety: file and folder names are cleaned with the rules in `sync-format.md` section 15 (reserved names, dots, length); every path is derived from cleaned names and stays inside the chosen folder; symbolic links are not followed; writes are atomic (temporary file, then rename). Name clashes get ` (2)`, ` (3)`.
8. **F-MD-8 (M)** Never exported: audio, voiceprints, keys, settings, Ask history. Remote content is not made fetchable: image embeds and raw HTML in the summary and transcript are written as plain text, so Obsidian makes no network request when it opens a file.
9. **F-MD-9 (S)** Exported files are never read back. Edits in Obsidian do not change Quire. Two-way editing waits for Phase F.

### 2.10 Non-functional (NF)
1. **NF-1 Offline by default.** With sync off and the cloud switch off, the only network traffic is a model download the user starts. A network inventory test lists every outbound call site with the setting that enables it, and fails when a call site is added without an entry.
2. **NF-2 Accessibility.** WCAG 2.2 AA for contrast and keyboard use (F-UI-3, F-UI-8). Touch and click targets at least 24 by 24 px.
3. **NF-3 Performance.** A sweep over 1,000 meetings takes under 2 s when nothing is due. The consent reminder adds under 100 ms to starting a recording. Opening the app is not more than 10 percent slower than at commit `818ef52`. The one-way export of 500 meetings finishes in under 60 s on the target PC.
4. **NF-4 Reliability.** A crash or power loss during a sweep or an export leaves no half-written note or transcript (atomic writes) and no audio marked `deleted` that still exists.
5. **NF-5 Portability.** Windows 11 first. No Windows-only code outside `cfg(windows)` modules, and paths come from Tauri's path APIs, so a Mac build stays possible.
6. **NF-6 Privacy in logs.** Logs at every level contain no transcript text, question text, names, or API keys. A test searches captured logs after a scripted session.

## 3. Design

### 3.1 Brand configuration and rename
* `brand/brand.json` holds: `name`, `slug`, `bundleId`, `windowTitle`, `installerName`, `recordingsDirName`, `vaultDirName`, `tagline`, `upstreamCredit`. `bundleId` is `com.openedtools.notes`. It is deliberately not derived from the name, because the identifier names the data folder, so a later rename never moves user data. In CSS and Tailwind the `muted` text token is named `ink-muted`, because shadcn already uses `muted` for a background.
* `scripts/brand-apply.mjs` reads it and writes `productName`, `mainBinaryName`, `identifier` and window titles into `tauri.conf.json`, and `frontend/src/lib/brand.generated.ts` for the UI. `--check` compares without writing. Crate and package names stay as they are, because renaming the Rust library breaks `RUST_LOG` filters and build scripts for no user benefit.
* Rust code that needs the name reads it from the generated module at compile time, not from string literals.
* The vault's format identifiers (`quire-vault`, `quire-format.json`, the `quire` front matter key) are deliberately not brand values and do not change on a rename (see `sync-format.md`).
* Data adoption (F-ID-4): the data folder name follows the Tauri identifier. On first launch under the new identifier the app finds the old folder, shows its size, and asks to copy it. The copy runs in a temporary folder and is moved into place only when complete, so an interrupted copy cannot leave a half database.
* Legacy constants stay in a single `legacy.rs` and `legacy.ts` with a comment saying why, and appear in `brand/allowed-mentions.txt`.

### 3.2 Design tokens and look

Corrected token values. The two values you suggested for muted text and accent text do not reach AA for small text (muted 3.64 on the background, accent 3.77). I kept your accent as a decorative colour and added a stronger terracotta for text and button fills. All ratios were computed.

Light theme

| Token | Value | Use | Contrast |
|---|---|---|---|
| `paper` | `#FAF6EE` | page background | n/a |
| `surface` | `#F2EBDD` | cards, sidebar, inputs | n/a |
| `line` | `#E4DACA` | decorative dividers | decorative only (1.28 on paper) |
| `line-strong` | `#897D6D` | borders of inputs and controls | 3.73 on paper, 3.39 on surface |
| `ink` | `#2B2622` | body text | 13.89 on paper, 12.62 on surface |
| `muted` | `#6B6155` (was `#8A7F72`) | secondary text | 5.62 on paper, 5.11 on surface |
| `accent` | `#C4623F` | icon, large shapes, large headings, focus accents | 3.77 on paper (passes 3:1 only) |
| `accent-strong` | `#A94E2B` | links, small accent text, primary button fill | 5.11 on paper, 4.64 on surface |
| `on-accent` | `#FAF6EE` | label on a primary button | 5.11 on `accent-strong` |
| `danger` | `#9E2B1F` | errors | 6.91 paper, 6.28 surface |
| `success` | `#3F6B3A` | confirmations | 5.77 paper, 5.24 surface |
| `warning` | `#8A5A00` | cautions | 5.50 paper, 5.00 surface |
| `sage` | `#6B6C54` | "on this device and private" indicators, secondary buttons | 5.00 on paper, 4.54 on surface; `on-accent` label 5.00 |
| `sage-tint` | `#E5E8D8` | chip and badge background | `ink` on it 12.02 |

Dark theme (warm charcoal, no pure black)

| Token | Value | Contrast |
|---|---|---|
| `paper` | `#1F1B17` | n/a |
| `surface` | `#2A2520` | n/a |
| `line` | `#3A332B` | decorative |
| `line-strong` | `#857A6B` | 4.07 on paper, 3.61 on surface |
| `ink` | `#EFE8DC` | 14.05 on paper, 12.46 on surface |
| `muted` | `#A89D8E` | 6.42 on paper, 5.69 on surface |
| `accent` and `accent-strong` | `#E08A66` | 6.52 on paper, 5.78 on surface |
| `on-accent` | `#1F1B17` | 6.52 on `accent` |
| `danger` | `#F08A7A` | 7.02 paper, 6.23 surface |
| `success` | `#8FC285` | 8.33 paper, 7.39 surface |
| `warning` | `#E0B05A` | 8.58 paper, 7.61 surface |
| `sage` | `#A9B393` | 7.79 on paper, 6.91 on surface; dark label on it 7.79 |
| `sage-tint` | `#313528` | `ink` on it 10.32, `sage` on it 5.72 |

Values are starting points to adjust by eye. Any change must keep the contrast script green.

Roles checked by the script (`tokens.contrast.json`): `ink` and `muted` on `paper` and `surface` are text (4.5). `accent-strong` on `paper` and `surface` is text (4.5). `on-accent` on `accent-strong` is text (4.5). `danger`, `success` and `warning` on `paper` and `surface` are text (4.5). `line-strong` on `paper` and `surface` is control border (3.0). `accent` on `paper` (3.77) is icon or large text (3.0), and `accent` on `surface` (3.43) is icon or large text only. The light `on-accent` on `accent` (3.77) is large text only and is not used for small labels. `line` is decorative and not checked. Placeholder text uses `muted`. Disabled controls are exempt from contrast but use a distinct `disabled` token. Hover and pressed states darken `accent-strong` by one step and are checked as text pairs. `sage` on `paper` and `surface` is text (4.5), `on-accent` on `sage` is text, and `ink` on `sage-tint` is text. In the light theme `sage` text on `sage-tint` is 4.33 and is therefore not used.

Type: Source Serif 4 (open licence) for the wordmark and headings, Source Sans 3 (already the app's body font) for everything else, both bundled as local font files through `next/font/local`. Scale 14, 16, 20, 28, 40 px; body line height 1.55. Spacing on an 8 px grid. Cards and panels 10 px radius, controls 8 px. No shadows; separation comes from `surface` against `paper` and from `line`.

Shape of the work: tokens first, then the codemod from raw palette classes to semantic classes, one screen at a time (record, meeting, sidebar, people, settings, dialogs), each reviewed with before and after screenshots in light and dark. The shadcn variables in `globals.css` become aliases of the tokens, so existing shadcn components inherit the look without edits.

Icon concept: `intent/private-meeting-recorder/assets/icon-concept.svg`. A terracotta Q on a cream tile, with a pale second ring behind it that suggests stacked folded sheets (a quire is a gathered set of folded sheets). It reads as a Q at 16 px, is a single flat shape, and is original. It becomes the SVG master in `brand/`.

Aesthetic reference: the concept image (not kept in the repository), a concept image supplied by the owner. It is a mood and style guide only. It is not a layout, and it omits features this app has and shows some this spec does not require.
* Taken from it: warm peach-cream surfaces; serif headings and wordmark; soft rounded cards with no heavy shadows; speaker chips as quiet warm neutrals with one peach variant; a muted sage green as a second accent that means "on this device and private" (the On Device toggle, the leaf icon); a restyled audio player with a waveform; a calm, airy summary panel; and, optionally, the soft landscape illustration at the foot of the sidebar (F-UI-10).
* Not taken: its layout, the Share button, the folder names, the action item checkboxes, the name "AudioTranscribe", and the "Parakeet v2" label (this app uses v3).
* Measured from the image: the Record label on its button is 2.85:1 and the On Device label on sage is 4.06:1, both below AA. Secondary text is fine (5.6:1). The tokens above fix this: buttons use `accent-strong` with a cream label (5.11), and sage uses `sage` (label 5.00).

### 3.3 Retention and the sweep
* New module `retention` in the Rust core with a pure decision function `due(meeting, now) -> Action` and a thin runner that does file operations. The clock is injected, so tests can set "now".
* Migration adds the columns in F-RET-2 to `meetings`. Existing meetings get `audio_retention = 'keep'`, `audio_state = 'present'` when they have a folder with audio, so nothing existing is deleted by upgrading.
* Off mode: the recording is written as usual (checkpoints make crash recovery possible). When the meeting is saved, a finalizer runs: save transcript, run speaker detection if the detect-on-save setting is on (F-RET-14), write the vault copy if sync is on, set `finalized_at` and (for Off) `audio_expires_at`, then call the deletion routine. Failure at any step leaves the audio and shows a notice.
* Transcript-only mode: a per-recording flag starts the recorder without the incremental saver. The existing `auto_save = false` path discards audio chunks, but it is a global setting and never finalizes the meeting. The work is the per-recording flag, a finalization path that completes `metadata.json` (status, duration, saved event) and sets `mode`, the disabled-feature states, and tests that prove no audio file appears.
* Deletion routine (the new audio-only function from F-RET-7, not `delete_recording_files`): list candidate files in the allowlisted folder, remove each, check each is gone, update `audio_state` and `audio_deleted_at` in one transaction. Expiry does not rewrite the vault note (`sync-format.md` section 10). An explicit "Delete audio now" does. Locked files are retried on the next sweep.
* Sweep runner: tokio interval of 60 minutes plus launch plus window focus, guarded by a busy flag like the existing detection guard, so two sweeps never overlap.
* Optional scheduled task (F-RET-11): a `--retention-sweep` command line mode that opens the database, runs one sweep and exits without creating a window.

### 3.4 Live Q&A
* New Tauri command `api_ask_transcript(meeting_id, question)` and an event for streamed tokens. For a live meeting it reads the lines already committed to the live buffer; for a saved meeting it reads SQLite.
* Context builder (pure, tested): sort lines by time, take the newest lines up to the recent budget, run BM25 over the rest for the question, take top matches up to the retrieval budget, label every line with its speaker name and `mm:ss`.
* It reuses the existing LLM client and provider selection. The BuiltInAI sidecar handles one request at a time, so a Q&A request queues behind a running summary and says so.
* Citations: the model is asked to cite `[mm:ss]`. The app keeps only citations that fall inside a supplied segment's time range.
* Benchmark before the thresholds are fixed (section 4, AC-QA-3): measure first-token time and transcription backlog on your PC with Parakeet running and each built-in model answering over a 30, 60 and 120 minute transcript. The Windows installer build is CPU-only today (`docs/SPEAKERS_FOLDERS_PEOPLE.md`), so prompt processing is the main cost, and the context budgets in F-QA-3 start small for that reason.

### 3.5 Consent reminder copy

Draft text, no legal claims. Final wording is yours to edit.

Reminder (small card):
* Title: "Tell people you're recording"
* Body: "Rules about recording and transcribing conversations differ by place, and some can apply even when no audio is saved. A good habit: tell everyone, get a clear yes, and follow your organization's policy. This is general guidance, not legal advice."
* Actions: "Got it", checkbox "Never show this again", link "Read more".

Read more page:
* Why it matters, in three short points: laws differ between states and countries; some laws cover learning the contents of a conversation, not only recording it, and courts are still working out how they apply to AI notetakers; people on the call may be in a different place than you.
* What to do: say it out loud at the start, ask for a yes, put it in the invitation, stop if someone objects, follow your workplace rules.
* Copy-ready lines. Spoken: "I'm using a tool to transcribe this meeting so I can take notes. Is that okay with everyone?" Invitation: "I'll be using a local transcription tool during this meeting. Reply if you'd rather I didn't."
* One line on transcript-only: "Not saving audio keeps less on your computer. It does not change whether you need people's consent."
* Link to an external state-by-state reference. The Reporters Committee for Freedom of the Press guide is a candidate. The link and its wording are verified live during the build, not copied from memory.

### 3.6 Model choice screen
* A static `models.json` lists transcription models (Parakeet v3 int8, Whisper sizes) with size, languages, source URL and SHA-256 when known, and summary models. Source URLs open through the existing external-link allowlist.
* The cloud switch is one stored boolean read by a single helper, `cloud_allowed()`, which every cloud code path calls first. A test enumerates the provider enum and fails for any cloud variant that does not call it.
* Key storage uses the `keyring` crate on Windows (Credential Manager). Migration moves each key, verifies a read back, then nulls the database column.

### 3.7 Sync architecture on the desktop
* New `vault` module: `format` (parse and write, pure, fixture-tested), `export`, `import`, `state` (the `sync_state` table), `scan` (watcher plus periodic rescan), `conflicts`.
* Recommended option (D1): SQLite stays the working store, and the vault is the shared exchange. Phase E exports one way. Phase F imports through the same repository functions the interface uses, with conflict rules from `sync-format.md` section 11.
* Alternative: files as the only store, SQLite as a rebuildable index. Cleaner, but it rewrites most of the persistence layer, the 46 repository tests and the speaker, people and voice code. I do not recommend it for the first version.

### 3.8 Data flow summary
Recording audio goes to the pipeline. Mixed audio goes to the recorder (absent in transcript-only mode). Speech segments go to the transcription engine, then to the live buffer and the Ask box. On stop, the finalizer saves the transcript to SQLite, optionally detects speakers, exports to the vault, then the retention routine deletes audio according to policy. The sweep and the vault scanner run in the background. Nothing leaves the computer except through iCloud's own client (if sync is on) and the cloud providers the user explicitly enabled.

### 3.10 My notes
* Storage: a new migration adds a `meeting_user_notes` table (`meeting_id` primary key referencing `meetings`, `body_markdown`, `body_blocks` JSON for the editor, `updated_at`). The `meeting_notes` table dropped in WP4.5 was unused upstream scaffolding and is not revived.
* During a live recording the meeting row does not exist yet, so notes are written under the recording session id to a small recovery store and attached to the meeting when it is saved, using the same path as transcript recovery.
* Commands `api_get_user_notes(meeting_id)` and `api_save_user_notes(meeting_id, blocks, markdown)`, debounced in the UI (save 1 second after the last keystroke, and on blur and on navigation).
* Summary prompt: the notes go into the final report step as an `<attendee_notes>` block before the transcript chunks, with the F-NOTE-3 rules in the system prompt. Chunk summaries do not repeat the notes, to save context. Note text is treated as untrusted content in the same way as transcript text (F-QA-7).
* Ask context builder: the notes are always included (they are short), labelled `[your notes]`, ahead of the retrieved transcript lines.
* UI: the right-hand pane gets two tabs, My notes and Summary. During a recording My notes is the default; after a summary is generated the Summary tab opens. The Ask box sits under both.

### 3.9 Brand image prompts (for ChatGPT's image generator)

Use these to explore looks. Treat the output as concept art. The production icon is redrawn as a clean SVG from the chosen idea, because generated rasters are rarely clean at 16 px and cannot be recoloured exactly. Do not feed the upstream app's logo or any other app's logo to the generator as a reference, since the icon has to be original. Keep the prompt, tool, date and your choice in `brand/ASSETS.md`. Check the generator's current terms before an App Store release. For the wordmark, set the text in Source Serif 4 inside the app, since image generators misspell text.

Common style line to paste at the end of each prompt: "Flat vector style, limited palette of warm cream #FAF6EE, sand #E4DACA, warm charcoal #2B2622 and terracotta #C4623F, no gradients, no shadows, no gloss, no photographic textures, no text, no logos of existing apps."

1. App icon, main concept: "A minimal app icon on a rounded-square cream tile. A thick terracotta ring forming the letter Q with a short diagonal tail at lower right. A second ring in pale sand sits slightly offset behind it, suggesting stacked folded sheets of paper. Centered, generous padding, simple enough to recognise at 16 pixels. Square, 1024 by 1024."
2. App icon, alternative idea: "A minimal app icon on a cream rounded square showing three sheets of paper folded in half and nested together, seen from the edge, in terracotta, sand and charcoal, like the folded sheets of a handmade book. One clear silhouette, no detail finer than a thick line. Square, 1024 by 1024."
3. App icon, alternative idea: "A minimal app icon on a cream rounded square: a single terracotta page with one folded top-right corner and three short charcoal lines of text, slightly rotated, with a soft sand page behind it. One clear silhouette. Square, 1024 by 1024."
4. Dark-mode icon: "The same icon on a warm charcoal #1F1B17 rounded square, with the ring in lighter terracotta #E08A66 and the second ring in dark sand #3A332B. Flat, no glow. Square, 1024 by 1024."
5. Empty state illustration for a screen that says no meetings yet: "A quiet line illustration of an open notebook with a pencil resting on it, drawn with a single terracotta line and a few warm charcoal strokes on transparent or cream background. No people, no faces, calm and uncluttered, lots of empty space. Landscape 1200 by 800."
6. Installer side panel (NSIS, tall): "A tall narrow panel in warm cream with a very subtle paper grain, one large pale sand circle partly off the edge, and a small terracotta folded-corner shape near the bottom. Mostly empty space. Portrait, 328 by 628 pixels, to be scaled down to 164 by 314."
7. Release banner for the repository page: "A wide calm banner on warm cream: on the left a terracotta Q ring mark with a pale sand ring behind it, on the right generous empty space for a title. Landscape 1600 by 500."
8. Sidebar landscape, light (optional, F-UI-10): "A soft flat landscape of layered rolling dunes in terracotta, peach and sand tones with a small pale sun and faint contour lines. The top edge fades into warm cream #F2EBDD so it blends into a sidebar. No text, no people. 660 by 440 pixels."
9. Sidebar landscape, dark: "The same scene at dusk: warm charcoal #1F1B17 sky fading in from the top, dunes in muted terracotta and olive-grey, a small dim sun. No text. 660 by 440 pixels."

Sizes to request. Please generate at these sizes or larger, as PNG:

| Asset | Request | Used as |
|---|---|---|
| Icon ideas (prompts 1 to 3) | 1024 by 1024 | Concept only. I redraw the winner as an SVG and generate 16 to 1024 px and the `.ico` from it |
| Dark icon (prompt 4) | 1024 by 1024 | Concept only |
| Installer side panel (prompt 6) | 328 by 628 | Scaled to 164 by 314 and converted to BMP for the Windows installer |
| Installer header | 300 by 114 | Scaled to 150 by 57 and converted to BMP, same cream and a small Q mark, no text |
| Sidebar landscapes (prompts 8 and 9) | 660 by 440 | Optional F-UI-10, converted to optimised WebP |
| Empty state (prompt 5) | 1200 by 800 | Shown at about 360 by 240 |
| Repository banner (prompt 7) | 1600 by 500 | README and release page |

Is it wise? Yes for the sidebar, empty state, banner and installer panels, which are decorative. For the app icon use the output as inspiration only, because it must be a crisp vector at 16 px. Ask for no text in any image. Keep files small and ship them inside the app, not from a web address. Note the generator's current terms of use in `brand/ASSETS.md`.

## 4. Acceptance criteria

All checks are run in Stage 4 and their output is pasted into the pull request. Commands assume the repository root.

| ID | Check | Method | Pass condition |
|---|---|---|---|
| AC-ID-1 | Rename is one line | Change `name` in `brand.json` to `Marginalia`, run `pnpm brand:apply`, run `pnpm tauri build` | Window title, installer file name, productName and About text show the new name; `git diff --stat` touches only `brand.json` and the generated files |
| AC-ID-2 | No stray old name | `pnpm lint:brand` | Zero matches for `meetily` (case-insensitive) in product source, build scripts and configuration outside `brand/allowed-mentions.txt`. Each exception there has a written reason and the lint fails on a stale one. Documentation, CI workflows and the archived backend are not scanned |
| AC-ID-3 | Data adoption | Test with a seeded old data folder | New folder holds an identical database; old folder byte-identical; an interrupted copy leaves no half database; legacy temp prefix and recovery store still honoured |
| AC-ID-4 | Credit and licence | Inspect About and `LICENSE.md` | About shows "Based on the upstream project" and the MIT text; `LICENSE.md` still contains the upstream notice |
| AC-ID-5 | No Pro | `rg -i "license_key\|licensing\|/get-profile"` in source, excluding `migrations/` and the test in `database/manager.rs` that proves the table is dropped | Zero matches, and a new migration drops the `licensing` table |
| AC-UI-1 | Contrast | `pnpm lint:contrast` | Every pair listed in `tokens.contrast.json` meets its role's threshold (4.5 text, 3.0 large text, icon or control border), in both themes. Decorative and exempt pairs are named in the file, not skipped silently |
| AC-UI-2 | No raw colours | `pnpm lint:colors` | Zero raw palette classes or hex colours outside `tokens.css` |
| AC-UI-3 | Visual review | Playwright screenshots of record, meeting, sidebar, people, settings, dialogs in light and dark, at 1280 by 800 | Reviewed against the token tables and the aesthetic reference (the concept image (not kept in the repository), mood only, not a layout); no clipped text; no component using the old grey palette |
| AC-UI-4 | Keyboard | Playwright script tabbing through each screen | Every interactive element reachable and shows a visible focus ring; every target at least 24 by 24 px |
| AC-UI-5 | Fonts offline | Build with the network disabled | Build succeeds; fonts render from local files |
| AC-UI-6 | Icon | Render the SVG at 16, 32, 64, 256 px | Still recognisable as a Q at 16 px; `.ico` contains 16, 32, 48, 256 |
| AC-RET-1 | Expiry decision | Rust unit tests with an injected clock | For each of the six settings, audio is due exactly at `ended` plus the period, not one millisecond before; Keep is never due |
| AC-RET-2 | Sweep deletes | Test with fixture folders (audio, checkpoints, temp files) and a vault `_audio/<id>.m4a` with audio sync off | After a sweep all audio artefacts for expired meetings are gone including the vault copy, transcript and metadata files remain, `audio_state` is `deleted` |
| AC-RET-3 | Verified deletion | Test with a locked file (Windows) and a read-only file | State stays `present`, "deletion pending" shown, a later sweep succeeds when the lock clears |
| AC-RET-4 | Off mode safety | Tests: save succeeds, save fails, detection fails | Audio deleted only in the first case; kept and reported in the others |
| AC-RET-5 | Allowlist safety | Tests: symlink and junction inside the folder, folder without markers, path outside the recordings root, file with unknown extension | None of these is deleted |
| AC-RET-6 | Launch and schedule | Test starting the app with overdue meetings; test the interval with a fake timer | Overdue audio is deleted within 30 s of launch; a due item is deleted at the next 60-minute tick; a window focus event after a paused timer triggers a sweep; a simulated 3-hour sleep is caught up at the next tick |
| AC-RET-7 | Upgrade safety | Migration test on a database from commit `818ef52` | No existing audio is scheduled for deletion; all columns present |
| AC-RET-8 | Wording | Review of settings text | States the three limits in F-RET-10 |
| AC-TO-1 | No audio ever | Test with a synthetic audio source and a directory watcher (not polling) on the recording folder and the system temp folder during and after a 2-minute recording; also assert the incremental saver is never constructed | Zero create events for audio or temp-audio files |
| AC-TO-2 | Disabled features | UI test | Detect speakers, Enhance and playback show disabled with a reason; manual labels work |
| AC-QA-1 | Context builder | Unit tests | Never exceeds the token budget; includes the newest lines; BM25 picks the line containing the asked keyword from a 2-hour transcript; output deterministic |
| AC-QA-2 | Citations | Unit tests | A citation outside every supplied segment is dropped; a valid one links to its line |
| AC-QA-3 | Performance | Benchmark on your PC (a Windows laptop with 16 GB RAM and an NVIDIA GPU) in two profiles, portable CPU and CUDA, plugged in and on battery, with 30, 60 and 120 minute transcripts, each built-in model, Parakeet running | Thresholds recorded in plan.md after the first run, then enforced: no dropped audio chunks, transcription backlog under 5 s, and first token within the agreed limit; the two context budgets in F-QA-3 are tuned here |
| AC-QA-4 | Local only | Run with the network blocked | A question is answered; zero outbound connection attempts |
| AC-QA-5 | No cloud by default | Test: fresh settings, cloud switch off | No cloud provider selectable; `cloud_allowed()` is false; every cloud code path returns an error before any request |
| AC-CON-1 | Reminder | UI test | Appears on start, does not delay start (under 100 ms), "Never show again" persists across restart and can be reset in Settings |
| AC-CON-2 | Copy | `pnpm lint:copy` and review | No em dash; no sentence saying the user is or is not compliant; the phrase "US compliance required" is absent; guidance page present; the external link opens only through the allowlist and an unlisted host is refused |
| AC-CON-3 | First-run notice | UI test | Shown once; readable again in Settings |
| AC-MOD-1 | Models screen | UI test and review | Parakeet first; sizes, languages and links shown; a language outside Parakeet's list produces the Whisper recommendation |
| AC-MOD-2 | Keys | Test with seeded plaintext keys | Keys read back from Credential Manager; database columns null; no key in logs |
| AC-MOD-3 | Endpoints | Test | A non-local Ollama or CustomOpenAI host requires confirmation |
| AC-SYNC-1 | Round trip | Export a corpus of at least 50 generated meetings, import into an empty database | Titles, summaries, segments, speakers, people, folders and timestamps equal; second export writes zero files |
| AC-SYNC-2 | Shared fixtures | Run the fixtures in `sync-format.md` section 18 | All pass; unknown keys survive; byte-stable rewrite |
| AC-SYNC-3 | Conflicts | Fixture tests | Winner deterministic; loser in `_conflicts`; iCloud-style duplicate handled; nothing lost |
| AC-SYNC-4 | Deletion | Fixture tests | Delete propagates; edit-after-delete restores; audio delete beats extend; audio file removed from the vault |
| AC-SYNC-5 | Hostile input | Fixture tests and a fuzz run of 10,000 mutated files | No panic; no file deleted or overwritten that could not be parsed; no path outside the vault touched; markdown from the vault cannot run script in any renderer (summary editor and viewer) |
| AC-SYNC-6 | Windows rules | Tests | Reserved names, leading and trailing dots, long and Unicode folder names, emoji names over 80 UTF-16 units, `Work` and `work`, and two folders with the same cleaned name all handled; longest path 132 characters relative, root warning above 100 and refusal above 120 |
| AC-SYNC-7 | Real devices | Manual checklist on your PC and iPhone | The four unverified items in `sync-format.md` section 15 are answered and recorded before Phase F starts |
| AC-HK-1 | Telemetry gone | `rg -i posthog` in source and `Cargo.lock` | Zero matches |
| AC-HK-2 | Em dashes | `pnpm lint:dashes` | Zero U+2014 in tracked files outside the allowlist |
| AC-HK-3 | Offline inventory | Network inventory test | Every outbound call site has a tag; test fails on an untagged one |
| AC-HK-4 | Logs | Scripted session with debug logging, then search logs | No transcript text, question text or key |
| AC-ID-6 | Icons replaced | Compare every file under `src-tauri/icons/` and the installer images with the upstream base commit (`git show`) | None is byte-identical to the upstream file; About and the window show the new icon |
| AC-UI-7 | Theme | Playwright with emulated light and dark colour schemes, then the Settings override | The app follows the emulated scheme; a forced light or dark persists after restart; no `theme` is forced in `tauri.conf.json` |
| AC-UI-8 | Calm and few controls | Screenshot review of record, meeting and settings at 1280 by 800 in both themes, recorded in the PR | Each main screen has one primary action; extra controls above the fold are listed and justified; no shadows other than focus rings |
| AC-UI-9 | Sidebar illustration | File check and a screenshot | Each version is at most 60 KB, has no text, has empty alt text, loads with the network blocked, and does not reduce contrast of any text above it below its role's threshold |
| AC-RET-9 | Finalizer race | Test with an injected clock: an Off meeting, with the sweep firing between save and finalization | Audio is not deleted before `finalized_at` is set, and is deleted at finalization when the save succeeded |
| AC-RET-10 | Audio-only deletion and meeting delete | Tests: expired audio deleted while `transcripts.json` and `metadata.json` remain; meeting delete with a locked audio file | Per-file results returned; transcript files untouched; the meeting stays listed as deleting and the retry succeeds when the lock clears; no orphan folder |
| AC-RET-11 | Recovery, import, orphans | Tests: recover a crashed recording, import an audio file, create a folder with no row | Recovered and imported meetings get a deadline from the current setting; the orphan folder is listed as unmanaged and never deleted |
| AC-RET-12 | Mass-expiry guard and clock | Tests: 21 recordings due at once; clock set backwards | The sweep stops and asks; with a backwards clock nothing is deleted and a notice shows |
| AC-RET-13 | Settings behaviour | UI tests | Changing the setting affects only new recordings; "Apply to existing" shows counts and needs confirmation; "Keep this one" and "Delete audio now" work; each meeting shows its audio status text |
| AC-RET-14 | Scheduled task | Manual checklist on Windows | Opt-in creates one Task Scheduler entry that runs `--retention-sweep` with no window and exits; one click removes it |
| AC-RET-15 | Detect on save | Tests with the setting on and off, and with detection failing | With the setting on, detection runs before Off deletes the audio; a failure keeps the audio; with the setting off the retention text states the limit |
| AC-TO-3 | Mode persistence and finalization | UI and Rust tests | The switch persists across restart; a transcript-only recording ends with status completed, a duration and the saved event; `mode` and `audio_state` are saved |
| AC-TO-4 | Wording | Review | The text matches F-TO-4 and the one-sentence difference between Off and transcript-only is shown (F-TO-5) |
| AC-QA-6 | Priority and cancel | Test with a stalled model and a growing transcription backlog | Cancel stops generation within 2 s; recording continues; a request is refused with a message when the backlog is over the limit |
| AC-QA-7 | Session, note, safety, no model | UI and unit tests | History is gone after restart; "Add to note" appends a labelled block; a hostile answer (HTML, script) renders inert; with no model the box shows the download prompt |
| AC-NOTE-1 | Notes persist | Rust test and UI test: type notes on a saved meeting and during a recording, restart | Notes are back unchanged; notes typed during a recording are attached to the saved meeting; deleting the meeting deletes them |
| AC-NOTE-2 | Notes in the summary | Unit test on the prompt builder | With notes, the final step contains one `<attendee_notes>` block and the F-NOTE-3 rules; without notes the prompt is byte-identical to today's |
| AC-NOTE-3 | Summary never edits notes | Test: generate and regenerate a summary on a meeting with notes | The stored notes are unchanged |
| AC-NOTE-4 | Crash safety | Test: write notes, stop without saving the meeting, recover | Notes come back with the recovered meeting |
| AC-NOTE-5 | Ask reads notes | Unit test on the context builder | Notes are included, labelled `[your notes]`, ahead of transcript lines |
| AC-NM-1 | Instant notes | UI test | "New meeting" shows a focused notes editor in under 500 ms; no recording command is called |
| AC-NM-2 | Record into the meeting | Rust test and UI test | Recording from a meeting saves its transcript and folder on that meeting id; the meeting count rises by one in total, not two; notes are unchanged |
| AC-NM-3 | Empty meetings | Rust test and UI test | Leaving an untouched new meeting removes its row; a meeting with notes, a recording or a new title stays; no file is deleted |
| AC-NM-4 | Crash during a meeting recording | Test: record from a meeting, stop without saving, recover | The recovered transcript attaches to the original meeting, with its notes |
| AC-NM-5 | Summary from notes alone | Unit test on the prompt builder (after D5) | A meeting with notes and no transcript produces a summary prompt built from the notes |
| AC-MD-1 | Opt-in | Filesystem test and UI test | No file is written before a folder is chosen and confirmed; the confirmation names synced folders |
| AC-MD-2 | File content | Unit tests on the pure renderer with fixture meetings | Exact expected bytes for a meeting with notes, summary and transcript, and for notes only; empty sections left out; front matter parses as strict YAML and Obsidian properties |
| AC-MD-3 | Names and places | Unit tests | Date and title naming; reserved and over-long names cleaned; clashes get ` (2)`; title, folder and move changes rename or move the one file; nothing outside the chosen folder is touched; a symbolic link is not followed |
| AC-MD-4 | Outside edits | Filesystem tests | An edited file is not overwritten or deleted and is listed; "Replace" restores Quire's version; an unchanged file is updated and deleted with its meeting |
| AC-MD-5 | Triggers | Rust and UI tests | Save, summary and notes changes each update the file once; "Export all now" exports every meeting; a second run with no changes writes zero files |
| AC-MD-6 | Nothing extra leaves | Unit tests with hostile fixtures | No audio, voiceprint or key in any file; image embeds, raw HTML and `javascript:` links come out as plain text |
| AC-MD-7 | Obsidian | Manual check on the Mac | A real Obsidian vault shows the files, properties and folders; opening a file makes no network request |
| AC-MOD-4 | Cloud switch rules | Tests | Turning the switch on shows the confirmation naming what is sent; an upgrade with a configured cloud provider starts on with a notice and a fresh install starts off; with the credential store unavailable the app refuses to save a key and writes no plaintext |
| AC-MOD-5 | Download manifest | Test with a corrupted download | A hash mismatch deletes the file and refuses to load it |
| AC-MOD-6 | API transcription | Test and review | Each provider is labelled "Sends audio to ..." and unavailable with the switch off; providers that fail end to end are hidden |
| AC-SYNC-8 | Sync screen | UI and filesystem tests | No file is written before the folder is chosen and confirmed; the disclosure shows before the first write; audio sync is off by default; Off and transcript-only recordings are never uploaded with audio sync on; turning sync off leaves files and stops writes; removing the app's vault files needs a separate confirmation |
| AC-SYNC-9 | Never synced | Test over an exported vault | No voiceprint, key, model file or setting appears in any file |
| AC-HK-5 | Dead code and updater | `rg` and build | Listed dead files are gone and the build passes; the updater plugin, dialog, About and tray entries are removed or disabled; no network call to the updater endpoint |
| AC-HK-6 | CLAUDE.md and hook | Review | CLAUDE.md is under one page with build, test, lint and a verify block; the hook blocks edits to existing test files when `LOCK_TESTS=1` is set (used for bug-fix tasks, so new tests can still be written in feature work) |
| AC-HK-7 | Recordings folder | Tests | The recorder and the importer write to the saved preference; the picker warns for a path inside OneDrive, iCloud Drive or Dropbox; a new install defaults to the app data folder |
| AC-HK-8 | ffmpeg licence | Run `ffmpeg -version` on the bundled binary | The result is recorded in the PR; if it is GPL, one of the three resolutions in concern 17 is applied |
| AC-HK-9 | Migration backup | Test | A dated backup exists before a new migration runs; only the last three are kept |
| AC-NF-1 | Performance | Benchmarks with fixtures | A sweep over 1,000 meetings takes under 2 s when nothing is due; the reminder adds under 100 ms; startup is within 10 percent of `818ef52`; export of 500 meetings takes under 60 s on the target PC |
| AC-NF-2 | Atomicity | Kill-the-process tests during a sweep and an export | No half-written note or sidecar; no meeting marked `deleted` whose files still exist |
| AC-NF-3 | Portability | `cargo check --lib` and a grep | Builds on Linux with Windows-only code behind `cfg(windows)`; paths come from Tauri path APIs |
| AC-PROC-1 | Build and test | `cargo test --lib`, `pnpm exec tsc --noEmit`, `pnpm lint`, UI tests | All green, output pasted |
| AC-PROC-2 | Independent review | A reviewer subagent or fresh session that did not see the build reviews the diff against this spec and plan.md | Findings recorded; every Important finding fixed or accepted by you |

### Traceability

Every requirement maps to at least one criterion. `scripts/check-traceability.mjs` (Stage 3) fails if a requirement ID in section 2 is missing from this table or points at an unknown criterion.

| Requirement | Criteria |
|---|---|
| F-ID-1, F-ID-2 | AC-ID-1 |
| F-ID-3, F-ID-9 | AC-ID-2 |
| F-ID-4 | AC-ID-3 |
| F-ID-5, F-ID-6 | AC-ID-4 |
| F-ID-7 | AC-ID-6 |
| F-ID-8 | AC-ID-5 |
| F-UI-1, F-UI-7 | AC-UI-2 |
| F-UI-2 | AC-UI-7 |
| F-UI-3 | AC-UI-1 |
| F-UI-4 | AC-UI-5 |
| F-UI-5, F-UI-6 | AC-UI-3, AC-UI-8 |
| F-UI-8 | AC-UI-4 |
| F-UI-9 | AC-UI-6 |
| F-UI-10 | AC-UI-9 |
| F-RET-1 | AC-RET-1, AC-RET-13 |
| F-RET-2 | AC-RET-7, AC-RET-11 |
| F-RET-3 | AC-RET-4, AC-RET-9, AC-RET-15 |
| F-RET-4 | AC-RET-6 |
| F-RET-5 | AC-RET-2 |
| F-RET-6 | AC-RET-3 |
| F-RET-7 | AC-RET-5, AC-RET-10 |
| F-RET-8, F-RET-9 | AC-RET-13 |
| F-RET-10 | AC-RET-8 |
| F-RET-11 | AC-RET-14 |
| F-RET-12 | AC-RET-10 |
| F-RET-13 | AC-RET-12 |
| F-RET-14 | AC-RET-15 |
| F-TO-1, F-TO-2 | AC-TO-1, AC-TO-3 |
| F-TO-3 | AC-TO-2 |
| F-TO-4, F-TO-5 | AC-TO-4 |
| F-QA-1, F-QA-2 | AC-QA-4, AC-QA-5 |
| F-QA-3 | AC-QA-1, AC-QA-3 |
| F-QA-4 | AC-QA-2 |
| F-QA-5 | AC-QA-3, AC-QA-6 |
| F-QA-6, F-QA-7, F-QA-8 | AC-QA-7 |
| F-NOTE-1, F-NOTE-2 | AC-NOTE-1, AC-NOTE-4 |
| F-NOTE-3, F-NOTE-4, F-NOTE-6 | AC-NOTE-2, AC-NOTE-3 |
| F-NOTE-5 | AC-NOTE-5 |
| F-NOTE-7 | AC-QA-4, AC-QA-5 |
| F-NM-1, F-NM-2 | AC-NM-1 |
| F-NM-3, F-NM-4, F-NM-6 | AC-NM-2, AC-NM-4 |
| F-NM-5 | AC-NM-3 |
| F-NM-7 | AC-NM-5 |
| F-MD-1 | AC-MD-1 |
| F-MD-2, F-MD-3 | AC-MD-2, AC-MD-7 |
| F-MD-4, F-MD-7 | AC-MD-3 |
| F-MD-5 | AC-MD-5 |
| F-MD-6, F-MD-9 | AC-MD-4 |
| F-MD-8 | AC-MD-6, AC-MD-7 |
| F-CON-1, F-CON-2, F-CON-3 | AC-CON-1 |
| F-CON-4, F-CON-5 | AC-CON-2 |
| F-CON-6 | AC-CON-3 |
| F-MOD-1, F-MOD-2, F-MOD-4 | AC-MOD-1 |
| F-MOD-3 | AC-MOD-6 |
| F-MOD-5 | AC-MOD-4 |
| F-MOD-6 | AC-MOD-2, AC-MOD-4 |
| F-MOD-7 | AC-MOD-3 |
| F-MOD-8 | AC-MOD-5 |
| F-SYNC-1, F-SYNC-2, F-SYNC-3, F-SYNC-9 | AC-SYNC-8 |
| F-SYNC-4 | AC-SYNC-1 |
| F-SYNC-5 | AC-SYNC-2, AC-SYNC-3, AC-SYNC-4, AC-SYNC-6, AC-SYNC-7 |
| F-SYNC-6 | AC-SYNC-3 |
| F-SYNC-7 | AC-SYNC-5 |
| F-SYNC-8 | AC-RET-2, AC-SYNC-4 |
| F-SYNC-10 | AC-SYNC-9 |
| F-HK-1 | AC-ID-5 |
| F-HK-2 | AC-HK-1 |
| F-HK-3, F-HK-4 | AC-HK-5 |
| F-HK-5 | AC-HK-2 |
| F-HK-6 | AC-HK-6 |
| F-HK-7 | AC-HK-7 |
| F-HK-8 | AC-HK-8 |
| F-HK-9 | AC-HK-9 |
| NF-1 | AC-HK-3, AC-QA-4 |
| NF-2 | AC-UI-1, AC-UI-4 |
| NF-3 | AC-NF-1 |
| NF-4 | AC-NF-2 |
| NF-5 | AC-NF-3 |
| NF-6 | AC-HK-4 |

## 5. Data handling

| Data | Where | Who can see it | Retention | Sensitivity |
|---|---|---|---|---|
| Audio | Per-meeting folder under the recordings folder (new installs: app data folder). In the vault `_audio/` only if audio sync is on. | You, anyone with access to your Windows account. With audio sync on, Apple's servers. | By the Retention setting. | High. Can contain other people's voices. |
| Transcripts and summaries | SQLite in app data. In the vault as markdown and JSON if sync is on. | You and anyone with your account. With sync on, Apple's servers. | Until you delete the meeting. | High. |
| People, notes about people | SQLite. In the vault `_people/` if sync is on. | As above. | Until deleted. | Medium. |
| Voiceprints | SQLite only (`meeting_speakers`). Never synced. | You. | Until the meeting or person is deleted. | High. Biometric-like data about other people. |
| API keys | Windows Credential Manager (after F-MOD-6). Today plaintext in SQLite. | You. | Until removed. | High. |
| Model files | App data. | You. | Until removed. | Low. |
| Crash-recovery lines | WebView IndexedDB, until the meeting is saved. | You. | Cleared on save; legacy name honoured. | High while present. |
| Logs | Files under app data. | You. | Rotated. Must contain no content (NF-6). | Medium. |
| Q&A history | Memory only. | You. | Session. "Add to note" writes to the note. | High. |
| Cloud-provider traffic | Only if the cloud switch is on and a provider is chosen. | The provider. | Provider's policy. | High. |

Sensitive items flagged: other people's voices and words (consent, concern 20 and 21), voiceprints (concern 21), plaintext keys today (concern 15), anything in iCloud (concerns 4 and 5), and work meetings on a personal device (concern 19).

## 6. Policies applied

* `intent.md`, accepted 2026-11-01: local first, sync format constraints, retention behaviour, consent wording, personal use only, no em dashes.
* Repository `CLAUDE.md` (upstream version): Tauri command and event pattern, audio pipeline structure, `perf_debug!` for hot paths, Rust `anyhow::Result`, naming "microphone" and "system", no dependence on the archived backend.
* The AI-native SDLC skill (Anthropic playbook, as installed in this session): artifact chain, solo mode gates, failing test first, independent review, CLAUDE.md verify block.
* WCAG 2.2 level AA for contrast, focus and target size.
* Upstream MIT licence terms (keep notice, credit).
* The previous security review (`docs/SPEAKERS_FOLDERS_PEOPLE.md`): SHA-256 pinning, allowlist-based deletion, URL scheme allowlist, invisible-character stripping, no shell from user strings. These are extended to the vault.
* Apple iCloud documentation and sources checked in this session for iCloud for Windows behaviour and the 30-day recovery window. Re-verified on devices at AC-SYNC-7.

Not applied, because none exist in this session: any organization security, brand or data-handling policy skill, and any review by a policy owner. Nothing here is legal advice.

## 7. Flagged concerns

"[Cannot fully satisfy]" marks a requirement that the physical or legal situation prevents from being met exactly as written. The resolution says what the spec does instead.

| # | Concern | Policy or source | Owner | Resolution |
|---|---|---|---|---|
| 1 | Scope mismatch: intent.md is iPhone-first; the desktop rename, re-skin, consent redesign and model screen are not in it. | intent.md | the owner | Section 0 addendum, applied only on your yes. |
| 2 | [Cannot fully satisfy] intent says transcript-only "is the same as" retention Off. In code they differ: Off writes audio then deletes it, transcript-only never writes it. Also, speaker detection is started by hand today, so Off would delete audio before detection could run. | intent.md, Proposed outcome | the owner | Two options with a one-sentence explanation (F-TO-5), plus a new detect-on-save setting so Off stays useful (F-RET-14). Decision D4. |
| 3 | Speaker detection, Enhance, voice memory and playback need audio. Off and transcript-only disable some of them. | Design | the owner | Off runs detection before deleting only when the new detect-on-save setting is on (F-RET-14). Transcript-only disables them with a stated reason (F-TO-3). Live detection from in-memory audio is a possible later feature. |
| 4 | [Cannot fully satisfy] "Nothing leaves the device by default" and "transcripts and notes sync by default". Synced files reach Apple's servers. | intent.md, Constraints | the owner | Sync is off until you pick a vault and confirm (F-SYNC-1, F-SYNC-2). After that, transcripts and notes are included by default. This reading of "by default" needs your accept (D3b). |
| 5 | [Cannot fully satisfy] Deleting audio cannot be guaranteed unrecoverable. iCloud keeps deleted files for up to 30 days, offline devices keep copies until they reconnect, backups and SSD wear leveling are outside the app. | Apple docs; intent.md | the owner | Audio sync off by default. "Deleted" is defined as removed from every place the app can reach, and the UI says so (F-RET-10). |
| 6 | [Cannot fully satisfy] The sweep only runs while the app runs. iOS and Windows give no timer guarantee. | intent.md, Constraints | the owner | Launch, hourly and on focus; optional daily scheduled task on Windows (F-RET-11); deadlines live in metadata so every device enforces them (sync-format section 10). Stated in the settings text. |
| 7 | Recordings always go to the default folder under Music today, because the saved folder preference is ignored. OneDrive known-folder backup may cover Music, copying audio to the cloud without you choosing it. | Windows behaviour | the owner | Make the folder a real setting, default new installs to the app data folder, and warn about sync roots (F-HK-7). Existing installs keep the Music folder until they choose. |
| 8 | Changing the bundle identifier changes the data folder, so the existing database would not be found. | Tauri | the owner | Copy-based adoption with confirmation, old folder untouched, legacy names recognised (F-ID-4). |
| 9 | Your suggested muted and accent text colours fail AA for small text (3.64 and 3.77 against 4.5). | WCAG 2.2 | the owner (taste) | Corrected `muted` and new `accent-strong`; the original accent stays as a decorative colour (section 3.2). |
| 10 | Re-skin is large: 1,283 raw palette class uses in 81 files. | Codebase | the owner | Tokens, then codemod, then lint, one screen at a time. Phase A is the largest phase. |
| 11 | Fonts load from Google at build time today. | intent.md (local first) | the owner | Bundle Source Serif 4 and Source Sans 3 locally (F-UI-4). |
| 12 | Name collisions. A task-management product named Quire (quire.io) exists, and several apps named Marginalia exist. No trademark search was run. | intent.md open question 5 | the owner | Not a blocker for personal use and TestFlight. Decide before an App Store release. The brand config makes switching cheap. |
| 13 | Pro scaffolding remains (licensing table, profile commands with `license_key`, profile calls to a local server on port 5167). | "Do not copy Pro" | the owner | Removed (F-HK-1, AC-ID-5). |
| 14 | Telemetry code remains. The key is empty so nothing is sent, but that is a config fact, not a code fact. | Local first | the owner | Removed (F-HK-2, AC-HK-1). |
| 15 | API keys are stored in plaintext columns in SQLite. | Security | the owner | Windows Credential Manager with migration (F-MOD-6). No plaintext fallback. |
| 16 | Updater has upstream's public key with this fork's endpoint, so updates cannot verify. The plugin is also wired into `lib.rs`, the update dialog, About and the tray. | Security | the owner | Updater disabled until the fork has its own signing key (F-HK-4). |
| 17 | The Windows ffmpeg bundled in the installer is the "essentials" build from gyan.dev. I believe that build is GPL v3. Sharing the installer with your friend is distribution, which would carry GPL obligations (provide source, licence text). I have not verified this. | Licensing | the owner | Check with `ffmpeg -version` (look for `--enable-gpl`). If GPL: switch to an LGPL build, or download ffmpeg on first run instead of bundling, or ship the licence and source offer (F-HK-8). Applies to the installer you already built. |
| 18 | The intent's "send the transcript to my desktop" for the phone implies a listening server on the desktop. That conflicts with "no server I run" and with the app's current no-listening-sockets posture. | intent.md | the owner | Not in this spec. The iOS app can use a local model or a user-supplied Ollama address. Revisit as its own intent. |
| 19 | Work use on a personal device, plus iCloud, may implicate records and information-handling rules. | intent.md open question 6 | the owner | Carried forward. The app shows the personal-use notice (F-CON-6) and cannot enforce it. Not a build blocker, a use blocker until answered. |
| 20 | The existing consent notice makes a legal claim ("US compliance required"). | intent.md, Constraints | the owner | Removed. Guidance only, not legal advice, with a verified external reference (F-CON-5). |
| 21 | Voiceprints are biometric-like data about other people. Some state laws name voiceprints as biometric identifiers (for example Illinois), mostly for commercial use. Personal use is lower risk. | Privacy law, not legal advice | the owner | Voiceprints stay local, never sync, and are removed with the meeting or person. Revisit before distributing the app beyond personal use. A "remember voices" switch already exists in the detection dialog. |
| 22 | The em dash rule. The repository has 119 in source (code comments and a few user-visible strings) and none in any other tracked text file. | intent.md | the owner | Clean all 119, lint every tracked text file, no exemptions needed today (F-HK-5). Settled, no decision needed. |
| 23 | iCloud for Windows behaviour is only partly documented: placeholder attribute, conflict copies, dot-names, directory rename handling. | Apple docs gaps | the owner (needs devices) | Phase E (export only) ships first. Phase F waits for AC-SYNC-7. |
| 24 | Running transcription and a language model at once may overheat or slow the PC. Thresholds are an open question. | intent.md open question 3 | the owner | Benchmark first (AC-QA-3) on your PC (D9, answered): 16 GB RAM, NVIDIA GPU. It is a laptop, so the benchmark runs plugged in and on battery and records clock and fan behaviour, and runs with your usual apps open. |
| 25 | Someone could say "ignore your instructions" during a meeting and have it reach the model. | Security | the owner | The model has no tools or actions; answers are labelled and sanitized (F-QA-7). Residual risk is a wrong answer. |
| 26 | Logs may carry transcript text at debug level. | Privacy | the owner | Audit and test (NF-6, AC-HK-4). |
| 27 | Cloud model providers and custom endpoints can send transcript text off the computer. | intent.md | the owner | Master cloud switch, labels, confirmation, non-local host warning (F-MOD-5, F-MOD-7). |
| 28 | Process: the repository CLAUDE.md is upstream's, 409 lines, and describes a legacy backend. | SDLC skill | the owner | Replaced in Phase A (F-HK-6). Not written yet because Stage 3 has not started. |
| 29 | Model downloads for Whisper and Parakeet are not hash-pinned like the speaker model and ffmpeg. | Security review | the owner | Manifest with SHA-256 (F-MOD-8, should). The Parakeet v3 download also uses an upstream-hosted address, a single point of failure for onboarding, to be reviewed in Phase C. |
| 30 | The Windows installer build is CPU-only, so prompt processing on a large transcript may be slow beside Parakeet. | Performance, intent open question 3 | the owner | Small starting context budgets, tuned by benchmark (F-QA-3, AC-QA-3). For your own PC a CUDA build is possible with the NVIDIA GPU (confirm its video memory in Task Manager; the laptop part normally has 8 GB). It is a separate build profile from the shared portable installer, scheduled in Phase D. |
| 31 | Migrations are one way. A newer build that migrates the database cannot be undone by installing an older build. | Reliability | the owner | Dated database backup before migration (F-HK-9). |
| 32 | Antivirus scanners, the audio player and sync clients can lock audio files on Windows, which blocks deletion. | Windows behaviour | the owner | Retry-based deletion, a "deletion pending" status, and never a false "deleted" (F-RET-6). |
| 33 | Existing installs may already use Claude, OpenAI or other cloud providers, which conflicts with "cloud off by default". | intent.md | the owner | The switch starts on, with a notice, only for installs that already have a cloud provider and key (F-MOD-5). Fresh installs start off. |
| 34 | `auto_save` is a global setting and its disabled path never finalizes the meeting (metadata stays "recording"). Transcript-only builds on it. | Code | the owner | Per-recording flag and a finalization path (F-TO-2, section 3.3). |
| 35 | Vault content is rendered by more than one markdown path in the WebView, which holds broad file permissions. | Security | the owner | Content passes through validated Tauri commands only, every renderer is tested with hostile bodies, and permissions are narrowed in the plan (F-SYNC-7, AC-SYNC-5). |
| 36 | Draft 1 of the sync format allowed simultaneous expiry conflicts on several devices, resurrected deleted notes, and path-derived deletion. | Independent review | the owner | Fixed in `sync-format.md` draft 2 (sections 10, 11, 12, 14). Fixtures cover each case (section 18). |

Decisions needed at the gate

* **D1** Source of truth for sync. Recommended: SQLite stays, vault is the exchange, built in two phases (section 3.7). Alternative: files first.
* **D2** Bundle identifier. Recommended: new identifier plus data adoption (F-ID-4). Alternative: keep `com.meetily.ai` internally and change only visible names.
* **D3** Defaults. (a) New installs record to the app data folder; existing installs keep the Music folder and keep audio forever until they choose. (b) "Sync by default" means "after you enable sync, transcripts and notes are included".
* **D4** Keep retention Off and transcript-only as two options, with a new detect-on-save setting (F-RET-14) so Off can still use speaker detection (recommended), or merge them as intent.md says and accept that detection after the fact is lost.
* **D5** (settled) Em dashes: clean all 119 and lint every tracked text file. No decision needed unless you want an exemption.
* **D6** Apply the intent.md addendum in section 0.
* **D7** Default retention for new installs: 7 days (recommended) or Keep.
* **D8** Desktop API transcription: verify and keep behind the cloud switch, or hide it for now (recommended: hide unless it already works).
* **D9** Your Windows PC's CPU, GPU and RAM, so the Q&A benchmark thresholds and the default model can be set.

Recorded on 2026-11-01: the owner answered "Accept" with no changes, so D1 to D4 and D6 to D8 stand as recommended, D5 is settled, and D9 is answered in section 8 (hardware). The UI concept image was supplied as an aesthetic reference only (section 3.2).

## 8. Open questions from intent

1. **How the fork stores notes and folders.** Answered in section 1. Reading and writing the shared format is designed in `sync-format.md` and section 3.7. The first-pass decision is D1.
2. **Which on-device LLM.** Desktop: the four built-in models already in the app (Qwen 3.5 2B at 1.2 GB, Qwen 3.5 4B at 2.6 GB, Gemma 3 4B at 2.4 GB, Gemma 3 1B at 1.0 GB, 32k context) plus Ollama. Answer from your hardware (D9): a Windows 11 laptop with 16 GB RAM and an NVIDIA GPU. Starting default is Qwen 3.5 2B for everyone, and 4B is offered when installed RAM is 16 GB or more. RAM is the likelier limit than CPU: Windows showed 2.66 GB free when you captured the system screen. The Q&A benchmark (AC-QA-3) decides whether 4B becomes your default. Carried to the iOS spec: candidates are Apple's on-device Foundation Models framework (no download, availability to be verified on the target iPhone and OS) versus a small downloaded model. Decide after a test on the device.
3. **Battery and heat.** Carried. Desktop: measured in AC-QA-3. Proposed starting thresholds for the iOS spec, to confirm: a 60-minute live transcription uses no more than 25 percent battery with the screen off, and the system thermal state never reaches serious.
4. **Desktop changes for the shared format and retention.** Answered in section 1 (yes), designed in sections 2.3, 2.8 and 3.
5. **Final name and icon.** Carried. Working name Quire, backup Marginalia. Findings in concern 12. An icon concept exists (section 3.2) and prompts for exploring more are in section 3.9.
6. **Local policy on recording work meetings with a personal device.** Carried unchanged. Owner: the owner. It gates work use of the app, not the build.

## 9. Suggested delivery phases (input to plan.md, after you accept)

Each phase is independently shippable, ends in its own tests and an independent review, and is a separate gate.

* **A. Identity, tokens, housekeeping.** Brand config and generator, data adoption, tokens, fonts, codemod and lint, icon, About, removal of Pro, telemetry and dead files, updater off, em dash lint, database backup before migration, the recordings folder setting, the sage token and the aesthetic reference, one-page CLAUDE.md with verify block and a hook that locks tests during bug-fix tasks. Largest phase because of the re-skin.
* **B. Privacy core.** Retention setting, sweep and its guards, detect-on-save, per-recording transcript-only with a finalization path, the audio-only deletion function and two-phase meeting delete, consent reminder, first-run notice. Failing tests first for every deletion rule.
* **C. Models.** Models screen, cloud switch, Credential Manager migration, model manifest.
* **D. Live Q&A and My notes.** My notes pane and storage (F-NOTE), notes in the summary prompt, benchmark on your PC in two profiles (portable CPU and a CUDA build for the NVIDIA GPU), context builder, Ask box, citations. Moved to run straight after Phase A (see section 12).
* **E. Vault export.** Format library with fixtures, one-way export, Sync screen, disclosure.
* **F. Two-way sync.** Import, conflicts, deletion propagation, real-device verification first.

Rule for bugs found during any phase: reproduce as a failing test, commit it, then fix without editing that test.

## 10. Independent review log

A reviewer that had not seen the drafting checked these documents against `intent.md` and the repository on 2026-11-01. Its verdict on draft 1 was "not ready as written". Disposition:

* Applied to this spec: a detect-on-save setting, because speaker detection is manual today (F-RET-14); deadlines set at finalization (F-RET-2, F-RET-3); recovery, import and orphan handling (F-RET-2); an audio-only deletion function and a two-phase meeting delete, because the existing function deletes transcripts and the delete order orphans folders (F-RET-7, F-RET-12); vault audio always removed (F-RET-5); a mass-expiry guard (F-RET-13); per-recording transcript-only with a finalization path (F-TO-2); consent wording that no longer asserts a legal duty (F-TO-4, section 3.5); an upgrade rule for existing cloud users and the full key inventory (F-MOD-5, F-MOD-6); a real recordings folder setting (F-HK-7); the WebView profile in data adoption (F-ID-4); lint allowlists (F-ID-9, AC-ID-2, AC-ID-5); contrast roles and the forced Light theme (F-UI-2, F-UI-3, AC-UI-1); Q&A budgets as parameters (F-QA-3); a migration backup (F-HK-9); the section 0 list of additions beyond intent; and criteria for the 28 requirements that had none (traceability table).
* Applied to the format (`sync-format.md` draft 2): one deterministic merge function, expiry that never rewrites a note, tombstone and absence rules, content-addressed sidecars, a normative front matter grammar, derived audio paths, name cleaning and path budget numbers, move handling, import suffixes.
* Corrected facts: the licence file is `LICENSE.md`, the profile server is local, more dead files exist, the updater is wired into four places, and no tracked file outside source has an em dash.
* Not changed: the raw palette count stays at 1,283 (the reviewer counted 1,310 with a broader pattern); Phase A remains the largest phase.
* Left for the plan: key migration code, narrowing the WebView file permissions, and the benchmark thresholds.

These revisions have not had a second independent pass. A fresh-session read of the final spec is recommended after you accept or revise.

## 11. Amendments after planning (2026-11-01)

Plan-mode exploration for Phase A found facts that change some assumptions. The spec text above has been amended to match, and `plan.md` records the reasoning.

1. F-HK-6: upstream's long document moves to `docs/UPSTREAM_NOTES.md`, not `docs/ARCHITECTURE.md` (case collision on Windows).
2. AC-HK-6: the hook locks existing tests when `LOCK_TESTS=1` is set.
3. F-UI-4: fonts ship through pinned fontsource packages.
4. Token `muted` is `ink-muted` in CSS and Tailwind.
5. F-ID-4 and the identifier: `com.openedtools.notes`, name-independent. Adoption is more than a folder copy (stores, copied models, snapshot of the database, runs before app state exists, asks once, refuses a database from a newer version).
6. F-HK-1: backend-connection commands, the 5167 CSP entry and the sidebar server address also go.
7. F-HK-3: the dead-file list is longer, and one migration drops both unused tables.
8. F-ID-5: the About screen drops the upstream marketing block and the analytics switch.
9. Acceptance criteria are assigned to phases in `plan.md`. AC-HK-3 moves to Phase C and AC-HK-4 to Phase B.
10. Concern 29: the Parakeet v3 model host is upstream-hosted.

## 12. Amendments after the A3 review (2026-11-01)

* Added section 2.11 My notes (F-NOTE-1 to F-NOTE-7), design 3.10 and AC-NOTE-1 to AC-NOTE-5, at the owner's request.
* Phase D (Live Q&A, now with My notes) runs straight after Phase A, before B, C, E and F, because notes and Ask share the meeting's right-hand pane and the owner wants them first. Order: A, D, B, C, E, F.
* The final icon is the owner's own artwork (`brand/source`), replacing the SVG concept in 3.2. Section headings are sans; serif stays for the wordmark and page titles (a change to F-UI-4). Corners are 8 px for cards and 6 px for controls (a change to F-UI-5).
* AC-ID-5 excludes the one test that must name the `licensing` table to prove the migration drops it (found by the Phase A review).

## 13. Amendments during Phase D (2026-11-02)

* Added section 2.12 Notes-first meetings (F-NM-1 to F-NM-7) and AC-NM-1 to AC-NM-4, at the owner's request. Delivered as checkpoint D5 in `plan.md`.
* Design note: today the meeting row is created a few seconds after Stop (`TranscriptsRepository::save_transcript`). With F-NM the row exists first, so saving a recording takes an optional existing meeting id and updates that row (folder path, transcripts) instead of inserting one. Live notes for a meeting-first recording save straight to SQLite, since the row exists; the IndexedDB store stays for the Record shortcut (F-NM-6).

## 14. Amendment: Markdown export (accepted 2026-11-02)

* Adds section 2.13 (F-MD-1 to F-MD-9) and AC-MD-1 to AC-MD-7, at the owner's request. Delivered as Phase M in `plan.md`, before Phase B.
* The shared vault format is unchanged. The export is a second, one-way output for reading in Obsidian, and nothing reads it back.
* Requirements added on my own judgment and open to cutting: F-MD-6 (never overwrite outside edits), F-MD-8 (remote content written as plain text) and the date prefix in F-MD-2.

## 15. Amendments for Phase B (accepted 2026-11-02)

Accepted with the Phase B plan (decisions PB1 to PB7):
* F-TO-3: the app has no audio playback today, so transcript-only disables speaker detection and Enhance only.
* F-RET-7 and AC-RET-5: deletion happens only inside a known recordings root (the current setting, the app data folder, the earlier default) or the exact folder a meeting names when it carries the app's marker files; a folder that more than one meeting points at is never deleted automatically. Both kinds are listed for the person instead.
* AC-CON-2: the consent guidance link opens through a host allowlist; the general link opener keeps its scheme check.
* F-RET-11 (scheduled task) is deferred to a checkpoint after Phase B.

## 16. Amendment: remove the upstream association (accepted 2026-11-02)

With Phase R in `plan.md`:
* F-ID-4 (data adoption from the earlier app) is retired; installs that adopted keep their recordings folder (PR3).
* F-ID-5: the About screen no longer credits the upstream project. F-ID-6 is unchanged: the upstream copyright and permission notice stays in `LICENSE.md`, and `THIRD_PARTY_NOTICES.md` points at it.
* F-ID-9 and AC-ID-2: the allowed mentions shrink to `LICENSE.md`, `THIRD_PARTY_NOTICES.md` and applied migrations, and the lint scans every tracked file.
* F-MOD-8 is pulled forward for the Parakeet model: a public source pinned by SHA-256 (PR4).

## 17. Amendments for Phase C (accepted 2026-11-02)

With Phase C in `plan.md`:
* F-MOD-6 and AC-MOD-2: keys go to the operating system's credential store (Credential Manager on Windows, Keychain on the Mac). The window never receives a key back. `geminiApiKey` is cleared with the others.
* F-MOD-5: a custom server or Ollama on another computer also needs the cloud switch (PC2).
* F-MOD-3 and AC-MOD-6: API transcription is removed, not verified, because no code sends audio to any provider (D8).
* F-MOD-8: the manifest covers the speaker model and Parakeet too; models already downloaded are checked once. The build's ffmpeg download is pinned on every platform and no longer comes from the upstream company (PC5).
* F-MOD-1: the language list moves into `models.json`.
