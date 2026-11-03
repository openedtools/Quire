# Spec: Quire for iPhone (draft 1)

Status: accepted by the owner on 2026-11-02, with `plan-ios.md`. Derived from `intent.md`, which is iPhone-first. The desktop spec (`spec.md`) does not cover the phone; `sync-format.md` is the contract between the two apps. Versions and library facts marked "check live" are verified during the build, not assumed.

## 0. Scope

In: a native iPhone app in Swift, distributed to the owner's own phone first (TestFlight later), with the same privacy behaviour as the desktop: on-device transcription, retention, transcript-only, the consent reminder, summaries and Ask on the device, cloud off until switched on, and the shared vault in iCloud Drive.

Out (v1, from intent.md): Android, phone call audio (iOS does not allow it), a server the owner runs, calendar and CRM integrations, App Store release, iPad and Mac layouts.

## 1. Requirements

### 1.1 Recording and transcription (F-IOS-REC)
1. **F-IOS-REC-1 (M)** Record from the microphone with a live transcript. Recording continues with the screen locked (the background audio mode).
2. **F-IOS-REC-2 (M)** Transcription runs on the device with Parakeet v3 through FluidAudio (check live: version, licence, iOS minimum). Nothing is sent anywhere.
3. **F-IOS-REC-3 (S)** Speaker separation on the device (FluidAudio diarization), shown as Speaker 1, Speaker 2; names come from the vault's people.
4. **F-IOS-REC-4 (S)** Import an audio file (a speakerphone or voice memo recording), since phone calls cannot be captured.
5. **F-IOS-REC-5 (M)** A crash or a killed app never loses more than the last few seconds: the transcript is saved as it is made.

### 1.2 Privacy (F-IOS-PRIV), same rules as the desktop
1. **F-IOS-PRIV-1 (M)** Audio retention per recording: Off, 24 hours, 7, 14, 30 days, Keep (the desktop's F-RET values and meaning). The sweep runs at launch, on return to the foreground and in a background task; iOS gives no timer guarantee and the settings text says so.
2. **F-IOS-PRIV-2 (M)** Transcript only: no audio file is ever written.
3. **F-IOS-PRIV-3 (M)** The consent reminder when a recording starts, dismissible, with "Never show this again" and the same plain-language guidance and wording rules as the desktop (spec 3.5).
4. **F-IOS-PRIV-4 (M)** Cloud off by default: no network request except a model download the person starts. A cloud summary provider, when added, sits behind the same "Allow sending data to cloud services" switch.
5. **F-IOS-PRIV-5 (M)** API keys, if any, live in the iOS Keychain, never in files or the vault.

### 1.3 Summaries and Ask (F-IOS-AI)
1. **F-IOS-AI-1 (M)** A summary after the meeting, made on the device.
2. **F-IOS-AI-2 (M)** Ask a question during or after a meeting and get an answer from the transcript so far, citing times.
3. **F-IOS-AI-3 (M)** The model choice follows plan decision PI4.

### 1.4 The vault (F-IOS-VAULT)
1. **F-IOS-VAULT-1 (M)** The person picks the vault folder in iCloud Drive (the one the desktop writes); the app keeps access with a security-scoped bookmark. Nothing is written until they confirm, with the same iCloud disclosure as the desktop (F-SYNC-2).
2. **F-IOS-VAULT-2 (M)** Read the vault exactly as `sync-format.md` says: the strict grammar, untrusted input (section 14), the shared fixtures in `docs/vault-fixtures` all pass, unknown keys preserved.
3. **F-IOS-VAULT-3 (M)** Meetings recorded on the phone are written to the vault in the same format, with the phone's own device ID.
4. **F-IOS-VAULT-4 (M)** Editing a meeting that came from the desktop, and two-way conflict handling, wait for desktop Phase F (the device checks in `sync-format.md` section 15). Until then desktop meetings are read-only on the phone, and the phone only adds its own.
5. **F-IOS-VAULT-5 (M)** Audio never enters the vault in v1 (audio sync stays off).

### 1.5 Look and feel (F-IOS-UI)
1. **F-IOS-UI-1 (M)** The desktop's paper look: cream and terracotta with the sage "on this device" accent, light and dark, the same tokens and contrast rules (desktop spec 3.2, WCAG 2.2 AA), Dynamic Type and VoiceOver labels on every control.
2. **F-IOS-UI-2 (M)** Screens: Meetings (folders), Record, Meeting (transcript, summary, notes, Ask), People, Settings (models, retention, privacy, vault).

### 1.6 Non-functional
1. **NF-IOS-1** Battery and heat: a 60-minute live transcription with the screen off uses no more than 25 percent battery on a current iPhone, and the phone never reaches thermal "serious" (the starting thresholds in `spec.md` section 8, measured in the plan).
2. **NF-IOS-2** No personal data in the repository: no real meetings, names, device IDs, Apple team IDs or provisioning profiles; the signing team lives in an untracked local file.

## 2. Acceptance criteria

| ID | Criterion | How | Pass |
|---|---|---|---|
| AC-IOS-1 | Vault conformance | Unit tests over `docs/vault-fixtures` | Every canonical file reads and writes back byte for byte; every grammar fixture is rejected; folder names match `names.json` |
| AC-IOS-2 | Desktop vault | UI test with a generated vault | Meetings, folders, people, transcripts and summaries from the desktop show correctly, read-only |
| AC-IOS-3 | Offline | Network test with cloud off | No request during recording, transcription, summary or Ask |
| AC-IOS-4 | Lock screen | Device test | A 10-minute recording with the screen locked keeps transcribing |
| AC-IOS-5 | Retention | Unit tests with an injected clock | The desktop's AC-RET rules hold (deadlines, mass-expiry guard, clock going back) |
| AC-IOS-6 | Transcript only | Test | No audio file exists at any point |
| AC-IOS-7 | Consent | UI test | The reminder shows at start, never claims compliance, "Never show this again" sticks |
| AC-IOS-8 | Phone meetings in the vault | Test | A phone recording becomes a note and sidecar the desktop's reader accepts |
| AC-IOS-9 | Battery | Device measurement | NF-IOS-1 thresholds met, or the plan records the gap |
| AC-IOS-10 | Accessibility | UI review and contrast check | Every control has a label; text pairs pass AA in both themes |
