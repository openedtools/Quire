# Plan: Quire for iPhone

Status: approved by the owner on 2026-11-02 at the gate, with PI1 to PI6 as recommended, together with `spec-ios.md`. Same rules as the desktop: tests first, one commit per work package, an independent review per phase, no em dashes, no personal data in the repository.

## What the research found (shapes the plan)
* This Mac has Xcode 26.6 and the iOS 26.5 simulator, so the app can be built and tested here. Running it on the owner's iPhone needs the phone connected to a Mac once, and a signing team.
* Free signing (a personal Apple ID) installs on the owner's own phone for 7 days at a time. TestFlight needs the paid Apple Developer Program (99 US dollars a year).
* The desktop already defines everything the phone shares: the vault format and its fixtures, the retention rules, the consent wording, the design tokens. The phone implements them; it does not invent new ones.
* Apple's on-device Foundation Models (iOS 26, Apple Intelligence phones) need no download, but their context window is small (about 4,000 tokens, check live), so a long meeting needs the summary split the way the desktop splits for small models.
* Phone recordings join the vault as new notes only. Editing desktop meetings on the phone waits for desktop Phase F.

## Decisions for you at this gate
* **PI1. Where the code lives.** Recommended: an `ios/` folder in the Quire repository, so the vault fixtures, the format document and the design tokens are shared and tested together. Alternative: a separate `Quire-iOS` repository (a new public repository).
* **PI2. Lowest iOS version.** Recommended: iOS 26, the version your phone runs, which also brings Foundation Models; the app is for you first. Alternative: iOS 18, for older phones, without Foundation Models.
* **PI3. Transcription.** Recommended: Parakeet v3 through FluidAudio (as intent.md says), downloaded once in the app with a size shown first; Whisper for other languages comes in a later phase.
* **PI4. Summaries and Ask.** Recommended: Apple's on-device Foundation Models, with long transcripts split; no download, nothing leaves the phone. Alternative: a downloaded Qwen 3.5 2B (1.2 GB) through llama.cpp, the desktop's model, which is larger and warmer.
* **PI5. Where meetings live on the phone.** Recommended: the vault folder itself, the format being the store (no second database), with a small local cache for search; meetings recorded with no vault chosen yet stay in the app's own folder in the same format and move in when one is chosen.
* **PI6. Signing.** Recommended: free signing to your own phone for now, the team ID kept in an untracked file; join the Developer Program only when you want TestFlight.

## Design direction (the owner's concept images, 2026-11-02)
The images themselves stay outside the repository: their sample transcript shows real names. What they set, in words:
* **Home:** a large serif title ("Record & Transcribe"), one large terracotta microphone button with "Tap to record", folders as rounded cards with item counts, a floating add button, a bottom tab bar (Home, Transcripts, Library), a soft sage landscape top left and a small sun accent.
* **Recording:** a close button, the title with its folder path as a breadcrumb, a sage waveform, a large timer, pause, a large terracotta stop button and a bookmark button; transcription settings in a bottom sheet; a warm terracotta landscape along the bottom.
* **Meeting:** a serif title, date and duration, a segmented control (Transcript, Summary, Notes), transcript lines with a round initial for each speaker in its own colour, the name and the time; audio playback with a scrubber, speed and volume.
* **Where the concepts and the spec differ, settled at later gates:** the concepts offer "Cloud (API)" transcription, which the spec keeps off (if added, it sits behind the cloud switch: I2 gate); they show Parakeet v2, while the spec uses v3 for its 25 languages; playback shows only while retention keeps the audio (I3).
* All sample content in mockups, screenshots and tests is made up, like the desktop's.

