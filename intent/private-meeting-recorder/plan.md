# Plan: Quire desktop, Phase A (from spec.md, accepted 2026-11-01)
Status: approved by the owner on 2026-11-01 at the plan gate. P1 (identifier `com.openedtools.notes`), P2 (branch `enhance/quire-phase-a`) and P3 (classify the failing tests, fix causes, delete only tests of removed code) accepted as recommended. The spec amendments below are applied to `spec.md` section 11.


## Context

The Stage 2 spec was accepted on 2026-11-01. This plan starts Stage 3 (Build). The spec has six phases (A to F). This plan details **Phase A: identity, safety, clean-up and re-skin**. Phases B to F get their own plan sections, written and approved when each starts, so that each phase stays a separate gate (spec section 9).

Phase A is the largest phase and the one that changes the most files, but it adds no new user feature. It makes the app a separate product (own name, own data folder, no upstream scaffolding) and gives it the paper look. Everything later builds on it.

Plan-mode exploration found facts that change the spec's assumptions. They are listed under "Spec amendments" and will be applied to `spec.md` in the same commit as `plan.md`.

## Decisions for you at this gate

* **P1 Bundle identifier: `com.openedtools.notes`.** It is deliberately not derived from the app name. The identifier names the data folder, so if Quire later becomes Marginalia, a name-derived identifier would force a second data move. With this one, a rename only changes display names.
* **P2 Branch:** work on `enhance/quire-phase-a`, created from the current `feature/speakers-folders-people` head, so the speaker features stay a clean base. One commit per work package. No pull request unless you ask.
* **P3 Pre-existing failing tests.** 19 of 45 frontend tests already fail on the current branch (details below). Recommended: classify them in WP1, delete only tests of code we remove, and bring every other failing test to green by fixing the cause. You see the list at checkpoint A1.

## Baseline recorded during planning

| Check | Result now |
|---|---|
| `npx tsc --noEmit` (frontend) | Clean, 7.5 s |
| `bun test` (frontend) | 26 pass, **19 fail**, 45 total. Failing files: `tests/lib/analytics.test.ts`, `tests/hooks/summary-generation.test.tsx`, `tests/hooks/meeting-details-refresh.test.tsx`, `tests/lib/summary-language-preferences.test.js` (a "Consumer failed" error and mock failures). Whether they also fail on the upstream base commit is unknown. |
| `pnpm lint` | Does not work. `eslint` and `eslint-config-next` are not installed. |
| Rust tests | About 330 inline tests (245 `#[test]`, 85 `#[tokio::test]`). WP1 records the passing count with the known environment recipe (system ONNX Runtime, stub `llama-helper`). |
| CI | None runs tests. All 8 workflows are manual builds. |
| Rust dialog plugin | `tauri-plugin-dialog 2.3.0` already in `Cargo.toml`. |

## WP1 baseline results (recorded 2026-11-01)

| Check | Result |
|---|---|
| `cargo test --lib` (frontend/src-tauri) | **322 passed, 0 failed, 4 ignored**, exit 0. Two ignored tests need the speaker model and recordings, two need audio hardware or files. |
| `bun test` (plain) | 26 pass, 19 fail. The same 19 fail at upstream's release commit `a2cb62e`, so they predate this fork. |
| Cause of the 19 | Not code defects. `mock.module` leaks between test files in one bun process. Every file passes alone (45 of 45). |
| Fix | `pnpm test` runs `bun test --isolate` (fresh global per file) plus `node --test` for the one node test file. No test was edited or removed. 45 + 1 pass. |
| `tsc --noEmit` | Clean. |

P3 therefore needed no test deletions. The two tests in `tests/lib/analytics.test.ts` go away in WP4 together with the module they test.

## Spec amendments found while planning

1. **F-HK-6:** `docs/ARCHITECTURE.md` would collide with the existing `docs/architecture.md` on case-insensitive Windows. Upstream's long CLAUDE.md moves to `docs/UPSTREAM_NOTES.md` instead. Its link to the missing `AUDIO_MODULARIZATION_PLAN.md` is dropped.
2. **AC-HK-6 hook:** the hook locks existing tests when `LOCK_TESTS=1` is set (for bug-fix tasks), not when `ALLOW_TEST_EDITS` is unset. The spec's wording would block writing new tests during feature work.
3. **F-UI-4 fonts:** bundled through pinned `@fontsource-variable/source-serif-4` and `@fontsource-variable/source-sans-3` packages (both OFL). The files ship in the build and no network is needed at build or run time after install. BlockNote's own Inter font import is overridden.
4. **Token name:** the spec token `muted` is `ink-muted` in CSS and Tailwind, because shadcn already uses `muted` for a background.
5. **F-ID-4 adoption** is more than a data-folder copy. A new identifier looks like a first launch, so onboarding would run and the models (several GB) would be re-downloaded. Adoption therefore also copies the stores (`onboarding-status.json`, `recording_preferences.json`), copies model files (hard links would let one app truncate a file the other uses), reads the old database through `VACUUM INTO` (consistent even if the old app is open), and runs **before** any app state exists. Three data locations are hard-coded and do not follow the identifier: `Meetily/templates`, `meetily/notifications.json`, and `meetily-recordings`. All move into the app data folder (existing recordings keep their saved absolute path).
6. **F-HK-1:** also remove `test_backend_connection`, `debug_backend_connection`, `get_server_address`, the `localhost:5167` CSP entry and the sidebar's `serverAddress`, which only served the archived backend.
7. **F-HK-3:** the dead list is longer: `audio/stt.rs`, `audio/system_audio_stream.rs`, `audio/system_audio_types.ts`, `whisper_engine/_stderr_suppressor.rs`, `audio_v2/`, `CLEANUP_PLAN.md`, `build_backup.bat`, `app/metadata.ts` and `.tsx`, the empty `AudioPlayer.tsx`. Each is deleted only after `cargo check` and `tsc` confirm nothing references it. The new migration drops both `licensing` and `meeting_notes`. Old migrations are never edited (sqlx checks their checksums).
8. **About screen:** the upstream marketing block ("Chat with the team") and the analytics switch are removed, not restyled.
9. **Acceptance criteria by phase:** AC-HK-3 (network inventory) moves to Phase C and AC-HK-4 (logs) to Phase B. AC-NF-1 is split by phase. The mapping is at the end of this plan.
10. **Parakeet v3 download host:** the model comes from an upstream-hosted address (`meetily.towardsgeneralintelligence.com`). If that host disappears, onboarding breaks. This is recorded for Phase C (F-MOD-8), not acted on here.

## Order of work

Four checkpoints. After each one you can build the installer (`frontend\build_installer_windows.bat`) and try it. Every step leaves `cargo check --lib` and `tsc` green.

The order is chosen so that **no new build touches the data of the upstream app you use today**: the identifier switch and the database backup come first.

### A1. Safety and isolation

**WP1. Baseline and process** (no behaviour change)
* Record Rust and frontend baselines. Classify the 19 failing frontend tests by running them in a temporary worktree at the upstream base commit.
* Replace `CLAUDE.md` with a one-page version: build, test and lint commands, the Rust environment recipe, conventions, the audio-pipeline map, things Claude gets wrong, and a verify-before-done block. Move upstream's to `docs/UPSTREAM_NOTES.md`.
* Add `.claude/settings.json` and `.claude/hooks/protect-tests.sh` (blocks edits to existing test files when `LOCK_TESTS=1`, exits 2 with a reason, `onFailure: block`), and deny reads of `.env*`.
* Add scripts: `typecheck` (`tsc --noEmit`), and a `lint` runner that executes the custom checks as they land (dashes, brand, colours, contrast, brand drift). `lint:dashes` lands now with an allowlist that shrinks to empty in WP4.
* Files: `CLAUDE.md`, `docs/UPSTREAM_NOTES.md`, `.claude/*`, `frontend/package.json`, `frontend/scripts/lint/*`.

**WP2. Database backup before migrations** (F-HK-9). Tests first.
* In `DatabaseManager::new` (`src-tauri/src/database/manager.rs`), after the legacy copy or create step and before `sqlx::migrate!(...).run`: if `meeting_minutes.sqlite` exists and `_sqlx_migrations` is behind the newest embedded migration, copy the database (with its `-wal` and `-shm` files, before the pool opens) to `backups/meeting_minutes.<target version>.<utc>.bak`, then keep the newest 3.
* Idempotent: `new()` can run twice through the existing retry path, so a backup for the same target version is not repeated and never rotates out a good one. Do the backup before the retry code that deletes WAL files.
* Tests: none for a fresh database; none when up to date; created when behind; WAL included; second call no-op; rotation keeps 3.

**WP3. Brand config, identifier, data adoption** (F-ID-1 to F-ID-4, F-ID-9)
* `brand/brand.json`: `name`, `slug`, `bundleId` (P1), `windowTitle`, `tagline`, `vaultDirName`, `upstreamCredit`, and a `legacy` block (old identifier `com.meetily.ai`, old template and notification directories, old recordings folder name, old temp prefix).
* `frontend/scripts/brand-apply.mjs` writes `productName`, `mainBinaryName`, `identifier` and the window title into `tauri.conf.json` by targeted key edits (no reformatting), `frontend/src/lib/brand.generated.ts`, and `src-tauri/src/brand_generated.rs`. `brand:check` fails on drift. Tests with `bun:test`.
* Replace every user-visible upstream name (about 39 strings: `Logo.tsx`, `Info.tsx`, `LegacyDatabaseImport.tsx`, `notifications/types.rs`, `notifications/commands.rs`, `tray.rs`, two network `User-Agent` strings and the test that asserts one) with brand constants. The macOS `log stream --process meetily` commands use the brand slug.
* Legacy constants move into `legacy.rs` and `legacy.ts`. The `.meetily_decode_` prefix is created with a new prefix, and the deletion allowlist in `api/recording_files.rs` matches both. The IndexedDB name and the two storage keys are renamed (the new identifier has a fresh WebView profile, so the old ones are unreachable anyway).
* Path module: templates (`summary/templates/loader.rs`), notification settings (`notifications/settings.rs`) and the engine fallback paths move under `app_data_dir`.
* **Adoption** (`src-tauri/src/adopt.rs`), called at the top of `run()` before the Tauri builder, using `rfd` (already a transitive dependency of the dialog plugin) for one Copy or Start fresh prompt:
  1. Conditions: the old folder has `meeting_minutes.sqlite`, the new folder has none, and no marker file exists.
  2. Copy into a temporary sibling folder, then rename into place, so an interrupted copy leaves nothing half-made.
  3. Database through read-only `VACUUM INTO`, refused if it comes from a newer version than this build understands. Model files, stores and the legacy template and notification folders by copy (recordings are not copied; both apps keep using the same audio files, and the offer says so).
  4. Write an `.adopted` or `.declined` marker. The old folder is never modified or deleted.
* Tests first, in a temporary tree: copy correctness, source byte-identical afterwards, interrupted copy leaves no partial target, second run is a no-op, declined is remembered, pure decision function covers each condition.
* Not testable here (manual Windows checklist below): the real dialog, the old app running while adoption happens, and the WebView profile (not copied; the dialog says unsaved recovery data does not carry over).
* Files: `brand/*`, `frontend/scripts/brand-apply.mjs`, `tauri.conf.json`, `src-tauri/src/{adopt,legacy,brand_generated,lib,paths}.rs`, the string sites above.

**Checkpoint A1.** The owner builds and runs the Windows checklist. Success: the new app starts beside the old one, offers to bring the data over, shows the meetings, people and folders, skips onboarding, and the old app still works unchanged.

### A2. Clean-up (no visible change)

**WP4. Removals**, in this order, each its own commit:
1. Dead files (list in amendment 7). Delete, then `cargo check` and `tsc` decide.
2. Updater: `lib.rs` plugin lines for `updater` and `process`, both Cargo dependencies, the tray item `check_updates` and its handler, `tauri.conf.json` (`updater:default`, `process:default`, the plugin block), `UpdateDialog`, `UpdateNotification`, `updateService`, `useUpdateCheck`, `UpdateCheckProvider` (and its layout wiring), the About button, and both npm packages.
3. Pro and backend leftovers: the three profile commands with their structs and registration, `make_api_request`, the two backend-connection commands, `APP_SERVER_URL`, the CSP entry, and the sidebar `serverAddress`. Check `ModelSettingsModal.tsx` before removing the prop.
4. Telemetry in three stages so `tsc` guards each one: (a) replace `lib/analytics.ts` with a no-op stub; (b) remove the 94 call sites in 26 files, plus `AnalyticsProvider`, `AnalyticsConsentSwitch`, `AnalyticsDataModal` and the meeting-count helpers in `useRecordingStop.ts`; (c) delete the stub, the `analytics/` Rust module, its 25 registered commands, the `posthog-rs` dependency, the analytics block in `recording_commands.rs` (about lines 902 to 1002, with `classify_device_type`), and rewrite `PRIVACY_POLICY.md` to what the app actually does. Delete the analytics test file and fix the two tests that import it.
5. Migration `20261102000000_drop_unused_tables.sql`: `DROP TABLE IF EXISTS licensing; DROP TABLE IF EXISTS meeting_notes;`. The WP2 backup runs before it. Test that it applies after all earlier migrations (four existing tests already run the full chain in memory).
6. Em dashes (119 in 21 files): comments and log strings become ` - ` mechanically. The 6 user-visible strings, the 3 error strings and the 3 LLM prompt lines in `summary/processor.rs` are edited by hand, and the processor tests (30) must stay green. Then `lint:dashes` has an empty allowlist.
* Proof: `cargo test --lib` count equals baseline minus the 3 analytics tests plus new tests; `tsc` clean; frontend tests as classified in WP1.

**Checkpoint A2.** Build, behaviour unchanged. Settings, recording and summaries still work.

### A3. Look and feel

**WP5. Tokens, fonts, theme**
* `frontend/src/styles/tokens.css` (CSS variables for light and dark, the spec 3.2 values plus `sage`, `sage-tint`, `scrim`, `disabled`, hover and pressed variants) and `tokens.contrast.json` (each pair with its role). `globals.css` aliases the shadcn variables to the tokens. `tailwind.config.js` adds semantic colours (`paper`, `surface`, `line`, `line-strong`, `ink`, `ink-muted`, `accent`, `accent-strong`, `on-accent`, `sage`, `sage-tint`, `danger`, `success`, `warning`) and a serif family. Retoken the `.bn-*`, `.ProseMirror` and scrollbar rules in `globals.css`.
* Fonts via the two fontsource packages. Heading serif is a base style on `h1` to `h5` (74 headings), not per-component edits.
* **Theme provider (new, there is none today):** a small provider sets `.dark` on `<html>`, follows `prefers-color-scheme`, stores the override, and calls the Tauri window `setTheme` (adds `core:window:allow-set-theme`). Remove the forced `"theme": "Light"`. A Light, Dark, System control goes in the General settings tab. Wire BlockNote's `theme` prop (hard-coded `light` in `BlockNoteEditor/Editor.tsx` and `AISummary/BlockNoteSummaryView.tsx`) and the sonner toaster.
* `scripts/lint/contrast.mjs` reads the two token files and fails below role thresholds. `bun:test` unit tests pin known ratios (ink on paper 13.89, `accent-strong` on paper 5.11, and so on).

**WP6. Re-skin.** `scripts/codemod/palette.mjs` rewrites raw classes using a mapping table and prints a report of anything it could not map. The mapping, in principle:

| Raw class family | Becomes |
|---|---|
| `gray-50`, `gray-100` backgrounds | `surface` |
| `white` backgrounds | `paper` (cards) |
| `gray-200` borders and dividers | `line` |
| `gray-300` control borders | `line-strong` |
| `gray-600` to `gray-900` text | `ink` or `ink-muted` by shade |
| `blue-600` fills with white text | `accent-strong` with `on-accent` |
| `blue-50` to `blue-100` fills | `sage-tint` (info) or an accent tint |
| red, green, amber, yellow, orange | `danger`, `success`, `warning` |
| `bg-black/80` overlays (dialog, sheet) | `scrim` |

* Hand work the codemod cannot do: the 8 speaker colours in `lib/speakers.ts` (a designed set of speaker tokens with 4.5 text contrast), about 70 state-dependent colour ternaries, 10 gradients, 2 SVG strokes in `ModelDownloadProgress.tsx`, and the 5 `button.tsx` variants.
* One directory per commit, in this order: Sidebar and Logo (98), settings (`app/_components` 86, `app/settings`, preference components), People and Speakers (171), MeetingDetails and AISummary and the BlockNote views (98), onboarding (94), model managers and dialogs (the large top-level group, 714), `components/ui`. Then `lint:colors` turns on with an empty allowlist.
* Screens are verified with a repo-owned Playwright harness (`frontend/tests/ui/`, a mocked Tauri `invoke` like the one used for the earlier browser checks). It captures record, meeting, sidebar, people, five settings tabs, onboarding steps and the main dialogs, in light and dark at 1280 by 800. I review them against the spec tokens and the aesthetic reference, and the screenshots are attached to the pull request.
* Keyboard and target-size checks run in the same harness.

**WP7. Identity surfaces**
* About rewritten: wordmark, version from `getVersion()`, "Based on the upstream project, MIT licence", the full MIT text (bundled `LICENSE.md`), and a third-party notices page (`THIRD_PARTY_NOTICES.md`: Parakeet CC BY 4.0 attribution, WeSpeaker, Whisper, ONNX Runtime, llama.cpp, Qwen, Gemma terms, ffmpeg).
* `Logo.tsx` becomes a serif wordmark. Delete `public/logo.png` and `logo-collapsed.png` if unreferenced. Metadata and window title come from brand.
* Icons: render `icon-concept.svg` to 1024 px with Playwright, then `pnpm tauri icon` produces the full set (`.ico`, `.icns`, Store logos). Add `bundle.windows.nsis` with `installerIcon`, `sidebarImage` and `headerImage`, using plain on-brand BMPs generated from the SVG until you supply your own generated images (sizes are in spec section 3.9).
* Optional (F-UI-10, could): sidebar illustration if you provide it.

**Checkpoint A3.** Build and look. Review light and dark by eye.

### A4. Remaining Phase A items and close-out

**WP8. Recordings folder setting** (F-HK-7, should). Tests first.
* `recording_saver.rs` (`initialize_meeting_folder`) and `audio/import.rs` load the saved preference instead of calling `get_default_recordings_folder()`.
* Settings gets a folder picker (`select_recording_folder` is a stub today; use `tauri-plugin-dialog`). The picker warns when the path is inside OneDrive, iCloud Drive or Dropbox (detected from the known folder names under the user profile).
* New installs default to `<app data>/recordings`. Adopted or older installs keep their saved absolute path, because adoption copies `recording_preferences.json`.
* Deleting a meeting removes its audio. Recordings from an adopted install live in the earlier app's folder and are shared with it, so the new app must not delete files outside its own recordings folder without asking. Until then the adoption offer warns about it.
* Add the folder to the asset and fs scopes only if a later phase needs the frontend to read it (none today).

**WP9. Close-out**
* `scripts/check-traceability.mjs` (spec section 4), wired into `lint`.
* Full verification (below), independent review by a subagent that did not see the build, review findings fixed or accepted, and the manual Windows checklist handed to you.
* Ask you to run `ffmpeg -version` on the bundled binary for AC-HK-8 (the build environment cannot reach the download host).

## Files that change (patterns)

* Rust: `lib.rs`, `database/manager.rs`, `api/api.rs`, `audio/{recording_saver,import,decoder,recording_commands,recording_preferences}.rs`, `tray.rs`, `notifications/*`, `summary/templates/loader.rs`, new `adopt.rs`, `legacy.rs`, `paths.rs`, `brand_generated.rs`; deletions listed in WP4.
* Frontend: about 84 files for colours (one pattern, described once above); 26 files for analytics call sites; `layout.tsx`, `globals.css`, `tailwind.config.js`, `About.tsx`, `Logo.tsx`, `Sidebar/*`, settings components; new `styles/`, `scripts/`, `tests/ui/`, `brand/`.
* Config: `tauri.conf.json`, `package.json`, `Cargo.toml`, `.claude/*`, `docs/*`.

## Reuse (do not rebuild)

* The allowlist and marker logic in `api/recording_files.rs`, the SHA-256 pinning pattern in `diarization/model.rs` and `build/ffmpeg.rs`, the `Busy` guard pattern in `diarization/commands.rs`, `tauri-plugin-dialog` for the picker, the existing in-memory migration test pool in `database/repositories/*`, `lucide-react` for icons.

## Risks

1. **Identifier and adoption (riskiest).** Hard to prove on Linux. Mitigations: copy-only, markers, temporary folder then rename, old data never touched, tests on a temporary tree, a Windows checklist at A1. Tripwire: if the checklist fails and the cause is not quickly fixable, fall back to D2's alternative (keep the old identifier, change only visible names), which is a one-line change in `brand.json`.
2. **Re-skin regression.** 1,441 class uses in 84 files. Mitigations: codemod with an unmapped report, one directory per commit, screenshots in both themes, no logic edits in these commits.
3. **Telemetry removal.** 94 call sites. The stub stage lets `tsc` guard each removal. The three direct `invoke` calls in the consent switch go away with it.
4. **Migration ordering.** The backup precedes the drop migration. Older builds cannot open a database migrated by newer ones, which is why the identifier switch (separate data folder) comes first.
5. **LLM prompt text.** Dash edits in `summary/processor.rs` change model input. Hand edited, with the 30 processor tests as a guard.
6. **Dependencies.** Two fontsource packages and `rfd` as a direct dependency, all pinned. `posthog-rs`, the updater and process plugins, and two npm packages are removed. Lockfiles reviewed in the diff.
7. **Not testable here:** Windows dialog, WebView profile behaviour, NSIS visuals, the installer. Covered by the checklist.
8. **Existing red tests.** The 19 failures are unexplained. If the base commit is also red, they are fixed in WP1 or reported to you, never skipped silently.

## Options rejected

* Keep `com.meetily.ai` internally (D2's alternative): simpler, but leaves the upstream name in the data path and tray process. Kept only as the tripwire fallback.
* Edit or delete the old licensing and notes migrations: breaks sqlx checksum checks for every existing database.
* A TypeScript-AST codemod (ts-morph): heavy for class strings; a regex mapping with a report and screenshots is enough.
* Hand-placing font files with `next/font/local`: more steps than the pinned fontsource packages for the same result.
* Doing adoption last: the isolation it gives is worth more than deferring risk.
* A native modal dialog from inside `.setup`: blocking dialogs on the main thread can deadlock, so adoption runs before the builder.

## Proof (what runs before anything is called done)

Commands, from the repository root unless noted. The Rust recipe (system ONNX Runtime, stub `llama-helper`) goes into the new `CLAUDE.md`.

* `cargo check --lib` and `cargo test --lib` in `frontend/src-tauri`. Report passes against the WP1 baseline, with analytics tests removed and new tests added.
* In `frontend`: `pnpm typecheck`, `pnpm lint` (dashes, brand, colours, contrast, brand drift, traceability), `bun test`, and the Playwright UI suite.
* `pnpm brand:apply` with the name changed to "Marginalia", then `pnpm brand:check`: only `brand.json` and generated files differ (AC-ID-1).
* Independent review of the diff against `spec.md` and this plan, in a fresh subagent.

Windows checklist for you at each checkpoint:
* A1: install beside the old app; the adoption prompt appears; choose Copy; meetings, people, folders and models are present; no onboarding; the old app still opens its own data.
* A3: light, dark and system themes switch (including the title bar); the About screen shows the credit; the icon is the new one in the taskbar, tray and installer.
* A4: choose a recordings folder; a recording lands there; run `ffmpeg -version` on the bundled binary and send me the `configuration:` line.

## Acceptance criteria covered by Phase A

AC-ID-1 to AC-ID-6, AC-UI-1 to AC-UI-8 (AC-UI-9 only if the illustration is supplied), AC-HK-1, 2, 5, 6, 7, 8, 9, AC-NF-3, AC-PROC-1, AC-PROC-2, and the startup-time part of AC-NF-1.

## Later phases (each gets its own plan section and gate)

Order from 2026-11-01: A, then D (Live Q&A and My notes), then B, C, E, F (spec section 12).

| Phase | Content | Acceptance criteria |
|---|---|---|
| B. Privacy core | Retention, sweep, finalizer, transcript-only, consent reminder, first-run notice | AC-RET-1 to 15, AC-TO-1 to 4, AC-CON-1 to 3, AC-HK-4, sweep and reminder parts of AC-NF-1 and AC-NF-2 |
| C. Models | Models screen, cloud switch, Credential Manager, download manifest (and the Parakeet host question) | AC-MOD-1 to 6, AC-QA-5, AC-HK-3 |
| D. Live Q&A and My notes (next after A) | My notes pane and storage, notes in the summary prompt, benchmark in two profiles (CPU and CUDA for the NVIDIA GPU), context builder, Ask box | AC-QA-1 to 4, AC-QA-6, AC-QA-7, AC-NOTE-1 to 5 |
| E. Vault export | Format library and fixtures, one-way export, Sync screen, disclosure | AC-SYNC-1, 2, 5, 6, 8, 9, export part of AC-NF-1 and AC-NF-2 |
| F. Two-way sync | Import, conflicts, deletion propagation, after the device checks | AC-SYNC-3, 4, 7 |
| WP5 | Tokens from spec 3.2 plus hover and pressed variants | Also `surface-hover`, `accent-tint`, `danger-tint`, `success-tint`, `warning-tint`, eight speaker pairs and HSL copies of eleven tokens for BlockNote. All text pairs are in `tokens.contrast.json` (89 checks) | The codemod needed tints for alerts and badges; BlockNote's precompiled CSS reads `hsl(var(--x))` |
| WP5 | Theme stored as an override | Stored in WebView local storage, applied before first paint by an inline script | No flash of the wrong theme; nothing to migrate |
| WP5 | shadcn `accent` | shadcn's own `accent` (a grey hover) is now `surface-hover` in the five ui components, so `accent` means terracotta as the spec says | Name clash |
| WP6 | One directory per commit with before and after screenshots | Seven codemod commits, then hand work, reviewed with one full screenshot pass in light and dark (before shots taken once, light only). The first codemod commit also carried the speaker tokens and the codemod script | A full capture per directory would have tripled the run time without changing the review |
| WP6 | Not in the plan | Tailwind now scans `src/lib`, `src/hooks` and `src/contexts`; BlockNote overrides moved outside `@layer base` | Speaker colours had never been generated, and Tailwind silently dropped layered rules for classes it never saw |
| WP6 | Not in the plan | Keyboard fixes: focus ring on the summary editor, 24 px speaker chips, switches and resize handle, hover-only buttons visible on focus, an accessible name on the sidebar toggle | Found by the keyboard check (AC-UI-4) |
| WP7 | Installer images until the owner supplies generated art | Plain on-brand BMPs rendered from the icon SVG by `scripts/brand-assets.mjs` with Playwright's Chromium | No image library on the build machine |
| A3 review | Icon redrawn as an SVG master (spec 3.2, F-UI-9) | the owner supplied the final artwork on 2026-11-01: a transparent mark (`brand/source/mark.webp`) for the app and installer images, and a tiled version (`brand/source/app-icon.webp`) for the .exe, taskbar and tray. Used as given; `scripts/brand-assets.mjs` only resizes and converts | the owner's choice. The SVG concept is retired |
| A3 review | Serif for all headings, 10 px cards, 8 px controls | Serif for page titles (h1, h2) only, sans for section headings; 8 px cards, 6 px controls; no pill-shaped buttons or badges; sidebar with a header collapse button, one primary New recording button and compact navigation rows | the owner asked for a more modern look, not anchored to the upstream layout |
| WP8 | New installs default to `<app data>/recordings`; adopted or older installs keep their saved path | Also: an adopted install with no saved folder keeps the earlier default (where its recordings are); a recording whose folder cannot be written falls back to app data with a warning; a folder is checked before it is saved | Phase A review findings 1, 2 and 10 |
| WP9 | Independent review | Done by a subagent that did not see the build. Fixed: the recordings folder findings, the Settings label, sidebar keyboard access and names, an Escape exit from the editors (WCAG 2.1.2), the startup theme flash, the drop overlay, the three dead files still tracked (`core-old.rs`, `recording_saver_old.rs`, `recording_commands.rs.backup`), the hard-coded version and the upstream issues link. AC-ID-5 now excludes the test that proves the table is dropped | Review report, 2026-11-01 |

Feedback from the A1 and A2 Windows tests, to place when those phases are planned:

* **Phase C:** a summary through a local OpenAI-compatible server (LM Studio) failed because Quire sends the whole transcript to a custom address in one request (`summary/service.rs`, threshold 100000). It needs the model's context size, from a setting or from the server, and chunking like Ollama. Also: let the built-in engine use a GGUF from a folder the person picks (for example LM Studio's), so models are not downloaded twice.
* **Speakers (phase to be decided):** speaker detection separated a male and a female voice but mixed up two female voices.
* **Build (decide before Linux builds):** there is no `.gitattributes`, so migration files have CRLF on Windows and LF on Linux, and sqlx checksums differ between the two. Databases made by Windows builds would be refused by a Linux build.