## Order of work
* **I1. Skeleton and vault reader** (this gate): Xcode project under `ios/`, the design tokens, the Swift vault library with the shared fixtures passing (AC-IOS-1), choose the vault folder with the disclosure, and browse desktop meetings read-only (AC-IOS-2). Runs in the simulator against a generated vault; no microphone yet.
* **I2. Recording and live transcription:** FluidAudio Parakeet, background audio, saving as it goes, transcript only, the consent reminder (AC-IOS-4, 6, 7).
* **I3. Retention and the vault writer:** the sweep with the desktop's rules, phone meetings written to the vault (AC-IOS-5, 8).
* **I4. Summaries and Ask** on the device (AC-IOS-3).
* **I5. Device pass:** battery and heat, lock screen, accessibility on your phone (AC-IOS-9, 10), then TestFlight if you want it.
Each of I2 to I5 gets its own short plan section and gate.

## I2. Recording and live transcription

Status: approved by the owner on 2026-11-02, with PI7 to PI12 as recommended. Covers F-IOS-REC-1, 2, 4, 5, F-IOS-PRIV-2 and 3, AC-IOS-4, 6, 7. Branch `enhance/quire-ios-i2`. Same rules: tests first, one commit per work package, independent review.

### What the research found
* FluidAudio 0.18.2 (Apache-2.0, iOS 17 and later, checked live on 2026-11-02) runs Parakeet TDT v3 on the Neural Engine. v3 is a batch model that works near real time and covers 25 European languages and Japanese; FluidAudio's word-by-word streaming model (Parakeet EOU) is English only. It also has Silero voice activity detection.
* The model repository at Hugging Face (`FluidInference/parakeet-tdt-0.6b-v3-coreml`, CC BY 4.0) holds 3.5 GB of variants; the app needs only the files for one variant, a few hundred MB (exact size checked live during the build).
* The simulator has no microphone. Recording is tested there by feeding a spoken test file (made up text, generated with the Mac's own voice), and on your phone with the microphone.
* The desktop transcribes in pieces between pauses; the phone can do the same with the same model family.

### Decisions for you at this gate
* **PI7. Live transcription.** Recommended: cut the audio at pauses with voice activity detection and transcribe each piece with Parakeet v3, so a line appears a second or two after each sentence, in any of its 26 languages. Alternative: Parakeet EOU word by word, English only.
* **PI8. The model download.** Recommended: only the needed files, from a pinned revision, each checked by SHA-256 (as on the desktop), with the size shown and a yes asked first; on mobile data, ask again. Alternative: let FluidAudio download its default set itself (no pinning).
* **PI9. The audio file.** Recommended: AAC in `.m4a`, mono, kept in the app's own storage beside the meeting (audio never goes to the vault); transcript only writes no audio file at all, ever (AC-IOS-6).
* **PI10. Where recordings go in I2.** Recommended: as notes and sidecars in the vault format in the app's own folder, written as the recording goes (so a crash loses seconds, F-IOS-REC-5); I3 moves them into the iCloud vault with the retention rules.
* **PI11. The concept's extras.** Recommended: pause and the bookmark button (a bookmark adds a line with the time to your notes); no "Cloud (API)" choice, since the spec keeps cloud off and there is no provider yet; Parakeet v3 rather than v2 for its languages.
* **PI12. Importing a recording** (F-IOS-REC-4). Recommended: in I2, from Files or Voice Memos, through the same transcription path; it also makes the simulator tests real.

### Order of work
* **I2.1** model download with pins and the Settings model row; **I2.2** the recorder (microphone, background audio, pause, bookmark, saving as it goes, transcript only); **I2.3** live transcription with voice activity detection and Parakeet v3, and import; **I2.4** the consent reminder (the desktop's wording) and the recording screen from the concept; **I2.5** verify, review, and a build on your phone.

### Your checklist after I2 (needs your phone and this Mac)
* Install from this Mac, download the model on Wi-Fi, record a short test with the screen locked for a while, and check the lines appear and the recording keeps going; try transcript only and check Settings shows no audio for it; import a voice memo.

### Risks
* The Neural Engine shares heat and battery with the rest of the phone; the hour-long battery test is I5.
* iOS may stop background audio if another app takes the microphone (a phone call); the recording then saves what it has and says so.

## I3. Retention and the vault writer

Status: approved by the owner on 2026-11-02, with PI13 to PI17 as recommended. Covers F-IOS-PRIV-1, F-IOS-VAULT-3 and 5, AC-IOS-5 and 8. Branch `enhance/quire-ios-i3`. Same rules: tests first, one commit per work package, independent review.

### What the research found
* The desktop deletes only files it wrote itself (its own records of each path), so notes the phone adds to the vault are never removed or changed by the desktop (`sync-format.md` section 2, rule 5; checked in `vault/export.rs`).
* **Heads-up: the desktop does not read the vault yet.** Its export is one way until desktop Phase F. So in I3, meetings recorded on the phone appear in the iCloud vault and on the phone, but not in the desktop app. They show on the desktop once Phase F imports from the vault.
* Retention is already fully specified: the six values and their meaning (spec F-RET-1 to 13), the fields in the note, expiry computed by every device and never written back, the mass-expiry guard and the clock-going-back pause (`sync-format.md` section 10). The phone implements the same rules; nothing new is invented.
* With audio sync off (the only setting in version 1), the phone's audio lives only in the app's own storage. The sweep deletes the phone's own copy, and any `_audio/<id>.*` copy it finds in the vault for an expired note (section 10, rule 3).
* iOS gives no timer guarantee. A background refresh task runs when iOS decides, so the sweep also runs at launch and on return to the foreground, and the settings text says a phone that is not used cannot delete anything (rule 6).
* Writes into iCloud Drive go through `NSFileCoordinator`, sidecar first, then the note, with the staging file in the app's own storage so it is never uploaded (section 15).

### Decisions for you at this gate
* **PI13. When phone meetings move into the vault.** Recommended: when a recording finishes (and at launch for any finished one still waiting), the note and sidecar are written to the vault, read back with the vault reader to confirm, and only then removed from the app's own folder. Recordings stay local while in progress, or when no vault is chosen, and move later. Alternative: keep them local until you press "Move to vault".
* **PI14. Where in the vault.** Recommended: unfiled at the vault root by default, with an optional choice of an existing desktop folder on the record sheet (a note belongs to the folder it sits in, so no folder file is changed). Alternative: unfiled only; folders wait for desktop Phase F.
* **PI15. Retention on the phone.** Recommended: the desktop's six values (Off, 24 hours, 7, 14, 30 days, Keep) in Settings, default 7 days as on the desktop, applying to new recordings; each phone meeting shows its audio status in words and offers "Keep this one" and "Delete audio now". Off deletes the phone's audio as soon as the transcript is confirmed in the vault (or saved locally when there is no vault). Transcript only stays "No audio saved".
* **PI16. Playback.** Recommended: simple playback for phone meetings while their audio is kept (play and pause, a scrubber, speed), as in the concept; "Audio deleted on <date>" once it is gone. Alternative: no playback until I5.
* **PI17. When the sweep runs.** Recommended: at launch, on return to the foreground, and in a background refresh task, with the mass-expiry guard (more than 20 recordings or a quarter of them asks first) and the pause when the clock reads earlier than the last run.

### Order of work
* **I3.1** retention rules in RecordingKit (deadlines, effective state, the guard, the clock check) with an injected clock, mirroring the desktop's AC-RET cases (AC-IOS-5); **I3.2** the vault writer: coordinated, sidecar then note, read back, then the local copy removed; a later change to a phone note is written only if the vault copy is still the one the phone last wrote, otherwise the person is told (AC-IOS-8); **I3.3** the sweep at launch, foreground and in the background, with verified deletion and the guard prompt; **I3.4** Settings (retention and its plain-language limits), the record sheet's folder choice, audio status, "Keep this one", "Delete audio now" and playback on the meeting screen; **I3.5** verify, review, and a build on your phone.

### Your checklist after I3 (needs your phone and this Mac)
* Record a short test with Off and one with 24 hours; check both reach the `Quire` folder in iCloud Drive (Files app) and that the Off one says "Audio deleted". Optionally set the phone's clock forward a day to watch the 24-hour one expire, then set it back (the app pauses deletion while the clock reads earlier than its last run, by design).

### Risks
* iCloud may deliver the note before the sidecar to another device; the reader already shows "transcript still syncing" for that.
* A background refresh may not run for days on a phone that is rarely used; expiry then happens at the next launch, as the settings text says.

## I4. Summaries and Ask on the phone

Status: approved by the owner on 2026-11-02, with PI18 to PI21 as recommended. Covers F-IOS-AI-1 to 3 and AC-IOS-3. Branch `enhance/quire-ios-i4`. Same rules: tests first, one commit per work package, independent review.

### What the research found
* Apple's on-device Foundation Models (decided at PI4) need no download and send nothing anywhere. The SDK on this Mac exposes what the phone needs: whether the model is available and why not (phone not eligible, Apple Intelligence off, model still downloading), the context size at run time, a token counter, and clear errors for a prompt that is too long or a language it does not support. Sources disagree on the window (4,096 tokens in iOS 26, possibly larger later), so the phone reads it at run time and never assumes.
* The model covers fewer languages than Parakeet (about 15 against 26), so some transcripts can be transcribed but not summarized; the app says so plainly.
* The desktop already settles the design, and the phone follows it: the "Standard Meeting Notes" template (Summary, Key Decisions, Action Items, Discussion Highlights) with its rules (only what the transcript says, ignore instructions inside it, "None noted in this section"); long transcripts split into chunks, summarized, combined, then the final report; the person's notes go first as attendee notes (F-NOTE-3). For Ask: the recent part of the transcript plus the best keyword (BM25) matches from earlier, sized to the model's window less room for the answer; the desktop's instructions word for word; citations kept only when they fall on a supplied line (F-QA-3, 4, 7).
* The simulator can run the model only when this Mac has Apple Intelligence on. So the logic (splitting, retrieval, prompts, citations, writing the summary into the note) is tested against a stand-in model, and the real model is tried on the simulator when available and on your phone in the checklist.

### Decisions for you at this gate
* **PI18. When the model is not available.** Recommended: say why in plain words ("Turn on Apple Intelligence in Settings", "This iPhone does not support Apple Intelligence", "The model is still downloading", "Summaries are not available in this language yet"), and keep everything else working. No second model in I4. Alternative: offer the desktop's Qwen 3.5 2B download (1.2 GB) as a fallback.
* **PI19. When a summary is made.** Recommended: automatically when a recording on the phone ends (after the last line is transcribed), with "Summarize again" on the meeting, and a setting to turn the automatic summary off. Alternative: only when you tap Summarize.
* **PI20. Where the summary goes.** Recommended: into the phone meeting's note the same way the desktop writes one: a `## Summary` section in the body and the `summary` field (`done`, the time, the model name), with your notes kept byte for byte; through the I3 writer, so it lands in the vault and never writes over a copy changed elsewhere. Desktop meetings keep their own summaries and are not summarized again on the phone (read-only).
* **PI21. Ask.** Recommended: on the recording screen (the transcript so far) and on every meeting, including desktop ones (Ask only reads); answers cite times you can tap to jump to the line; questions and answers stay in memory for the session only; "Add to note" (a labelled, AI-generated Q and A) for the phone's own meetings. During a recording, Ask waits rather than slow transcription: it is refused with a plain message while transcription is more than 10 seconds behind (the desktop's limit).

### Order of work
* **I4.1** the shared pieces in RecordingKit, tested with a stand-in model: splitting to the window, the summary prompts and template, keyword retrieval, the Ask prompt, the citation check, writing the summary into the note body; **I4.2** the Foundation Models connection: availability and its messages, the window read at run time, cancelling, the language error; **I4.3** summaries: automatic after a recording, Summarize again, the setting, the Summary tab filled on the phone; **I4.4** Ask on the recording screen and the meeting screen, tappable citations, Add to note; **I4.5** verify (including a test that no network request is made during summary or Ask, AC-IOS-3), review, and a build on your phone.

### Your checklist after I4 (needs your phone and this Mac)
* With Apple Intelligence on, record a short made-up conversation, wait for the summary, ask one question and tap its time; then turn Apple Intelligence off and check the app says so instead of failing.

### Risks
* A long meeting means many model calls on a 4,096-token window; a one-hour summary may take a few minutes and warm the phone. Measured in I5 with the battery pass.
* The model may refuse some content (its safety rules); the meeting then shows "The summary could not be made" with the reason, and the transcript is unaffected.

## I5. Device pass, accessibility and the I2 gaps

Status: approved by the owner on 2026-11-03, with PI22 to PI25 as recommended. Covers F-IOS-UI-1, NF-IOS-1, AC-IOS-4, 9, 10, and closes the two I2 gaps in the deviations table. Branch `enhance/quire-ios-i5`. Same rules. The owner is away from this Mac for two weeks: everything below except the battery and lock-screen measurements is done and tested here; those two run on the owner's phone through TestFlight.

### What the research found
* iOS reports the phone's heat as nominal, fair, serious or critical, and announces changes. NF-IOS-1 says the phone must never reach "serious" in a one-hour recording; the app can watch for it and react, but only the phone can show whether it gets there.
* An AAC file in an `.m4a` container is unplayable until it is closed (the I2 gap). The same AAC audio in Apple's `.caf` container stays readable up to the last piece written, at the same size, and `caf` is already one of the vault format's audio types (sync-format.md section 10).
* The desktop's "Read more" consent page is plain guidance plus two sample lines to copy (one to say aloud, one for an invitation); the phone has neither yet (the I2 gap).
* The desktop checks every text and background pair against WCAG AA in both themes from its tokens; the phone uses the same token values, so the same pairs can be checked in a test. Dynamic Type and VoiceOver labels can be checked on the simulator at the largest text sizes.

### Decisions for you at this gate
* **PI22. Heat.** Recommended: during a recording, at "serious" the app keeps recording audio but pauses transcribing and summaries until the phone cools (the lines catch up afterwards) and says so on screen; at "critical" it stops and saves, as for a phone call. Alternative: only measure, change nothing.
* **PI23. Audio that survives a crash.** Recommended: record into AAC in `.caf` instead of `.m4a`, so a crash or a killed app keeps the audio up to the last second, not only the transcript; nothing to convert later. Recordings already made keep their `.m4a`.
* **PI24. The consent page.** Recommended: the desktop's "Read more" page on the phone, word for word, with the two sample lines to copy, opened from the reminder and from Settings.
* **PI25. Accessibility.** Recommended: a test that checks the phone's colour pairs against WCAG AA in light and dark (the desktop's pairs), a UI test that every button and control on each screen has a spoken label, screens checked at the largest accessibility text size, and fixes for whatever these find.