## Phase D. Live Q&A and My notes

Status: approved by the owner on 2026-11-01 at the plan gate, with PD1 to PD4 as recommended. Branch `enhance/quire-phase-d`, started from the end of Phase A.

Covers F-NOTE-1 to F-NOTE-7, F-QA-1 to F-QA-8, AC-NOTE-1 to AC-NOTE-5 and AC-QA-1 to AC-QA-7. Same rules as Phase A: tests first, one commit per work package, a Windows installer and checklist at each checkpoint, an independent review before close-out.

### What the code research found (shapes the plan)

* Summaries: the UI builds the transcript text (with speaker names) and calls `api_process_transcript`; Rust runs chunk, combine and final-report steps in `summary/processor.rs`. The final user prompt is built inline (lines 498 to 504), so it needs a small pure builder before notes can be added and tested. The summary cache key must include the notes, or a regeneration after editing notes would reuse the old result.
* Nothing streams today: every provider returns one whole response, and the UI polls for summary status.
* The built-in model is not safe with two requests at once. An Ask arriving during a summary could swap responses, or time out and kill the summary (a timeout or a cancel shuts the whole sidecar down).
* Only Ollama and the built-in model are ever chunked. Cloud and custom servers (LM Studio) get the whole transcript in one request, which is why your LM Studio summary failed, and Ask would fail the same way.
* During a recording the meeting row does not exist yet; it is created a few seconds after Stop. Live lines are available from Rust (`get_transcript_history`) but carry no speaker. Crash recovery keeps live data in IndexedDB keyed by a frontend session id.
* The meeting page shows transcript and summary side by side; the live screen has no right-hand pane at all.

### Decisions for you at this gate

* **PD1. Where notes live during a recording.** Recommended: in the same IndexedDB recovery store as the live transcript (a new `notes` store keyed by the session id), moved into SQLite when the meeting is saved or recovered. One recovery path for both. Alternative: a `notes.json` in the meeting folder (on disk, but in a folder that may be shared with the upstream app).
* **PD2. Streaming.** Recommended: stream Ask answers word by word for Ollama, LM Studio and other OpenAI-compatible servers, and the cloud providers; the built-in model shows a progress indicator and the full answer at the end (streaming it needs a change to the bundled helper, which can follow later). Summaries stay as they are.
* **PD3. Context size for custom servers (pulled forward from Phase C).** Recommended: a "Context size" field for Custom OpenAI (default 8,192, which matches LM Studio's default), used to chunk summaries and to budget Ask. This also fixes the LM Studio summary failure. Without it Ask cannot work reliably with LM Studio.
* **PD4. CUDA benchmark profile.** The spec asks for a benchmark on CPU and on a CUDA build for your NVIDIA GPU. A CUDA build needs the NVIDIA CUDA Toolkit (a large download). Recommended: benchmark the normal CPU build in D4, and add the CUDA profile only if you want to install the toolkit.

### Order of work: four checkpoints

**D1. My notes** (F-NOTE-1 to 4, F-NOTE-6, F-NOTE-7; AC-NOTE-1 to 4)
* Migration `meeting_user_notes` (`meeting_id` primary key, cascade on delete, `body_blocks`, `body_markdown`, `updated_at`); repository with tests; `api_get_user_notes` and `api_save_user_notes`; explicit delete step added to the meeting delete transaction.
* Live notes: IndexedDB `notes` store (PD1), saved 1 second after the last keystroke, moved to SQLite when the meeting is saved (`useRecordingStop`) or recovered (`useTranscriptRecovery`), and deleted with the recovery record.
* UI: the meeting page's right pane gets My notes and Summary tabs; the live screen gets a right-hand My notes pane beside the transcript. Notes reuse the BlockNote editor (with the Escape exit from Phase A).
* Summary: notes go into the final report step as an `<attendee_notes>` block with the F-NOTE-3 rules; a pure prompt builder makes "no notes means a byte-identical prompt" testable (AC-NOTE-2); notes join the summary cache key.
* F-NOTE-6: after generation, summary lines that closely match a note line are shown in full ink and the model's additions in a softer tone. Done by a tested matching function, not by asking the model to mark them.

**D2. AI plumbing** (prerequisite for Ask; no visible change except PD3)
* A request queue for the built-in model: one request at a time, a queued request waits without timing out, and cancelling one request never kills another.
* Streaming client for HTTP providers (PD2) with an idle timeout instead of a whole-request timeout, a per-request cancel registry, and a Tauri event for streamed text.
* Custom OpenAI context size (PD3), used for summary chunking and the Ask budget.

**D3. Ask** (F-QA-1 to 8, F-NOTE-5; AC-QA-1, 2, 4 to 7, AC-NOTE-5)
* Pure, tested context builder (spec 3.4): notes first, labelled `[your notes]`; the newest lines up to the recent budget (8,000 tokens to start); BM25 matches from earlier lines up to the retrieval budget (3,000); every line labelled with speaker (saved meetings) and `mm:ss`.
* `api_ask_transcript(meeting_id or live, question)` reads live lines from `get_transcript_history` or saved lines from SQLite, plus notes; streams the answer; keeps only citations inside supplied segments; clicking a citation scrolls to the line.
* Ask box under the right pane on the live screen and on saved meetings, collapsed by default; session-only history; "Add to note" appends a labelled AI answer to My notes; "Sends the transcript to <provider>" shown when the provider is not on this computer (the cloud switch itself arrives with Phase C); "Download a model in Settings" when none is installed.
* Transcription first (F-QA-5): Ask runs at lower priority and is refused with a clear message when the transcription backlog is over a limit.

**D4. Benchmark and close-out** (AC-QA-3)
* Benchmark script on your PC: first-token time and transcription backlog with Parakeet running, over 30, 60 and 120 minute transcripts, for each installed model; record the numbers and set the budgets and the backlog limit here.
* Full verification, independent review, fixes, Windows checklist.

**D5. Notes-first meetings** (F-NM-1 to F-NM-6; AC-NM-1 to AC-NM-4; added 2026-11-02, needs your accept)
* Rust: `api_create_meeting(title)` inserts a meeting with no folder and returns its id. `save_transcript` takes an optional existing meeting id and, when given, sets `folder_path` and adds the transcripts to that row in one transaction instead of inserting a new meeting. The recording start command carries the optional meeting id through to the saver. `api_discard_empty_meeting(id)` deletes the row only when it has no transcripts, no notes, no folder and its default title (database only, never `delete_recording_files`). Tests first for each.
* Recovery: the IndexedDB recovery record stores the target meeting id, so a recovered recording attaches to that meeting (AC-NM-4).
* UI: "New meeting" button in the sidebar (above the Record button) and on the home screen; it creates the meeting and opens `/meeting-details?id=...` with My notes focused. The meeting page gets a Record button in its header while it has no recording; while recording it shows the live transcript beside My notes in the same page, using the existing live components. Leaving an untouched new meeting calls the discard command. The sidebar Record button behaves as today (F-NM-6).
* Notes during a meeting-first recording save straight to SQLite, since the row exists.
* Out of scope for D5: F-NM-7 (summary from notes alone) and more than one recording per meeting.

Decisions for you at this gate:
* **PD5. Default title.** Recommended: "Meeting, 2 Nov 2026 14:05", editable in place like today. Alternative: "Untitled meeting".
* **PD6. Second recording.** Recommended: one recording per meeting for now (F-NM-4). Adding a second recording to the same meeting means merging audio files and transcript timelines, which belongs with the retention work in Phase B.

### Windows checklist per checkpoint (for you)
* D1: type notes during a recording and after; close the app mid-recording and recover; notes are back and the summary uses them; regenerating keeps them.
* D2: run a summary with LM Studio (context size set) on a long meeting; it completes.
* D3: during a recording ask "what did I miss?"; answers cite times you can click; "Add to note" works; a question during a summary does not break the summary.
* D4: review the benchmark numbers and the chosen limits.
* D5: click New meeting and type at once; record from that meeting and stop; one meeting holds notes and transcript; open New meeting and leave without typing; it is gone from the list.

### Risks
* Built-in model concurrency is the riskiest change (it touches the summary path). Mitigation: the queue lands with tests in D2 before Ask uses it.
* Small local models may answer poorly over long meetings. Mitigation: the budgets and the "answered from" line make the limits visible; tuning in D4.
* Live speaker names are not available during a recording; live answers cite times only.

### Handoff, 2026-11-02 (Windows PC to Mac)

* Branch `enhance/quire-phase-d`. D1 to D5 are built, reviewed and fixed; the latest Windows installer was built from commit 91d66f3.
* Waiting on the owner: the Windows checklist above (D2, D3, D5), and the decision on Obsidian export (below).
* Open items: the token estimate for Chinese, Japanese and Korean text; Ask does not check that Ollama is running; built-in cancel within 2 s is untested; AC-NM-1 (notes focused in under 500 ms) is to be measured in an installed build; AC-NM-4 needs a real crash test.
* Obsidian request (2026-11-02, not yet in the spec): choose a folder; each meeting is a readable `.md` file there (title as file name, notes, summary and transcript in the body), so an Obsidian vault can show and organise it. The vault format in `sync-format.md` does not fit as written (ID file names, transcript in a JSON sidecar). Recommended: a one-way Markdown export ahead of Phase E. Needs a spec amendment and the owner's accept before code.
* Mac: CLAUDE.md lists Windows and Linux setup only. The first Mac session should work out the macOS build (Xcode command line tools, Rust, pnpm, CMake, ONNX Runtime, the sidecar binaries), record it in CLAUDE.md, and run the full verify. Meeting data and models stay on each computer; they are not in git.
* Mac result (2026-11-02, Mac, Apple silicon, macOS 26.6): setup recorded in CLAUDE.md. `pnpm verify` green (lint 6 of 6, 131 bun tests and 1 node test); Playwright 64 of 64; `cargo check --lib` clean; `cargo test --lib` 447 passed, 0 failed, 6 ignored. No macOS-only fixes were needed. `pnpm tauri:build` makes `Quire.app` and a `.dmg` (Metal and CoreML), ad-hoc signed; the app launches and creates its database. Open: the macOS ffmpeg download is not hash-pinned (Windows is), and it is a GPL build (concern 17); the Command Line Tools on the Mac are 14.3 and should be updated.

## Phase M. Markdown export for Obsidian

Status: approved by the owner on 2026-11-02 at the plan gate, with PM1 to PM5 as recommended. Covers F-MD-1 to F-MD-9 and AC-MD-1 to AC-MD-7 (spec section 2.13). Branch `enhance/quire-phase-m`, started from the end of Phase D. Same rules: tests first, one commit per work package, independent review before close-out.

### Decisions for you at this gate

* **PM1. When it exports.** Recommended: automatically (after save, after a summary, 5 s after notes stop changing) plus "Export all now". Alternative: only when you press Export.
* **PM2. File name.** Recommended: `2026-11-02 1405 Weekly planning.md` (sorts by date, rarely clashes). Alternative: the title only (`Weekly planning.md`), which clashes for recurring meetings.
* **PM3. Edits made in Obsidian.** Recommended: never overwrite them; list the file and offer "Replace with Quire's version" (F-MD-6). Alternative: Quire always overwrites its own files, and Obsidian edits are lost on the next export.
* **PM4. Deleting a meeting.** Recommended: delete its file only if unchanged since export. Alternative: always leave the file.
* **PM5. Order.** Recommended: build Phase M next, before B, after you finish the D checklist. It touches no recording or deletion code, so it does not hold up Phase B.

### Order of work

**M1. Renderer** (pure, no file access). `markdown_export/render.rs`: meeting, notes, summary markdown and named transcript segments in, file name and bytes out. Uses the strict front matter writer (sync-format section 4) and the name-cleaning rules (section 15). Neutralises image embeds, raw HTML and `javascript:` links. Fixture tests for AC-MD-2, AC-MD-3 naming and AC-MD-6.

**M2. Writer and state.** Migration `markdown_exports` (`meeting_id` primary key, cascade on delete, `rel_path`, `sha256`, `exported_at`); export folder and on or off in settings. Writer: path derived from cleaned names inside the chosen root, no symbolic links, temporary file then rename, outside-edit check by hash before any overwrite, rename or delete. Hooks after meeting save, summary completion, title or folder change, notes save (debounced) and meeting delete. Speaker names come from the same lookup Ask uses for saved meetings. Filesystem tests in a temporary tree for AC-MD-3 to AC-MD-5.

**M3. Settings.** A "Markdown export" card in Settings, General: folder picker (`tauri-plugin-dialog`), confirmation with the synced-folder sentence, on or off, "Export all now", last export time and the "edited in another app" list with Replace. UI test and screenshots in both themes (AC-MD-1, AC-MD-5).

**M4. Close-out.** Full verify on the Mac (and Windows when you are back), independent review, and the manual Obsidian check (AC-MD-7): point a real vault at the folder, check properties and folders, and watch for network requests.

### Risks
* Renames are the riskiest part: a title edit must move one file, never leave two or delete one you edited. The hash check guards it, with tests for each path.
* Obsidian may index a half-written file. Atomic rename avoids it.
* An export folder inside iCloud or Obsidian Sync puts transcripts on those services. The confirmation says so; the app cannot detect every sync tool.

## Phase B. Privacy core: retention, transcript-only, consent

Status: approved by the owner on 2026-11-02 at the plan gate, with PB1 to PB7 as recommended. Covers F-RET-1 to F-RET-14, F-TO-1 to F-TO-5, F-CON-1 to F-CON-6, AC-RET-1 to AC-RET-15, AC-TO-1 to AC-TO-4, AC-CON-1 to AC-CON-3 and AC-HK-4 (logs). Branch `enhance/quire-phase-b`, from the end of Phase M. Same rules: failing tests first for every deletion rule, one commit per work package, independent review, a release with both installers at the end.

### What the code research found (shapes the plan)

* The meeting row is saved by the window, not by Rust: Stop writes the files, then the window calls `api_save_transcript` and attaches notes. Rust never learns that the save succeeded, and Rust save errors are swallowed. A finalizer therefore has to be a Rust command the window calls after the save, and again after a crash recovery.
* Recording folders are named `<title>_<YYYY-MM-DD_HH-MM>` with `create_dir_all`, so two recordings with the same title in the same minute share one folder. Deleting one meeting's audio could remove the other's.
* Recordings live under several roots: the current setting, the app data fallback, the earlier `Music/meetily-recordings` for adopted installs, and older meetings keep the folder they were made in.
* `delete_recording_files` removes transcripts and metadata too, stops at the first error and does not check a root. Meeting delete removes the database row first, so a failed file delete orphans the folder with no retry, and the sidebar still says everything was removed.
* The deletion allowlist misses `audio.aac` and upper-case extensions (`audio.MP3` from import), while speaker detection finds any audio extension, so deletion could miss a file that detection uses.
* `auto_save = false` still creates a folder with `metadata.json` and `transcripts.json`, never finalizes it, and sends no saved event (spec section 1 still holds).
* There is no audio playback in the interface today, so F-TO-3's "playback disabled" has nothing to disable.
* A consent toast with "Don't show this again" already exists (`lib/recordingNotification.tsx`, stored as `show_recording_notification`); the old `ComplianceNotification` with "US compliance required" is no longer shown. Every start goes through `useRecordingStart`.
* The external-link opener checks the scheme (http, https, mailto) but has no host allowlist, which AC-CON-2 assumes.
* Logs at Info include transcript text in the Whisper engine, and titles or folder paths (which hold titles) in about a dozen places; Debug logs include whole segments.
* On a first launch the database is opened only after onboarding, so a launch sweep must wait for it. No window focus event is handled today.

### Decisions for you at this gate

* **PB1. Where finalization runs.** Recommended: a Rust command `api_finalize_recording(meeting_id)` that the window calls after the save and notes attach (and after a recovery). It runs detection if that setting is on, stamps `finalized_at` and the deadline, and for Off deletes the audio. Alternative: move the whole meeting save into Rust at Stop, which is cleaner but rewrites the save and notes-first attach paths tested in D5.
* **PB2. Shared folders.** Recommended: from now on every recording gets its own folder (seconds plus a short random suffix). A folder that more than one meeting points at is never deleted automatically; it is listed as shared, with a manual choice.
* **PB3. Where deletion may happen.** Recommended: only inside a known recordings root (the current setting, the app data folder, the earlier default) or the exact folder the meeting row names when it carries the app's marker files. Anything else is listed as "outside the recordings folders" and kept.
* **PB4. Transcript-only folders.** Recommended: keep writing `transcripts.json` and `metadata.json` (text only), because crash recovery reads them; never any audio, checkpoint or temp audio file. Alternative: no folder at all, which loses crash recovery for transcript-only recordings.
* **PB5. The consent reminder.** Recommended: replace the current toast with the spec card (F-CON-1 to 4) and keep its stored "never show again" choice, so your earlier answer carries over. The guidance link opens only a short host allowlist (one reference site), checked live during the build.
* **PB6. Scheduled task (F-RET-11, should).** Recommended: defer. It needs a `--retention-sweep` mode that runs before the single-instance plugin, and a Task Scheduler entry; worth its own small checkpoint after B.
* **PB7. Defaults.** As accepted in the spec (D3a, D7): new installs 7 days; your existing installs start at Keep, so upgrading deletes nothing. Detect-on-save starts off.

### Order of work

**B1. Safe deletion and logs** (F-RET-5, 6, 7, 12; AC-RET-2, 3, 5, 10; AC-HK-4). Tests first.
* `retention/files.rs`: list and delete one meeting's audio artefacts (final audio in any case and extension including aac, checkpoints, `.quire_decode_` and legacy temp files, the vault `_audio` copy when sync exists), file by file, verifying each is gone, returning a result per file. Refuses links and junctions, folders without markers, shared folders (PB2) and folders outside known roots (PB3).
* Unique folder names for new recordings (PB2).
* Two-phase meeting delete (F-RET-12): mark deleting, remove files, then the row; a locked file keeps the meeting listed as "deleting" for the sweep to retry. The sidebar reports what actually happened.
* Log clean-up: no transcript text, question text, titles, names or keys at any level; folder paths logged without the title part. A test runs a scripted session and searches the captured logs.

**B2. Retention** (F-RET-1 to 4, 8 to 10, 13, 14; AC-RET-1, 4, 6 to 9, 11 to 13, 15).
* Migration: `audio_retention`, `audio_expires_at`, `audio_state`, `audio_deleted_at`, `finalized_at`, `mode` on `meetings`, existing rows set to Keep and `present` or `none`; a small `retention_state` table for the last run time (clock guard). The Phase A backup runs first.
* Pure `due(meeting, now)` with an injected clock; the finalizer command (PB1) with detect-on-save; stamping on save, recovery and import.
* Sweep: after the database is ready at launch, every 60 minutes, and when the window regains focus; one at a time behind a busy guard shared with detection and retranscription; mass-expiry guard (more than 20, or a quarter) and backwards-clock pause.
* Interface: a Retention section in Settings, Recordings (six choices, plain descriptions, the limits from F-RET-10, "Apply to existing" with counts and confirmation); per meeting "Audio kept / Audio deleted on 8 Nov / No audio saved / Deletion pending", "Keep this one" and "Delete audio now"; unmanaged and shared folders listed under Storage.

**B3. Transcript-only** (F-TO-1 to 5; AC-TO-1 to 4).
* A remembered "Transcript only: do not save audio" switch next to Record, carried per recording to the recorder, which then never builds the incremental saver; a finalization path that completes `metadata.json`, sends the saved event and stores `mode`.
* Detect speakers and Enhance disabled with a reason; the F-TO-4 wording and the one-sentence difference from Off.
* AC-TO-1 with a file watcher (the `notify` crate if already present, otherwise added and pinned) on the folder and the temp folder during a synthetic recording.

**B4. Consent and first run** (F-CON-1 to 6; AC-CON-1 to 3).
* The reminder card near the record controls, non-blocking (measured under 100 ms), with Got it, Never show this again and Read more; the Read more page with the spec 3.5 copy; the external reference verified live and opened through a host allowlist.
* The personal-use first-run notice: a step in onboarding for new installs, a one-time card for existing ones, readable again in a new Settings, Privacy section that also resets the reminder.

**B5. Close-out.** Full verify on the Mac, independent review, fixes, a release with both installers, and the Windows checklist.

### Windows checklist (for you)
* B1: delete a meeting while its audio is open in another program; it shows "deleting" and goes when the program closes. Logs from a recording contain no transcript text or titles.
* B2: set Off, record, stop: the transcript stays, the audio goes; set 24 hours and change the clock forward a day: the sweep deletes, and a 21-meeting batch asks first; the upgrade from your current install deletes nothing.
* B3: record in transcript-only; no audio file appears; detection is disabled with a reason.
* B4: the reminder shows on every start until you tick Never; the first-run notice shows once.

### Risks
* Deletion cannot be undone. Mitigations: tests first for every rule, Keep for existing installs, the mass-expiry and clock guards, shared and outside folders never touched, and a dry-run count before "Apply to existing".
* The finalizer depends on a window round trip; if the window closes between save and finalize, the next launch finds meetings with no `finalized_at` and finalizes them (never deleting before the save is confirmed).
* Windows file locks (antivirus, sync clients) make retry the normal path; the state stays "deletion pending", never a false "deleted".
* Phase B touches the recording stop path; the D5 notes-first tests guard it.

### Spec amendments proposed with this plan
* F-TO-3: there is no audio playback today, so only detection and Enhance are disabled.
* F-RET-7 and AC-RET-5: "never leaves the recordings root" becomes the known-roots rule (PB3), and shared folders are never deleted automatically (PB2).
* AC-CON-2: the guidance link uses a host allowlist added in B4; the general opener keeps its scheme check.
* F-RET-11: deferred to a checkpoint after B (PB6).

## Phase R. Remove the upstream association

Status: approved by the owner on 2026-11-02 at the plan gate, with PR1 to PR7 as recommended. The owner asked to drop the association with the upstream app now that Quire has moved beyond it. Branch `enhance/quire-phase-r`, from the end of Phase B.

### What must stay
* The MIT licence's one condition: the upstream copyright and permission notice stays in `LICENSE.md` (and in copies of the app). Nothing else about the upstream app is required.

### What the inventory found (2026-11-02)
* Upstream leftovers with no use in Quire: the archived Python `backend/` (with a git submodule pointing at the upstream whisper.cpp fork), 9 upstream CI workflows built around upstream signing secrets (none runs tests), upstream build scripts in `frontend/` (`build.bat`, `build.ps1`, `build-gpu.*`, `dev-gpu.*`), upstream docs and images (`docs/`, 33 MB, including the old README and demo GIFs), `CONTRIBUTING.md`, `frontend/README.md` and `frontend/API.md`.
* Functional ties: the Parakeet model downloads from a server the upstream project runs; data adoption from `com.meetily.ai` and the legacy database import screens; installs that adopted the old data and never chose a folder still default to the old `meetily-recordings` folder; the Rust crate and the npm package are still named `meetily`.
* Text: the About credit, the README credit, the `upstreamCredit` brand value, `PRIVACY_POLICY.md`, `THIRD_PARTY_NOTICES.md`, CLAUDE.md and the planning docs.
* Unchangeable: one comment in an applied migration (sqlx checksums every byte), and the upstream commits in the git history.

### Decisions for you at this gate
* **PR1. Credit.** Recommended: remove the About credit, the README credit and the brand credit value; keep the upstream notice in `LICENSE.md` (required) and one line in `THIRD_PARTY_NOTICES.md` pointing at it, which is where licences belong. Alternative: `LICENSE.md` only.
* **PR2. Data adoption and legacy import.** Recommended: remove them (`adopt.rs`, the database import screens, the Homebrew detector, the legacy names in `brand.json`). Your installs already adopted their data in Phase A, and new installs start fresh. After this, an old upstream install on another computer is no longer offered a copy.
* **PR3. The old recordings folder.** Recommended: on the first launch of this build, an install with no saved recordings folder gets its current folder written into its settings, so nothing moves and recordings keep landing where they are; then the legacy default goes. Folders already on disk keep their names (renaming them would break the paths stored with each meeting). Leftover `.meetily_decode_` temp files stop being cleaned.
* **PR4. Parakeet download source.** Recommended: download from the public Hugging Face repository the model comes from, checked live during the build, with each file pinned by SHA-256 (part of F-MOD-8 pulled forward from Phase C). Existing downloaded models are kept and not downloaded again. Alternative: leave it until Phase C.
* **PR5. Internal names.** Recommended: rename the Rust crate and the npm package to `quire` (log filters and build scripts updated with them). The app identifier and data folder do not change.
* **PR6. Upstream leftovers.** Recommended: delete them (list above). Useful content is already in CLAUDE.md and the release scripts. The Mac and Windows release scripts and `build_installer_windows.bat` stay.
* **PR7. History.** Recommended: keep the git history as it is. The upstream commits are an honest record, the MIT licence allows them, and rewriting them away would also rewrite all of Quire's history again. Alternative: start the repository again from a single commit (loses all history).

### Order of work
* **R1. Settings safety first** (PR3), tests first: pin the current recordings folder for installs that rely on the legacy default, then remove adoption and the legacy import (PR2). A test proves an adopted install keeps recording to the same folder.
* **R2. Model source** (PR4): new pinned source, a test that a mismatched file is deleted and refused, existing models untouched.
* **R3. Names and credit** (PR1, PR5): crate and package rename, brand values, About, README, notices; the brand lint widens to every tracked file except `LICENSE.md`, `THIRD_PARTY_NOTICES.md` and applied migrations.
* **R4. Clean-up and close-out** (PR6): delete the leftovers, reword the planning docs to "the upstream project" where a name is not needed, full verify, independent review, release, Windows checklist.

### Windows checklist (for you)
* After installing: meetings, people and folders are all there; a new recording lands in the same recordings folder as before; transcription still works with the model you already downloaded; About no longer mentions the upstream app.

### Risks
* PR3 touches where recordings go; the pin runs before anything else reads the setting, and is tested on a copy of an adopted settings file.
* PR4 changes where a large model comes from; a wrong file is refused by its hash, never loaded.
* PR5 renames touch build scripts and the Windows cross-build; both installers are rebuilt and checked before release.

## Phase C. Models, keys and the cloud switch

Status: approved by the owner on 2026-11-02 at the plan gate, with PC1 to PC8 as recommended. Covers F-MOD-1 to F-MOD-8, AC-MOD-1 to AC-MOD-6, AC-QA-5 and AC-HK-3 (network inventory), with the spec amendment in section 17. Branch `enhance/quire-phase-c`, from the end of Phase R. Same rules: failing tests first, one commit per work package, independent review, a release with both installers at the end.

### What the code research found (shapes the plan)
* Keys: nine plaintext key columns across `settings` and `transcript_settings`, plus the key inside the `customOpenAIConfig` JSON. Five commands return keys to the window in plaintext, the window passes keys back to the model-list commands, and the config (key included) is broadcast as an event and printed to the window's console in six places. `geminiApiKey` is a column nothing reads. No credential store crate is a dependency.
* Cloud providers: `openai`, `claude`, `groq`, `openrouter` are cloud; `ollama` and `custom-openai` are local only when their address is on this computer (`ask/policy.rs`, `is_on_this_computer`, already tested). There is no switch today; a provider is used as soon as it is selected.
* Endpoints: the Ollama address defaults to localhost; the custom server address has no default; neither asks before a remote host is saved.
* API transcription (D8): Deepgram, ElevenLabs, Groq and OpenAI exist only as commented-out options and key fields. No code sends audio anywhere, so there is nothing to verify.
* Downloads: Parakeet and the speaker model are pinned by SHA-256. Whisper models (Hugging Face `main`) are checked by header and a size within 10 percent; the built-in summary models (Hugging Face `main`) by size within 10 percent and header. Catalogues are duplicated in the window (`lib/parakeet.ts` still lists a `v3-fp32` model the core does not have).
* **An upstream tie Phase R missed:** the build downloads ffmpeg from the upstream company's GitHub releases (`build/ffmpeg.rs`). Only the Windows archive is pinned; the Mac and Linux archives are not checked at all. The name lint only looks for the old product name, so it did not catch the company name.
* Network: 18 runtime call sites in Rust (6 downloads, 4 cloud model lists, 4 Ollama, 4 generation or test) plus 2 browser openers; the window makes no requests of its own. No inventory test exists.
* The Windows feedback about LM Studio summaries is already addressed: Phase D added a context size for custom servers and summaries are split to fit it (D2). Still to confirm on the PC.
* The built-in engine can only load models from its own catalogue (prompt templates are per model).

### Decisions for you at this gate
* **PC1. Key storage.** Recommended: the operating system's credential store through the `keyring` crate: Credential Manager on Windows, Keychain on the Mac (the spec named Windows only). Keys become write-only for the window: it can save, replace or remove a key and learns only "a key is saved", and every request that needs a key reads it in the core. On first launch each key is moved, read back, and only then cleared from the database; if the store is unavailable, saving a key is refused and an existing key stays where it is with a warning, never copied anywhere else. Alternative: keep showing keys in the window (show/hide), which keeps them flowing through the window and its console.
* **PC2. The cloud switch.** Recommended: one stored setting read by `cloud_allowed()`, off for fresh installs, with the one confirmation and the upgrade rule from F-MOD-5. A custom server or Ollama on another computer also counts as sending data off this computer, so it needs the switch as well as the F-MOD-7 confirmation. Alternative: only the four cloud providers need the switch; remote addresses get the confirmation only.
* **PC3. API transcription (D8).** Recommended: remove the dead options and their key fields; their keys (if any) are cleared by the PC1 migration. Bringing a provider back later is its own plan.
* **PC4. The manifest.** Recommended: one `models.json` in the repository with name, size, languages, source pinned to a revision, and SHA-256 for every Whisper, built-in summary, Parakeet and speaker file, checked live during the build. The core reads it at build time and the window gets it through one command, so the two copies of each catalogue go. Models already downloaded are hashed once in the background; a mismatch marks the model "needs downloading again" and it is not loaded.
* **PC5. ffmpeg's source.** Recommended: put the exact ffmpeg archives in use today on a release in your own Quire repository, pin all of them by SHA-256 and size, and point the build there; the GPL source and notice already attached to each release stay. Alternative: download from each platform's original builder, which means new binaries and new checks.
* **PC6. Network inventory.** Recommended: a lint like the log check: every outbound call site in Rust carries a `// net: <what enables it>` tag, and the lint fails on an untagged one (AC-HK-3).
* **PC7. The Models screen.** Recommended: one Models page in Settings with Transcription (Parakeet first, then Whisper) and Summaries and Q&A (built-in, Ollama, then cloud behind the switch), reusing the existing model managers. The transcription language list lives in the manifest, and a language outside Parakeet's 25 gives the Whisper recommendation.
* **PC8. A model from another folder (Windows feedback).** Recommended: defer. The built-in engine needs a prompt template per model, and LM Studio already works as a custom server.

### Order of work
* **C1. Manifest and pins** (PC4, PC5), tests first: a corrupted Whisper or summary download is deleted and refused; existing models are checked once; the ffmpeg build checks all platforms.
* **C2. Network inventory** (PC6): tag the 20 sites, then the lint.
* **C3. Keys** (PC1, PC3), tests first: seeded plaintext keys end in the store with the columns empty; an unavailable store refuses to save and writes no plaintext; no command returns a key; the window's console prints no key.
* **C4. Cloud switch and endpoints** (PC2), tests first: a fresh install has no cloud provider selectable and every cloud path stops before a request; an upgrade with a configured cloud provider starts on with a notice; a remote address needs confirmation.
* **C5. Models screen** (PC7) with UI tests.
* **C6. Close-out:** full verify, independent review, release, Windows checklist.

### Windows checklist (for you)
* After installing: your summary provider still works; if you used a cloud provider, the switch is on with a notice; your keys are in Credential Manager (Windows Credentials, entries ending in com.openedtools.notes) and Settings shows them only as saved; the Models screen lists the models you have as installed without downloading them again; a summary through LM Studio works on a long meeting.

### Risks
* C3 moves secrets: the database column is cleared only after the stored key reads back the same.
* On the Mac, an unsigned build may ask for Keychain access again after each update.
* C1 hashes several GB of existing models once; it runs in the background and never blocks a recording.

## Phase E. Vault export (one way)

Status: approved by the owner on 2026-11-02 at the plan gate, with PE1 to PE6 as recommended. Covers F-SYNC-1 to F-SYNC-4, F-SYNC-9, F-SYNC-10 and AC-SYNC-1, AC-SYNC-2 (export side), AC-SYNC-6, AC-SYNC-8, AC-SYNC-9, with the format in `sync-format.md`. Branch `enhance/quire-phase-e`, from the end of Phase C. Same rules: tests first, one commit per work package, independent review, a release with both installers at the end. Two-way sync (Phase F) stays out: it waits for the device checks in `sync-format.md` section 15.

### What the research found (shapes the plan)
* The vault is the contract with the future iPhone app: stable ID-based file names, titles in front matter, sidecar transcripts, `_people`, `_folder.json`, a version marker. It is not the same thing as the Phase M Obsidian export, which writes readable date-and-title file names for a person to browse.
* Phase M already has the safe parts the vault needs: the strict front matter writer (`markdown_export/render.rs`), name cleaning for Windows, atomic writes that stay inside the chosen folder and never follow links (`writer.rs`), and "never overwrite a file edited elsewhere". Phase E reuses them rather than copying them.
* The vault will hold real meeting content in an iCloud folder. Nothing about it touches the repository: every test and fixture uses generated, clearly fictional meetings (AC-SYNC-1 asks for 50).
* Audio in the vault depends on retention across devices (`sync-format.md` section 10), which only makes sense once something reads the vault back.

### Decisions for you at this gate
* **PE1. What comes next.** Recommended: Phase E now, export only, because the iPhone app needs a vault to read and this is the safe half (nothing is ever read back). Alternatives: start the iPhone app first and come back to E, or a short clean-up phase first (the two-female-voices speaker mix-up, `.gitattributes` line endings, the deferred scheduled sweep PB6).
* **PE2. Vault and Obsidian export side by side.** Recommended: keep both, in separate folders: the vault (machine format, for the phone) and the Obsidian export (readable, for you), each with its own on/off. Alternative: one folder for both, which makes Obsidian show ID-named files and sidecars.
* **PE3. Where the truth lives.** Recommended: the database stays the desktop's working store and the vault is the exchange (`sync-format.md` 16.8, option one). The only database changes are the new `sync_id` columns and a `sync_state` table.
* **PE4. Audio in Phase E.** Recommended: no audio in the vault yet; the audio switch appears in Phase F with the retention rules that need both directions. Alternative: offer the switch now, off by default, write-only.
* **PE5. The vault folder.** Recommended: on Windows, offer the iCloud Drive folder when it exists (`iCloudDrive\Quire`), refuse a root path over 120 characters and warn over 100 (AC-SYNC-6), and show the iCloud disclosure before the first write (F-SYNC-2). Turning it off leaves the files; a separate confirmed action removes the app's files.
* **PE6. Fixtures.** Recommended: build the shared conformance fixtures for the export side now (`docs/vault-fixtures/`, generated content only), so the iPhone app can test against the same files later.

### Order of work
* **E1. Format library**, tests first: the note, sidecar, person, folder and marker writers, byte-stable, with the fixtures (PE6) and the Windows name rules.
* **E2. Database**: `sync_id` columns (ULID, from `created_at`), `sync_state`, filled on first export, never changed after.
* **E3. Exporter**: atomic writes, sidecar before note, skip unchanged rows, then the verification re-read (`sync-format.md` 16.5); a mismatch marks nothing as synced. Round trip test over 50 generated meetings (AC-SYNC-1), and "never synced" over the exported vault (AC-SYNC-9).
* **E4. Sync screen**: choose folder, disclosure, on/off, "Export now", status, remove the app's files (AC-SYNC-8), with UI tests.
* **E5. Close-out**: full verify, independent review, release, Windows checklist.

### Windows checklist (for you)
* Turn on the vault, choose the iCloud Drive folder, read the disclosure, export. Check in File Explorer that `Quire` holds `quire-format.json`, one `.md` and one `.transcript.json` per meeting, `_people` and your folders, and nothing else (no audio, no keys). Check the files reach iCloud on the web. The Obsidian export keeps working as before.

### Risks
* iCloud for Windows is only partly documented; Phase E writes only, so the worst case is a slow or duplicated upload, never lost meetings.
* A large first export (hundreds of meetings) runs in the background and never blocks a recording.

## Phase K. Desktop clean-up

Status: approved by the owner on 2026-11-02 at the plan gate, with PK1 to PK5 as recommended. Small items found in testing and in earlier reviews, before the iPhone app's next step. Branch `enhance/quire-phase-k`. Same rules: tests first, one commit per item, independent review, a release with both installers.

### What the research found (shapes the plan)
* Speakers: detection embeds each line with the WeSpeaker ResNet34 model and estimates the number of speakers from the biggest gap in the clustering tree. Two similar voices sit close together, so "Auto" can merge them; the code says so, and the Detect dialog already lets you give the number of speakers, and each line can be reassigned by hand. The fix is a better voice model, chosen by measurement, not a guess at thresholds.
* Line endings: the repository stores migrations with LF; the Windows build converts them to CRLF on purpose, because your Windows database was created from CRLF files and sqlx checks every byte. A `.gitattributes` rule would only make that explicit; changing line endings for real would make the Windows app refuse its own database.
* The scheduled sweep (PB6, deferred from Phase B): audio is deleted on time only while the app runs.
* Accepted but unfixed in earlier reviews: confirming many deletions passes a count, not the list; the sweep checks for detection and retranscription only when it starts; recordings kept because their folder is shared or outside the recordings folders are not listed; credential store calls and vault file work run on async worker threads (turning the vault off waits for a long first export); three audio import tests read a sample from the deleted backend and so never run; a few compiler warnings.

### Decisions for you at this gate
* **PK1. Speaker model.** Recommended: measure the current model against the 3D-Speaker CAM++ model (Apache-2.0, about 28 MB, also used by sherpa-onnx) on a public meeting recording with labelled speakers (for example from the AMI corpus, CC BY 4.0), downloaded to this Mac and never committed, with the existing opt-in accuracy check; switch only if it is clearly better, pinned by SHA-256 in `models.json`. Alternative: keep the model and only make the Detect dialog suggest setting the number of speakers.
* **PK2. Line endings.** Recommended: a `.gitattributes` that marks migrations as never converted, plus a test that the Windows build script converts every migration, so the current, working arrangement cannot drift. Nothing changes for your databases.
* **PK3. Scheduled sweep.** Recommended: on Windows, a `--retention-sweep` mode that runs before the single-instance check, does one sweep and exits, and a daily Task Scheduler entry the app adds while retention is not Keep and removes when it is (shown in Settings). The Mac waits. Alternative: leave it; deletion happens the next time the app runs.
* **PK4. Review leftovers.** Recommended: fix them all: confirm by the list of meetings, check for detection and retranscription before each deletion, list shared and outside recordings in Settings with a manual choice, move credential store and vault file work off the async threads, and fix the warnings.
* **PK5. The three dead import tests.** Recommended: replace them with tests that read a short generated WAV, so they run again (an edit to existing tests, so it needs your yes).

### Order of work
* **K1** line endings (PK2), **K2** review leftovers (PK4, PK5), **K3** scheduled sweep (PK3), **K4** speaker model measurement and switch (PK1), **K5** close-out: verify, review, release, Windows checklist.

### Windows checklist (for you)
* Settings shows the daily audio check while retention is not Keep; Task Scheduler lists it; after a day with the app closed, an expired recording's audio is gone. Speaker detection on a meeting with two similar voices separates them (if PK1 switched the model).

### Risks
* A new speaker model changes the voices remembered for people: voiceprints from the old model cannot be compared with the new one, so remembered voices are kept per model and re-learned as you confirm speakers.
* A scheduled task runs the app without a window; it must never record, never show anything, and exit when the app is already open.

## Phase L. Local model polish: names, faster first text, a model guide, transcript check, the iPhone look

Status: approved by the owner on 2026-11-03 at the plan gate, with PL1 to PL6 as recommended. Asked for that day after trying the desktop build. Covers the desktop (Windows and Mac share the same frontend) and, where marked, the iPhone app. Branch `enhance/quire-phase-l`. Same rules: tests first, one commit per item, independent review, made-up fixtures only.

### What the research found (shapes the plan)
* Names: the desktop already takes a title from the summary. The template asks for `# [AI-Generated Title]`, and when a summary finishes the first heading replaces `meetings.title` (`summary/service.rs`). So a meeting is named only when a summary runs, the name has no fixed shape, and it overwrites a name you typed. The iPhone never names a meeting.
* Models: the built-in list is Qwen 3.5 2B and 4B and Gemma 3 4B and 1B ("Fast", about 1 GB), all with a 32k window. `llm_client::generate_summary` already sends one prompt to whichever provider is chosen, so a title call needs no new connection code.
* Slow first text, from the code (not yet measured): the live path cuts audio only when the speaker pauses for 0.5 s, with no maximum length, so the first line appears only after the first full sentence (about 4 to 7 s), and continuous speech can show nothing at all (the upstream code notes this as issue 756). Audio also reaches the voice detector in 0.6 s steps. Parakeet is loaded when recording starts and unloaded when it stops, so every recording pays the load time again. The iPhone already cuts at 14 s and after a 0.6 s pause.
* Transcript editing: there is no way to change a line's text today, by hand or otherwise. Only speakers can be changed. Retranscribing replaces every line.
* Look: the desktop uses the same colours and fonts as the iPhone (`tokens.css`) but has no illustrations: the home screen shows "Welcome to Quire!" and a button, and recording shows a pulsing dot.

### Decisions for you at this gate
* **PL1. Automatic names.** Recommended: a separate, short title call when the transcript is final (desktop and iPhone), whether or not a summary is made. It reads the start of the transcript and your notes and returns a topic of at most six words. The summary stops renaming meetings. A meeting is named only while it still has its default name, so a name you typed is never replaced. "Suggest a name" on the meeting does it again on request. Off when no local model is set up; a cloud model is used only if you have already turned the cloud switch on (Phase C).
* **PL2. Name format.** Recommended: a setting with a pattern built from `{date}`, `{time}`, `{topic}` and `{folder}`, a live preview and a default of `{date} {topic}` (for example "2026-11-03 Budget review"). Dates are year-month-day so names sort in order; characters that are not allowed in file names are removed, since the vault file is named from the title. The same setting exists on the iPhone.
* **PL3. Faster first text.** Recommended: measure first, then fix. A test plays a made-up recording through the live path and records the time to the first line, before and after. Then: keep Parakeet loaded while the app is open (loaded in the background at start, so the first recording does not wait), cut a line after at most 14 s of continuous speech as the iPhone does, and feed the voice detector in smaller steps if the measurement shows it helps. Goal: the first line within 2 s of the first pause. Cost: about 1 GB more memory while the app is open; a setting turns this off on smaller machines.
* **PL4. Model guide.** Recommended: a "Which model suits this computer" panel in Settings, Models. It reads the memory and graphics (Apple Silicon, NVIDIA or neither), recommends a model in plain words (for example: "8 GB and no graphics card: Gemma 3 1B (Fast). Summaries take about a minute for an hour's meeting."), and has a "Test speed" button that summarizes a made-up sample transcript and reports how long it took. Nothing is sent anywhere.
* **PL5. Transcript check.** Recommended: yes, as a suggestion list you review, never automatic. "Check transcript" asks the local model for likely mistakes: misheard words, a name spelled differently from People or your notes, the same term spelled two ways. Each suggestion shows the line, the original words, the fix and the reason, with Accept, Edit and Skip, plus "Accept all of this kind" for a repeated name. Lines can also be edited by hand. The original text of every changed line is kept so a change can be undone, and retranscribing warns that it would replace your fixes. Desktop in this phase; the iPhone later, after you have tried it on the desktop (its smaller model makes more false suggestions). Alternative: hand editing only, no model check.
* **PL6. Desktop look.** Recommended: bring the iPhone's style across, sized for a window: sage hills with a small terracotta sun behind the home header, terracotta hills along the bottom of the recording view, the record button with its soft ring, the mirrored waveform, the pill tabs for Transcript, Summary and Notes, folder cards and a one-click Copy on each tab and each answer. Drawn as SVG from the tokens in light and dark, and checked by the existing contrast and screenshot passes. Your ChatGPT art can replace the hills later in both apps. A Mac build is not part of this phase; the same code will look the same there.

### Order of work
* **L1** name pattern and title prompt, shared rules tested on both apps (PL1, PL2); **L2** desktop naming, the setting and "Suggest a name"; **L3** iPhone naming with Apple's model; **L4** first text: the timing test, then the changes (PL3); **L5** model guide and speed test (PL4); **L6** desktop look (PL6); **L7** line editing and the transcript check (PL5); **L8** close-out: verify, review, a desktop release and a TestFlight build.

### Your checklist after Phase L
* Desktop with Gemma 3 1B: record a short made-up conversation and check that the first line appears quickly and the meeting is named in your format; rename one by hand and check that it stays; run "Check transcript" and accept, edit and undo one suggestion; open the model guide and run the speed test. iPhone: a recording is named in the same format.

### Risks
* A 1B model writes weaker names and more false suggestions. The prompt limits it to words that occur in the transcript or notes, and nothing changes without your yes.
* Keeping Parakeet loaded uses memory all the time; the setting and the guide say so.
* Cutting a line mid-sentence at 14 s can split a sentence over two lines. The iPhone already does this, and the desktop's batch paths are unaffected.
* Edited lines no longer match the audio exactly; the original text is kept for that reason.

## Phase N. Thought mode, one-tap record, the phone's meeting list, shorter Settings, a clean history

Status: approved by the owner on 2026-11-03 at the plan gate, with PN1 to PN6 as recommended (PN6 asked again before it runs). Asked for that day after trying the iPhone screens. Covers the desktop and the iPhone where marked. Branch `enhance/quire-phase-n`. Same rules: tests first, one commit per item, independent review, made-up fixtures only.

### What the research found (shapes the plan)
* The iPhone's tabs do not match the spec (F-IOS-UI-2 names Meetings with folders, Record, People): "Transcripts" is a flat list and "Library" opens People. The phone shows the vault's folders but cannot make one.
* Record always opens a sheet first (transcript only, folder, model), on every tap.
* Long dictation is already safe line by line on the iPhone (each line is written to the note as it arrives, I2) and on the desktop (lines go to the browser store as they arrive, audio checkpoints every 30 s, and the recovery dialog offers them after a crash). What is missing is a mode for it: no consent reminder, no speaker detection, and a way to turn the ramble into a prompt.
* GitHub's contributor list comes from the authors in the commit history: 854 commits, most by the upstream project's authors. The repository is not a GitHub fork, so only a new history removes them. `LICENSE.md` keeps the upstream copyright notice, which the MIT licence requires; that stays.
* Several Settings sections open with a paragraph of explanation (retention, vault, models, privacy) before the control.

### Decisions for you at this gate
* **PN1. The phone's tabs.** Recommended: Home, Meetings and People. Meetings shows folders first (with their meeting counts), then unfiled meetings, newest first, with search. People stays as it is under its own name.
* **PN2. Folders on the phone.** Recommended: make, rename and move meetings into folders for the phone's own meetings, written to the vault in the existing folder format (sync-format section 9). Until two-way sync (Phase F) the desktop does not read them back, so a folder made on the phone shows on the phone and in the vault folder only; the screen says so. Alternative: wait for Phase F and keep folders read-only on the phone.
* **PN3. One-tap record.** Recommended: on both apps, tapping record starts at once with the choices you used last (transcript only, folder). The options sheet appears only the first time, when the speech model is missing, or when you ask for it (a long press on the iPhone, the small menu next to the button on the desktop). The consent reminder keeps its own "Never show this again".
* **PN4. Thought mode.** Recommended: a second way to start, "Record my thoughts", on both apps. It is a normal meeting, so it is saved line by line, synced and kept the same way, but: no consent reminder, no speaker detection, named "Thoughts" in your name format, and filed in a folder named Thoughts. When it ends, instead of a meeting summary it offers "Make it a prompt": the local model turns the transcript into one clear, well-ordered prompt (your words and intent kept, filler and repeats removed, nothing added), shown under the transcript with Copy; "Copy transcript" is beside it for pasting into any model you choose. No length limit beyond the device's storage.
* **PN5. Shorter Settings.** Recommended: on both apps, each section keeps one plain sentence and the control; the longer explanation moves behind "More about this", closed by default, never hiding a warning that needs action.
* **PN6. A clean history on GitHub.** Recommended: replace the repository's history with one starting commit of today's code, authored by openedtools with Claude as co-author, then force-push main and delete the older releases and tags so no old commit stays reachable; republish the latest installers as a new release. A full copy of the old history is kept on this Mac outside the repository (a git bundle), in case you ever need it. Upstream credit stays in `LICENSE.md` as the licence requires. GitHub can take a while to refresh its contributor count. This cannot be undone on GitHub once pushed, so it is the last step, after you have seen the new release work. Alternative: keep the history.

### Order of work
* **N1** phone tabs and the Meetings list (PN1); **N2** phone folders (PN2); **N3** one-tap record on both apps (PN3); **N4** thought mode on both apps (PN4); **N5** shorter Settings (PN5); **N6** close-out: verify, review, desktop release, TestFlight build; **N7** the clean history (PN6), only with your yes at that moment.

### Your checklist after Phase N
* iPhone: tap record once and it starts; long press shows the options; record your thoughts for several minutes with the screen locked, then "Make it a prompt" and Copy; make a folder and move a meeting into it. Desktop: the same one-tap start and thought mode; Settings sections are short. GitHub: the contributors page lists only you and Claude (after GitHub refreshes).

### Risks
* One-tap record skips the moment where you see the folder choice; the recording screen shows where it is being saved, and the folder can be changed after.
* A prompt from a 1B model may drop a point; the transcript is always beside it, and the prompt is labelled AI generated.
* Rewriting history breaks any clone or link to an old commit; the only clone is yours on this Mac and your PC (the PC needs a fresh clone afterwards, which the checklist will say).

## Deviations from this plan (recorded as the work proceeds)

| WP | Plan said | What was done | Why |
|---|---|---|---|
| WP2 | Copy the database file with its `-wal` and `-shm` files before the pool opens | `VACUUM INTO` a `.part` file after the pool opens, then rename | One consistent snapshot, includes rows still in the write-ahead log, no raw file pairs to keep in step. Same guarantees, tested including a WAL case |
| WP1 | `.claude/hooks/protect-tests.sh` | `protect-tests.mjs`, run through `node` in exec form, and it also locks the inline `#[cfg(test)] mod` section of Rust files | Rust tests here are inline, so a path-only hook could not protect them. Exec form avoids quoting problems on Windows. Docs agent confirmed the settings keys and that `onFailure: block` needs Claude Code 2.1.295 or later (2.1.316 in use) |
| WP1 | A `lint` runner | Custom checks: dashes (ratchet allowlist), brand drift, brand mentions. `pnpm lint` no longer means `next lint`, which never worked (ESLint is not installed) | Plan baseline finding |
| WP3 | `legacy.rs` and `legacy.ts` modules | Legacy names live in the `legacy` block of `brand/brand.json` and surface as `LEGACY_*` constants in the generated Rust file; TypeScript needs none | One place for the names instead of two hand-written modules |
| WP3 | Move the default recordings folder name to the brand | The default folder name stays the earlier name (`LEGACY_RECORDINGS_DIR_NAME`) until WP8 | The recorder ignores the saved folder preference today, so changing the default early would silently move where adopted installs record. WP8 fixes both together |
| WP3 | `.meetily_decode_` prefix kept as a legacy constant | New prefix `.quire_decode_` (derived from the slug), and the deletion allowlist matches both | Leftovers from the earlier build are still cleaned |
| WP3 | Lint scope not stated | `lint:brand` scans product source, build scripts and configuration. README, docs, CI workflows and the archived backend are not scanned. `brand/allowed-mentions.txt` entries need a reason and a stale entry fails | Upstream documents legitimately describe the upstream product |
| WP3 | n/a | Removed a broken placeholder link (`your-org/meetily`, to a notice file that does not exist) from the Bluetooth playback warning; the macOS aggregate device name is now the neutral `system-audio-tap` | No product name where it was only decoration |
| WP3 | Models by hard link | Models are copied | Review finding: a hard link lets a failed re-download in one app truncate the file the other uses, which breaks "the old folder is never changed". Costs disk space and a minute of copying |
| WP3 | A Yes or No offer, a dismissal counts as No | Three answers: Esc or the window button is not a decision and the person is asked again; only the No button is remembered | Review finding: rfd returns Cancel on dismissal, which would have stored a permanent No |
| WP3 | Not in the plan | Adoption lock file (stale after one hour), refusal of a database from a newer version, retry of the final rename, a failure dialog that offers to open empty or close, folder size and a plain warning in the offer | Review findings on double launch, antivirus holding files on Windows, startup failing on every launch, and an offer that promised more than it did |
| WP2, WP3 | Not in the plan | A database failure at startup shows a window with the reason instead of closing silently; the backup rename retries | Review finding: a release build has no console |
| WP1 | Not in the plan | The test-lock hook normalises line endings and ignores case in paths; `brand:check` ignores line endings | Review findings for Windows checkouts |
| WP3 | n/a | `rfd` added as a direct dependency for Windows and macOS only; Linux adopts without asking | Linux is not a user platform here, and the dialog toolkit is not linked there |
| A1 (Windows) | Adoption reads the old database read-only | When the old app is closed, the old database is opened as immutable | Windows test finding: a read-only connection to a write-ahead-log database created `-wal` and `-shm` files in the old folder and could not remove them. Test added |
| WP4.3 | Remove the `APP_SERVER_URL` CSP entry and `serverAddress` | Also removed `localhost:8178` (old whisper server) from the CSP and the sidebar's `transcriptServerAddress` | Same kind of leftover, set once and never used |
| WP4.4 | Delete the analytics test file in stage (c) | Deleted in stage (a), with the stub | The stub made those tests fail; moving it keeps every commit green |
| WP4.4 | Dead-file list in amendment 7 | Also deleted `lib_old_complex.rs`, an old copy of `lib.rs` that is never compiled | Missed in the list; it was the last place naming the PostHog host |
| WP4.6 | Edit the 3 error strings by hand | The hot-swap errors are now full sentences ("the microphone switch was cancelled") | They can reach the person through the recording error path |
| D2 | A per-request cancel registry in the streaming client | The registry lives with Ask (`ask/commands.rs`); the streaming client takes a cancel token | Ask is the only streaming caller; summaries keep their own registry |
| D2 | Not stated | Claude streaming asks for at most 4,096 answer tokens | The Messages API needs a value; Ask answers are short |
| D3 | Ask runs at lower priority than transcription | Ask is refused while more than 10 s of audio waits to be transcribed, and a running answer stops if the backlog passes 20 s. No operating system priority change | Measurable and testable; the limit is a starting value for the D4 benchmark |
| D3 | A queued built-in request says so | The box shows Thinking... while it waits behind a summary | The queue has no progress events yet; the wait is short in practice |
| D3 | Not stated | Ask uses the model chosen for summaries; notes take at most half the recent budget; a custom server's budget comes from its context size | One model setting to manage; long notes cannot crowd out the transcript |
| D3 | Answers through the sanitizing markdown path | Answers render without raw HTML or images, and only citation jumps are links | An image or link in an answer could make a network request (AC-QA-4) |
| D3 | Ask box under the right pane | Saved meeting: under the My notes and Summary tabs. Live screen: at the bottom of the My notes pane | The live screen's only right-hand pane is My notes |
| D3 | n/a | Fixed: Escape then Tab went back into the same editor. Focus now parks after the editor | Found by the keyboard test once the Ask box sat below the summary |
| D3 review | n/a | Independent review found no blockers. Fixed: model-written jump links, badly formed or nested times kept unchecked, characters split across stream chunks, cut-off or error streams shown as complete, Ollama budgeted at the trained window instead of its 4,096 runtime window, a running answer kept after leaving the meeting, Stop ignored during setup, a backlog that could stick after a worker stopped, error bodies in the log, links and images carried into notes. Each has a test | Review against spec 2.5 and this plan |
| D3 review | n/a | Open: the token estimate (0.35 per character) undercounts Chinese, Japanese and Korean text by two to three times; Ask does not check that Ollama is running before it says ready; cancel within 2 s on the built-in model is untested | To tune or test in D4 |
| D4 | Benchmark each installed model, with Parakeet running | Benchmarked LM Studio with bonsai-27b (Q1, 25,088 window) only, with no recording running. Results below. The built-in model is not reachable from the benchmark, so its timing comes from the "Ask ... first text after" log line in normal use | The only model loaded in LM Studio. The built-in Qwen 3.5 2B was downloaded on 2026-11-02 at the owner's request |
| D4 | Set the budgets and backlog limit from the numbers | Kept 8,000 and 3,000 tokens and the 10 s and 20 s backlog limits | The 27B model spends most of its time thinking before the first visible word, so the numbers say more about the model than the budgets. Revisit with the built-in 2B log lines |
| D5 | The recording start command carries the meeting id to the saver | The window keeps the target meeting (sessionStorage, and the recovery record) and passes it to `api_save_transcript` at save time | The window, not the recorder, saves the meeting row today, so this is the one place it is needed |
| D5 | Recording shown inside the meeting page | Record on a meeting opens the live screen, whose My notes pane edits that meeting's notes in the database; after Stop it returns to the same meeting | Reuses the live screen unchanged; no second live view to keep in step |
| D5 | Not stated | The sidebar's Record button is now "Record now" under the main "New meeting" button | New meeting is the main action |
| D5 | Not stated | The empty-meeting check runs 2 s after leaving the page | Lets notes typed just before leaving finish saving. Closing the app while an empty new meeting is open leaves it in the list |
| D5 | Not stated | A saved recording attaches only to a meeting that still exists and has no recording; otherwise it becomes a new meeting | A transcript is never merged into another recording or lost (F-NM-4) |
| D5 | AC-NM-1: notes focused under 500 ms | About 910 ms on the dev server (warm), measured by the UI test | To check in the installed build, which has no dev compile step |
| D5 | AC-NM-4 by test | Covered by the Rust attach tests and the recovery code path; no end-to-end crash test | A real crash needs the desktop app; on the Windows checklist |
| D5 review | n/a | Independent review: one blocker, fixed. The save read the target from the window, so a recording still being saved could land in a meeting opened later. The target is now taken from the recording's own recovery record, captured when it starts, and Record (meeting page and shortcut) waits until any earlier recording is fully saved. Also fixed: coming back to a new meeting within 2 s no longer removes it; a meeting filed in a folder is kept; a meeting whose Record failed is kept; the stop log no longer prints transcript text or the title. Each has a test | Review against spec 2.12 and this plan |
| D5 review | The Record shortcut test clicked Record now during a recording | The test now starts from idle with a target left behind | Record now is disabled during a recording by the fix above; the reviewer flagged the old flow as unrealistic |
| M2 | `markdown_exports` with cascade on delete | No foreign key; the exporter removes the row after it has dealt with the file | A cascade would drop the row with the meeting, and the app would no longer know which file to remove or whether it was edited |
| M2 | Not stated | A file removed or moved in another app is not recreated; it is listed as "moved or removed" with Replace | Recreating it would undo a deliberate delete or leave two copies after a move in Obsidian |
| M2 | Not stated | A case-only title change keeps the old file name; the title property updates | A rename that differs only in case is unreliable on case-insensitive disks and risks deleting the new file |
| M2 | Changing the folder | A new folder forgets the old one; files there are never touched again, and every meeting is exported to the new folder | Simplest rule that cannot damage the old folder |
| M1 | Remote content neutralised in summary and transcript | Also in My notes | Notes can hold pasted images or HTML from the editor |
| M3 | Detect synced folders | The shared cloud-folder check also knows the macOS locations (`Library/Mobile Documents` for iCloud, `Library/CloudStorage` for OneDrive, Dropbox and Google Drive), which also improves the recordings folder warning | Obsidian iCloud vaults live in `Mobile Documents` |
| M4 review | n/a | Independent review: no blockers. Fixed, each with a test where it can be tested: Replace could delete through a link put in place of a folder (removal now refuses any linked folder); turning the export off or moving it could race a running export (settings changes take the export lock, and the worker re-checks the settings before each meeting); remote content could pass the line scanner in list and indented-code edge cases, Dataview `dataviewjs` fences and `= ...` inline queries could run, and speaker names were not escaped (every body is now checked with a CommonMark parser, `pulldown-cmark`, and falls back to escaping every character; fence languages become `text`; names are escaped); a foreign file with a decomposed Unicode name could be overwritten (names compare after NFC, and new files are placed with a hard link that never replaces); missing re-export after renaming, merging or deleting a person, moving a line to another speaker, speaker detection, retranscription and audio import; the synced-folder line missing on re-enable, focus lost after the first Replace click, and no message when the status cannot be read | Review against spec 2.13 and this plan |
| M4 (Mac) | AC-MD-7: a real Obsidian vault shows the files | Partly done on 2026-11-02: the owner pointed the export at an Obsidian vault in iCloud Drive and a test meeting's transcript appeared in Obsidian. Still to look at: properties, folders, a `\\![[` line, and no network request on opening | Manual check on the Mac |
| M4 review | n/a | Accepted, not changed: an Obsidian autosave in the milliseconds between the hash check and the write can be lost; switching folders A to B to A makes ` (2)` copies in A; case-only renames keep the old file name; very long names can fail to write on some disks; file names follow this computer's time zone; a failed batch waits for the next change or "Export all now"; `\![[` relies on Obsidian honouring the backslash, to confirm in the manual Obsidian check (AC-MD-7) | Low risk or cosmetic, recorded for the owner |
| B1 | The Phase B columns in B2's migration | One migration in B1 adds all Phase B meeting columns (retention, deadline, audio state, finalized, mode, delete requested), and a second adds the retention settings | The two-phase delete needed its marker column in B1; one schema change is easier to review |
| B1 | PB3: known roots | The known-roots rule applies to automatic deletion (sweep, Off, Delete audio now). Deleting a meeting removes that meeting's own marked folder wherever it is, unless another meeting shares it | Otherwise "deleting a meeting never leaves audio behind" (F-RET-12) could not hold for recordings made before a folder change |
| B1 | AC-HK-4: a scripted session's logs searched | A lint (`pnpm lint`, logs) fails on any Rust log call that passes transcript text, titles, notes, names, keys, notification text or a recording folder path; reviewed exceptions carry a `// log-ok:` reason. About 80 calls were fixed or reviewed | Checks every call on every build instead of one session's output |
| B1 | Not planned | The Markdown export worker now runs on Tauri's own runtime and keeps the database with each queued meeting | The new retention tests exposed a worker tied to whichever runtime first scheduled it |
| B2 | F-RET-13: more than a quarter needs confirmation | The quarter rule applies from 4 recordings due at once | A small library would otherwise be asked about every pair of recordings |
| B2 | F-RET-3: Off deletes once the save is confirmed | Also keeps the audio when the meeting has no transcript lines, and says why | A silent or failed transcription would otherwise leave nothing at all |
| B2 | Vault copy removed with the audio (F-RET-5) | Not applicable yet | The vault arrives in Phase E; its `_audio` copies are deleted there |
| B2 | Not stated | A recording saved into a notes-first meeting resets that meeting's retention, so the recording's deadline starts when it is saved | Earlier stamps described a meeting with no audio |
| B3 | A switch carried per recording to the recorder | The switch is a recording preference read when a recording starts; the recording writes its mode into `metadata.json` and the finalizer stores it | Remembered between sessions as F-TO-1 asks, and one path for every way a recording starts |
| B3 | Not stated | The old "Save Audio Recordings" switch set off also counts as transcript only | It already wrote no audio; now the recording is completed and labelled the same way |
| B4 | A step in onboarding for new installs | A one-time notice after onboarding, for new and existing installs alike | One component; it reads the same for everyone |
| B4 | Not stated | The reminder switch moved from Settings, Recordings to the new Privacy tab; the earlier "don't show again" choice carries over | F-CON-3 places it under Privacy |
| B4 | The external reference | The Reporters Committee's Reporter's Recording Guide (https://www.rcfp.org/reporters-recording-guide/), checked live on 2026-11-02; the guidance host allowlist holds only that site | Spec 3.5 asked for a live check |
| B5 review | n/a | Independent review: one blocker, fixed. Under Off, audio kept because the transcript was empty or detection failed still carried a deadline of "now", so the next sweep deleted it after the person was told it was kept. Kept audio is now stored as Keep with no deadline. Also fixed, each with a failing test first: a late finalize by the sweep now treats detect-on-save as not run (so Off keeps the audio) and waits from the latest save, not from when a notes-first meeting was created; finalizing claims the meeting atomically so only one caller can delete; an unreachable folder (a drive not attached, a permission error) is never recorded as "no audio" or "deleted", and a meeting on it stays listed as deleting instead of leaving its files behind; "Delete audio now" needs a finalized recording; shared folders are recognised whatever the path spelling; recording fails closed (no audio) when the preferences cannot be read; the log check now catches file and folder paths, and the meeting paths it found were removed from the logs; the many-at-once prompt reports what actually happened; a clock set back can be resumed by the person; the guidance page no longer states what courts are doing | Review against spec 2.3, 2.4, 2.6 and this plan |
| B5 review | n/a | Accepted, not changed: confirming many deletions passes a count, not the list of meetings; the sweep checks detection and retranscription only when it starts; recordings kept because their folder is shared or outside the recordings folders are not listed in Settings yet; a junction inside a checkpoints folder keeps that meeting pending | Low risk; for a later phase |
| R1 | Remove the legacy import | The database manager's generic `.db` copy path stays | It is not tied to the upstream app and other code uses it |
| R1 | Credit removed in R3 | The About credit and the brand credit value went with R1 | The brand file change removed both at once; the README and release-note credit followed in R3 |
| R2 | Pin each model file by SHA-256 | A separate pin table keyed by file name and exact size, not a new field on every artifact | The artifact struct has 19 literals, most in tests, which stay unchanged; the real files are checked, test fixtures with other sizes are not |
| R3 | The brand lint scans every tracked file except the licence, the notices and migrations | Allowed with reasons: the recordings-folder pin, the lint's own pattern and test, and the planning docs (old identifiers in backticks) | Those names are needed where they are; prose in the planning docs says "the upstream project" |
| R3 | Widen `inScope` | A new `scanned()` scope with its own test; `inScope` and its test are unchanged | Existing tests are not edited |
| R3 | n/a | Build folders default to `cargo-target/quire`, `cargo-target/quire-win` and the `quire-win` worktree | Old local names; needed a clean rebuild once |
| R4 | Delete the listed leftovers | Also deleted the upstream issue and pull request templates, the two update-manifest scripts (the updater is gone), `inject_transcript.py` (it wrote to the old app's data folder) and `NOTIFICATION_TESTING.md`. The `backend/` paths in the Whisper engine's development fallback went too. `.github/force-portable-ggml.cmake` and its check stay (the Windows build uses them). Three import tests that read a sample from the old submodule were already skipped and are left as they are | Same kind of leftover; keeping the Windows build working; tests are not edited |
| R5 review | n/a | Independent review: no blocker, two important findings, both fixed with tests. An adopted install's older meetings keep their audio in the earlier default folder, which had stopped being a folder retention may act in; it is one again while the adoption marker exists. The store hides a failed read of the preferences file behind an empty store, so the pin now reads the file itself, leaves an unreadable or damaged file alone, and writes only when the store agrees with the file; an adopted install whose preferences cannot be read still defaults to the folder it has always used. Also: the model pin test no longer exempts `nemo128.onnx`, stale build-script comments and a tracked upstream log file were removed, and `intent.md` no longer asks for an About credit | Review against spec section 16 and this plan |
| C1 | The core reads `models.json` for every download | Whisper and Parakeet read their pinned address and hash from it; the built-in summary catalogue keeps naming its source at `resolve/main` (existing tests check that) and the downloader takes the pinned address from the manifest | Existing tests are not edited |
| C1 | n/a | A size problem is reported as "too small" or "too large" before the hash, so the existing short-download test still sees its message | Clearer message, existing test unchanged |
| C1 | n/a | The brand lint also catches the earlier company's name, which had survived Phase R in the ffmpeg build addresses | Found by the C1 research |
| C2 | n/a | Folder openers (`open`, `explorer`, `xdg-open` on a local folder) are tagged `none` rather than left out, so every opener is reviewed | One rule for every opener |
| C3 | Keys are migrated and then cleared | Keys of the removed cloud transcription options and the unused Gemini column are cleared without being moved | Review finding: moved, they would sit in the credential store where no screen could remove them |
| C3 | n/a | Credential store entries use the app identifier (`com.openedtools.notes`) as their service, not the name | The identifier never changes with the name, like the data folder |
| C4 | Confirmation for a remote address | Asked only when the address changes to a remote one; saving an already confirmed address again does not ask again | Avoids a question on every save |
| C5 | The window's own catalogues go | The Models page takes sizes, languages and sources from the manifest; the model managers keep their display text (names, descriptions). The Parakeet model the core does not have is removed | The managers' layout depends on that text; a later clean-up can move it |
| C5 | n/a | API transcription is removed from the type of transcript providers too; a stored cloud transcript provider (never selectable) falls back to Parakeet | Part of PC3 |
| C6 review | n/a | Independent review: no blocker, four important findings, all fixed with tests. A saved custom server key is used only with the address it was saved for, and saving a new address without a key removes the old one, so a key never reaches a server it was not meant for. The remote address confirmation applies to a saved Ollama address whatever provider is being saved. The window marks a key saved only when the core accepted it, shows the credential store's reason when it did not, and the key is saved before the provider changes. A model already on disk that does not match its pin is set aside (`.unverified`) and the window says so, instead of being deleted. Also fixed: a key of spaces no longer deletes the saved one; the cloud decision waits when saved keys cannot be read, so a declined Keychain prompt does not lock an existing cloud user out; addresses in logs keep only scheme, host and port; the network lint also covers shells, other HTTP clients, raw connections and opener crates (three more sites tagged); the window's network policy no longer allows an Ollama web host; the ffmpeg binary is pinned on every platform, not only Windows. Accepted, not changed: `*.localhost` counts as this computer (Phase D decision, tested); credential store calls run on async worker threads (short calls); a summary model whose size is in range is not re-hashed when chosen, which the one-time check covers; the network lint does not inventory the window, which makes no requests of its own | Review against the Phase C plan and spec F-MOD |
| E1 | n/a | The sidecar's canonical layout is one segment per line (each a compact JSON object), and `_folder.json` is one compact line; the format fixes the keys, not the spacing, so the canonical writer chose these and the fixtures pin them | Byte-stable rewrites need one exact layout |
| E1 | Unicode simple case folding for folder collisions | Lower-casing of the NFC form | Rust has no simple case folding built in; the two differ only for a few letters (for example `ß`), and the database already refuses folder names that differ only by case |
| E1 | n/a | Front matter parse errors never quote the value | They can reach the log, and a value can be part of a title |
| E3 | Stage outside the vault when on the same volume | Always `_tmp/` inside the vault | The format's fallback; it is always on the same volume, and every reader ignores `_tmp` |
| E3 | n/a | `language` is written as null and `source` is `recorded` when a meeting has transcript lines, otherwise `typed`; `summary.model` is null when the summary does not record its model | The database does not store a meeting's language, whether it was imported, or always the model |
| E3 | n/a | Sidecars that no note references are removed after 7 days, and only if unchanged; a deleted meeting's note goes to `_trash` with its tombstone | Section 7 and section 12, one way |
| E3 | n/a | The vault run is scheduled from the same place as the Markdown export, plus once at launch; each run plans the whole database and writes only what changed | One trigger list for both exports |
| E4 | n/a | Choosing iCloud Drive creates the `Quire` folder inside it if it is missing, after the disclosure | The suggested folder does not exist on a first run |
| E5 review | n/a | Independent review: two blockers and six important findings, all fixed with tests. Blockers: a run that stopped part way could leave a changed note recorded as "changed in the vault" for good (each write is now recorded as pending first, and `rev` and `updated` stay the same for the same content across retries); a folder renamed only in letter case deleted its notes on case-insensitive disks (paths now compare case-folded, and the directory is renamed in place). Important: an old file is removed only once its replacement is in place, and a note is written only after its sidecar; a file that exists but cannot be read is kept on record; deleted meetings leave `_trash` after 30 days and tombstones after a year; `rev` never goes back when a folder is chosen again (the app recognises its own files by ID and device); people and folder changes schedule a run; notes carry `legacy_id`. Also fixed: a lost request in the scheduler, a retry a minute after an unverified run, removal checks every part of each path and also takes `_trash` and old sidecars, tombstones never write over another file and record the folder's ID, the grammar rejects `\\u+041`, raw control characters and a non-integer `duration_s`, the Markdown export also refuses a folder inside the vault, and removal is offered after turning the vault off. Accepted: lower-casing instead of simple case folding; file work runs on the async worker and holds the vault lock during a large first export; parser errors from the app's own files are not logged in detail | Review against the Phase E plan and `sync-format.md` |
| K1 | A rule that marks migrations as never converted | `text`: stored with LF, checked out with each system's own endings | Your PC checks migrations out with CRLF and its database was created from those bytes; `-text` would have given a PC checkout LF and broken a build made there |
| K2 | Confirm by the list of meetings | The window still sends a count (an existing test pins it); the core remembers exactly which meetings it asked about and a yes covers only those | Same guarantee, existing test unchanged |
| K2 | Fix the compiler warnings | Fixed all but four dead-code warnings whose code is used only on other platforms or kept for the next step | Removing them could break the Windows build |
| K4 | Switch the speaker model if clearly better | Kept WeSpeaker ResNet34. Measured with the opt-in accuracy check on 6 AMI test meetings (4 speakers each, about 3 hours, CC BY 4.0, kept outside the repository): 54 percent of lines right with the speaker count given, against 39 percent for 3D-Speaker CAM++ (feature normalisation checked: the per-clip mean removal makes the two input scales equivalent). The Detect dialog already asks for the number of people before detection and suggests it again afterwards | The new model was worse, not better |
| K5 review | n/a | Independent review: no blocker, five important findings, fixed with tests. The app keeps trying for the lock after waiting for a long scheduled check, so a later check never sweeps beside the open app; a yes now carries the exact meetings the window showed (the K2 count path stays only for an older window); a vault turned off during an export stops before any clean-up and records nothing; the daily task is defined from XML so it runs after a missed start and on battery, per Windows account; Settings tells "wanted" from "set up", so a refused task is never described as off. Also: the task is changed only at start-up and on a settings change, only deadlines still ahead keep it, and the line-ending test reads what git stores. Accepted: the uninstaller leaves the task (it then fails quietly with the program gone); the kept list checks folders on each Settings visit | Review against the Phase K plan |
| L2 | "Suggest a name" on the meeting | On the desktop it sits in the rename box and fills in a suggestion you then save; on the iPhone it is in the meeting's More menu and asks "Use this name?" | A name is only ever replaced by the person's own click |
| L2 | Named when a recording or import is finalized | Recordings are named; an import keeps its file name, which counts as a name you chose | A file name cannot be told apart from a typed name; renaming it would break PL1's promise |
| L4 | Feed the voice detector in smaller steps if the measurement shows it helps | Unchanged at 600 ms. Measured on made-up speech from macOS `say` (`first_line_timing`, opt-in): with pauses the first line came after the first pause at 5.0 s with 600 ms or 200 ms steps alike, about 1.3 s after the pause ends; with no pause there was no line until the speech ended, now one at 14.6 s. Loading Parakeet took about 1.0 s at every record start and now happens once when the app opens; the first line then transcribes in 0.2 s | No gain, and shorter mixer windows would pad a lagging stream with silence more often |
| L4 | A timing test with a made-up recording | The made-up speech is spoken by macOS `say` when the test runs, so no audio file is committed; the cap test skips where there is no `say` | No recording in the repository |
| L5 | Recommend by memory and graphics | The Windows built-in engine is built for the processor only, so with an NVIDIA card the guide names a small built-in model and points to Ollama or LM Studio for the card. Memory is read with `sysinfo` (the old hardware check assumed 8 GB) | Found while planning the guide |
| L6 | Folder cards and coloured speaker circles | Not on the desktop: it has no folder page (folders stay in the sidebar), and the speaker chips are the control that assigns people, so they keep their names | Both would have been new desktop features, not a restyle |
| L6 | A mirrored waveform | Needed a level reading during recording: `api_recording_level`, a number from 0 to 1 polled every 100 ms, with no audio kept | The level stream existed only for the device test |
| L8 review | n/a | Independent review: one real bug and smaller ones, all fixed with tests. Accepting two suggestions on one line kept only the second: a suggestion now applies to the line as it is now (`api_apply_suggestion`), and other cards follow. A match ignoring case now lands on the right characters in non-English text. The check sends only this meeting's speakers and the known names already in its transcript or notes, not all of People. A failed automatic name logs only the kind of error, never a server's text. No length cap on the title call (a thinking model was cut off). The speed test loads the model before timing, and says three minutes of talk, which is what the sample is. The phone's default name has one fixed shape ("Recording, 3 Nov 2026 14:05"), so a typed name like "Recording session 2026 rehearsal 10:30" is never renamed. Also: no preload panic on a slow start, a suggestion fills only the rename box it was asked for, the VAD fallback skips the right samples after a cut, and the check's buttons and the Edited mark are named for screen readers | Review against the Phase L plan |
| N1 | Tabs Home, Meetings and People | As planned. The existing UI test that opened People by the tab's old name ("Library") now uses "People" | The tab's new name |
| N2 | Make, rename and move folders | As planned; only folders this phone made can be renamed here (`updated_by` is this phone), so the desktop's folders are renamed on the desktop | A rename here would be undone by the desktop's next export |
| N3 | One tap on both apps, the menu next to the button on the desktop | iPhone as planned (long press, Recording options, an accessibility action). The desktop already started on one click, so nothing changed there. The existing reminder test's second recording now starts with one tap | Found while building |
| N4 | A prompt under the transcript | The prompt is the meeting's summary, made with a new built-in template on the desktop ("Prompt from my thoughts") and with the same instructions on the iPhone, so it is saved, exported and synced like any summary; the tab is named Prompt on the iPhone and the button Make it a prompt on both. The note carries `kind: thoughts` (sync-format section 6); the desktop keeps it in a new `meetings.kind` column and does not write it to the vault yet | Reuses the summary path instead of a new section in the format |
| N5 | One sentence and More about this | As planned on both apps; statements about privacy or a needed action stay in view (audio never goes into the vault, a model that cannot run, a device locked during recording). The iPhone retention test opens More about this before checking the same limits | A warning is never folded away |
| N6 review | n/a | Independent review: three serious findings and several smaller ones, fixed with tests. A failed or other start no longer leaves the thoughts flag for the next recording on the desktop (it skipped the consent reminder for a real meeting). Renaming a folder on the phone now moves the phone's records of its notes too, and edits and moves find a note again if it moved. The desktop's vault export no longer writes into a folder another device made (both apps make "Thoughts"): such a directory counts as taken, so the desktop's own gets " (2)". The first thoughts recording on the phone is filed in the folder just made; one tap keeps the last folder while the vault is still being read; a note never moves without its transcript; a rename in letter case only goes through a temporary name; the desktop writes `kind: thoughts` into its notes too; what deletion cannot reach stays in view in Settings. Left for two-way sync (Phase F): the phone can file its notes in a desktop folder (as since I3), and if the desktop then renames or deletes that folder, the phone's note stays in a directory with no identity | Review against the Phase N plan |
| N7 | One starting commit, older releases and tags deleted | Done on 2026-11-03 with the owner's yes: main is one commit (dce18c9, same tree as 82a4d77) by openedtools with Claude as co-author; every other branch, every old release and tag removed. The latest installers were republished unchanged as quire-v0.4.1-dce18c9. The FFmpeg build inputs release keeps its tag name (the build downloads from it), now pointing at the new commit. The old history is in a git bundle in `~/Developer/backups`, outside the repository | Upstream credit stays in `LICENSE.md` |

### D4 benchmark (2026-11-02, LM Studio, bonsai-27b Q1, window 25,088, budgets 8,000 and 3,000)

| Ask benchmark: synthetic meeting, question "What was agreed about the venue?" | | | |
|---|---|---|---|
| Meeting | Context tokens | First text | Total |
| 30 min | 7,773 | 66.2 s | 68.3 s |
| 60 min | 10,872 | 55.6 s | 56.0 s |
| 120 min | 10,865 | 98.6 s | 99.3 s |

Too slow for live use. First text and total are close together, so most of the time goes to prompt reading and hidden thinking, not to the answer. For Ask during a meeting use the built-in Qwen 3.5 2B or a small model in LM Studio.