### Order of work
* **I5.1** heat watch with a test using a simulated heat state; **I5.2** recording into `.caf` with a test that reads a file cut off mid-recording; **I5.3** the consent page; **I5.4** the accessibility checks and fixes, in light and dark, at large text; **I5.5** review, then a TestFlight build when the App Store Connect steps are done.

### Your checklist after I5 (on your phone, through TestFlight)
* Record a one-hour made-up talk (a podcast playing nearby is enough) with the screen locked, and note the battery used and whether the phone warmed; check the transcript kept coming (AC-IOS-4, AC-IOS-9). Then the I2 to I4 checklists above.

### Risks
* The battery figure depends on the phone model and its battery health; the plan records what your phone measured.

## TestFlight (the owner's request, 2026-11-02)

The owner is away from this Mac for two weeks, so the phone checks for I2 to I4 move to TestFlight instead of a cable install. This replaces PI6 (free signing) now that TestFlight is wanted, as PI6 foresaw.

* **What is ready:** a Release build for real iPhones archives cleanly (16 MB; the transcription model downloads after install, as in PI8); the app icon, made from the brand tile icon by `ios/scripts/make-app-icon.py`; `ios/scripts/testflight.sh`, which signs automatically with an App Store Connect API key and uploads for internal testing only. Every account detail lives in `~/.config/quire/testflight.env` and a key file outside the repository; the script refuses a key inside it.
* **What only the owner can do** (paid, or tied to the owner's Apple account): join the Apple Developer Program (99 US dollars a year; the Apple Developer app on iPhone can do it; approval can take up to 48 hours); create the app record in App Store Connect; create an App Store Connect API key and put its file where this Mac can read it (iCloud Drive), with the key ID, issuer ID and team ID.
* **Then, from this Mac:** the key file is moved out of iCloud Drive into private storage, the script uploads, and the owner installs from the TestFlight app and runs the I2 to I4 checklists on the phone.
* Internal testing needs no Apple review. External testers (anyone outside the developer account) would need a short review and a privacy policy page, and are not planned.

## Your checklist after I1 (needs your phone and a Mac)
* Connect the iPhone to this Mac once, trust it, and sign in to Xcode with your Apple ID; I install the app. Open it, choose the `Quire` vault folder in iCloud Drive, read the disclosure, and check that your desktop meetings, folders and people appear, read-only.

## Risks
* iCloud folders on the phone may hold placeholders that download on demand; the reader shows "waiting for iCloud" rather than treating a file as missing (`sync-format.md` section 15).
* Foundation Models availability depends on the phone and on Apple Intelligence being on; the app says plainly when it is not available.
* Free signing expires every 7 days; the app keeps its data, but it must be reinstalled from the Mac.

## Deviations and review record

| Step | Plan said | What was done | Why |
|---|---|---|---|
| I1 | A small local cache for search (PI5) | No cache yet: the vault is read into memory on each open and refresh | A personal vault reads in well under a second; search comes with recording (I2) |
| I1 | n/a | The Xcode project is generated from `ios/project.yml` by XcodeGen and not committed; the signing team lives only in an untracked `ios/Local.xcconfig` | No generated churn, and no account detail in the repository |
| I1 review | n/a | Independent review: one blocker and seven important findings, fixed with tests. Blocker: a hostile number in a vault file could crash the app on every launch (numbers are now checked in the reader and the display never traps). Important: the grammar works on Unicode scalars and keeps a leading byte order mark, exactly like the desktop; JSON is read strictly (exact types, UTF-8 only, duplicate keys refused); a transcript hash is checked before any path is built from it; refreshing while iCloud downloads keeps the screen and backs off; files iCloud has not downloaded are recognised by their download status; notes without headings or with CRLF show; a newer vault opens read-only with a notice. Also: files are opened without following links and read with a bound, nesting is capped, duplicate IDs are dropped, a half-written marker means try again, the bookmark is released only when it was taken, and the Settings wording no longer contradicts the iCloud disclosure. Deferred to I3, when the phone writes: the sidecar number layout for exponent values (serde writes `1e16`, Swift `1e+16`); recording times never reach them | Review against `spec-ios.md` and `sync-format.md` |
| I2.3 | Pieces cut at pauses by voice activity detection (Silero, PI7) | Pauses are found by loudness against a noise floor that adapts (a 0.6 s pause after at least 1 s of speech, and a cut at 14 s in a long monologue), in RecordingKit with its own tests | No second model to download and pin; the cut points come out within a fraction of a second either way, and Silero can replace it later behind the same interface if real meetings need it |
| I2.1 | n/a | The model files are downloaded by the app itself from the pinned revision and loaded with FluidAudio's `loadLocal`, so FluidAudio never downloads anything of its own | Keeps the pin and the file-by-file SHA-256 check (PI8) in one place |
| I2 | n/a | The transcription test needs the model on this Mac and is skipped without it (`TEST_RUNNER_QUIRE_MODEL_DIR`); the UI tests use the screens with a stand-in model folder | 483 MB stays out of the repository; no path from this Mac is committed |
| I2.4 | The consent reminder as on the desktop | Same wording, close, "Never show this again" and "Got it"; no "Read more" yet | The desktop's "Read more" opens its own consent page, which the phone does not have; it comes with that page in I5 |
| I2.2 | Saving as it goes (F-IOS-REC-5) | The transcript and notes are saved after every line; the `.m4a` audio file is only complete when the recording stops, so a crash keeps the transcript but may leave that recording's audio unplayable | AAC needs its file closed; writing raw audio and converting at the end doubles the storage while recording. Revisit with the battery and heat pass in I5 |
| I2 review | n/a | Independent review: five important and seven minor findings, fixed with tests. Important: the old sidecar is now removed only after the new note is saved (a failed write keeps the previous pair readable); imports are `source: imported` with no recording times; cutting an hour-long import is linear and runs away from the main thread; a headset connected or removed mid-recording stops and saves with a plain note instead of going silent, and the observers are removed; finishing the last lines runs as a background task and releases the microphone only after the save. Minor: buffers arriving as pause is pressed still count; bookmarks and the duration use the exact recorded time; a piece that fails to transcribe is tried again and otherwise named in a note; save errors are shown, not hidden; closing the screen while the model loads leaves no empty meeting and no microphone in use; the unused `skip` was removed with its test. The UI tests run the screens with a stand-in model, so the reminder is checked over a start that fails; the recorder itself is checked by the transcription and import tests and on the phone | Review against `spec-ios.md`, `plan-ios.md` and `sync-format.md` |
| I3 | n/a | The UI tests open a temporary copy of the shared fixture vault, never the repository's own | From I3 the app writes its recordings into the vault it opens |
| I3 review | n/a | Independent review: four important and five minor findings, fixed with tests. Important: Off audio waits whenever a vault is chosen, also while it cannot be reached (a bookmark that does not resolve no longer counts as no vault); while the phone still has its own copy that copy decides, and Off counts as confirmed only once the phone has written and read back the note (its base), so an older copy or someone else's note with the same ID never triggers deletion; the phone replaces an older copy it wrote itself after a move cut short; housekeeping and "Keep this one" or "Delete audio now" run one at a time, and a local change during a move keeps the local copy for the next move; a yes to the mass-expiry question deletes exactly the recordings it asked about, as on the desktop. Minor: no deadline is set from a clock that went back; the base is saved before the local copy is removed, and a lost base is recovered only for a note this iPhone wrote last; the background task stops cleanly when iOS ends it; audio still on the phone shows "deletion pending" rather than "deleted"; the tests keep their own records instead of the app's | Review against `spec-ios.md`, `plan-ios.md`, `sync-format.md` and the desktop's `retention/policy.rs` |
| I4.3, I4.4 | One commit per work package | Summaries and Ask share the meeting screen, the in-memory history and one file, so they landed in one commit | They could not be built or tested apart |
| I4 | n/a | Summaries and answers are drawn by a small block renderer (headings, lists, table rows as plain rows) instead of inline Markdown only; links never live, images and HTML never drawn | The template's headings and Action Items table showed as raw Markdown |
| I4 | n/a | Times inside a summary are the model's own and are not checked, as on the desktop; only Ask answers have their citations checked (F-QA-4). On the simulator the model once gave a task the time of the line before it | A summary is prose, not a set of citations; checking it would need a new requirement |
| I4 | The desktop's summary pipeline | Without the desktop's second pass that rewrites a non-English summary into English: the phone has no language detection, and the model is told to write English, as in the desktop's first pass. Notes are cut to a third of the room left for the final report, and a combined summary that still does not fit is cut at a word boundary | The model's window is small (4,096 tokens on iOS 26); these keep every call inside it |
| I4 | AC-IOS-3, a network test | The test counts the app's own requests (the URL loading system); Apple's model runs in the system, outside the app, and Apple states it runs on the device | The app cannot observe the system's traffic from a test |
| I4 review | n/a | Independent review: four important and eight minor findings, fixed with tests. Important: Summarize again removes the whole earlier summary (up to `## Notes`), and only a summary at the top of the body counts as the app's, so a "## Summary" line in the notes stays; `## Summary` and `## Notes` lines in model output (summary or Add to note) are demoted, so a transcript cannot move the sections; the summary's background time ends cleanly when iOS takes it back; only summaries this iPhone made say "on this iPhone", a desktop one names its model. Minor: the combine step uses the desktop's separator and the final instructions match the desktop's text exactly (a test pins them); the final report and Ask retry smaller when the model still finds a prompt too long; condensing stops after three rounds or when it stops shrinking; Windows line endings and a missing leading line break are handled; tokens are counted as the desktop counts them; the test-only switches exist only in debug builds; Ask's Stop survives leaving the screen, Ask during a recording includes the bookmarks, and the backlog counts the piece being transcribed; the stand-in model now leaves room for the answer, and the screen test checks the jump whenever the model cites (it is asked to cite, not forced to, as on the desktop; one run answered correctly without a time) | Review against `plan-ios.md`, `spec-ios.md`, `sync-format.md` and the desktop's summary and Ask code |
| I5.2 | AAC in `.caf` (PI23) | AAC in ADTS framing (`.aac`) | Tested on this Mac: a `.caf` file cut off mid-recording reads back zero seconds (its packet index is written only on close) and an `.m4a` does not open at all, while `.aac` reads back 19.97 of 20 seconds; same audio, same size. `aac` is not in the vault format's audio list, which is fine because phone audio never enters the vault in version 1: the sweep lists it for the phone's own folder only. Recordings made before keep their `.m4a` |
| I5.1 to I5.3 | One commit per work package | One commit | The heat watch, the new audio file and the reminder's "Read more" all change the recorder and its screen |
| I5.3 | The desktop's consent page word for word | Word for word, except "on your computer" reads "on your iPhone"; the US guide opens in Safari | The page is about this device |
| I5.4 | Fixes for whatever the checks find | Found and fixed: the fourth speaker colour failed AA behind its initial (3.7:1), so the phone now uses the desktop's eight speaker colours, each a soft fill with its own ink; the serif titles did not grow with the text size; the consent reminder cut off its text at large sizes (it now scrolls); the Ask question field had no spoken label and was squeezed at large sizes (it now stacks); some links and close buttons used the system blue instead of the app's terracotta. A test pins the phone's colours to the desktop's `tokens.css` | AC-IOS-10, F-IOS-UI-1 |
| I5 review | n/a | Independent review: three important and six minor findings, fixed with tests. Important: speech held while the phone is warm is kept as 16-bit samples (half the memory) and a hold past half an hour stops and saves the recording instead of growing; at "critical" the rest of the transcript waits for the phone to cool, also after the recording stops, and the heat is watched until the transcript is finished; the heat note has its own line and never erases a save or transcription warning. Minor: a summary on a warm phone waits and starts by itself when it cools; the warm note clears once stopped; tests with a stand-in transcriber prove pieces wait and then finish; the serif titles use text styles, so they follow text-size changes while the app is open; the label check scrolls each screen to the bottom; the colour test covers warnings and the sage label on cards and grouped rows, and fails if a token is missing from `tokens.css`. Known gap: if iOS ends the app during a long hold, the audio of that stretch is kept but its lines are not; they are not recovered from the audio file in this version | Review against `plan-ios.md`, `spec-ios.md` and the desktop's consent page and tokens |
| After I5 | n/a | The owner's requests (2026-11-03): one-tap Copy on Transcript, Summary, Notes and every Ask answer (the transcript as `[mm:ss] Speaker: text` lines), and a closer match to the concept: the record button's ring, folder cards, a tinted tab control, solid speaker circles (the desktop's speaker inks, which pass AA under a light initial), the player with a large play button, Ask in a sheet from the toolbar, the recording breadcrumb and landscapes. Return now asks the question in Ask | Owner feedback on the screenshots |
