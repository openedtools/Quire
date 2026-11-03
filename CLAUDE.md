# CLAUDE.md

Private, local-first meeting recorder. Tauri 2 desktop app: Rust core in `frontend/src-tauri`, Next.js 14 static-export UI in `frontend/src`. Working name Quire. Personal use only.

Read `intent/private-meeting-recorder/` before changing behaviour: `intent.md` (why), `spec.md` (what, with acceptance criteria), `sync-format.md` (shared vault), `plan.md` (how, per phase). No product code without an accepted plan.

## Commands

Frontend, from `frontend/`:
- `pnpm install`, `pnpm dev` (port 3118), `pnpm tauri:dev`, `pnpm tauri:build`
- `pnpm typecheck` (tsc), `pnpm lint` (custom repo checks), `pnpm test`, `pnpm verify` (all three)
- Windows installer: `frontend\build_installer_windows.bat`
- Release (Mac): `frontend/scripts/release.sh [--windows-tested]` builds both installers from the pushed commit and publishes the latest GitHub release with stable names (`Quire-mac-arm64.dmg`, `Quire-windows-x64-setup.exe`) plus FFmpeg's GPL licence, source and notes. Publish after each verified phase or when the owner asks.

Rust, from `frontend/src-tauri/`:
- `cargo check --lib`, `cargo test --lib`
- Linux needs `ORT_STRATEGY=system` and `ORT_LIB_LOCATION` plus `LD_LIBRARY_PATH` pointing at a directory holding an ONNX Runtime shared library (with `.so.1` and versioned symlinks for tests; an empty directory is enough for `cargo check`), and placeholder `binaries/llama-helper-<target>` and `binaries/ffmpeg-<target>` files (gitignored). Keep the cargo target dir outside the repo (`CARGO_TARGET_DIR`).
- macOS (Apple silicon, set up 2026-11-02 on macOS 26.6 with Xcode 26.6):
  - `xcode-select -p` must point at `/Applications/Xcode.app`. `brew install rustup cmake`, then `rustup default stable` and put `$(brew --prefix)/opt/rustup/bin` on `PATH`.
  - `npm install -g pnpm@10 bun@1` (the Homebrew bun tap refuses to install while the Command Line Tools are older than Xcode). Then `pnpm install` and, once, `pnpm exec playwright install chromium`.
  - `export CARGO_TARGET_DIR=~/Developer/cargo-target/quire`. No ONNX Runtime variables: `ort` downloads and links its own runtime.
  - Sidecars: from the repo root, `cargo build --release -p llama-helper --features metal`, then copy `$CARGO_TARGET_DIR/release/llama-helper` to `frontend/src-tauri/binaries/llama-helper-aarch64-apple-darwin`. `build.rs` downloads `binaries/ffmpeg-aarch64-apple-darwin` itself (not hash-pinned; a GPL build).
  - `pnpm tauri:build` writes `$CARGO_TARGET_DIR/release/bundle/macos/Quire.app` and a `.dmg`, ad-hoc signed.
- Windows installer cross-built on the Mac (2026-11-02; the PC's `build_installer_windows.bat` stays the reference):
  - Once: `rustup target add x86_64-pc-windows-msvc`, `brew install llvm lld ninja nsis`, `cargo install cargo-xwin --locked` (it downloads Microsoft's CRT and Windows SDK, under Microsoft's licence).
  - Build in a separate worktree whose `migrations/*.sql` are converted to CRLF and never committed. The PC checks them out as CRLF and sqlx checksums the bytes, so an LF build refuses an existing Windows database.
  - Environment: llvm and lld bins on `PATH`, a separate `CARGO_TARGET_DIR`, `CMAKE_PROJECT_INCLUDE=<worktree>/.github/force-portable-ggml.cmake`, `CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS="-C target-cpu=x86-64-v2"`, `CFLAGS=-Wno-incompatible-pointer-types` (new Clang rejects llama.cpp code that MSVC accepts), `LIBCLANG_PATH=<Xcode toolchain>/usr/lib` (bindgen 0.69 with Homebrew's libclang leaves `whisper_full_params` empty).
  - `frontend/scripts/build-windows-on-mac.sh` does all of this from the current commit (also `LC_ALL=en_US.UTF-8`, without which `makensis` crashes, and no sign command, which needs PowerShell) and copies the installer to the Desktop. It cannot be run or tested on the Mac.

## Conventions

- **No personal data, ever** (the repository is public). No real meeting notes, transcripts, people, names, emails, accounts, devices, home-folder paths, or details of the owner's work or location, in any file, commit message, screenshot or release. Test data is clearly fictional. Commit as `openedtools <67639231+openedtools@users.noreply.github.com>`. `pnpm lint` (privacy) checks emails, home folders and the owner's private deny list (`~/.config/quire/privacy-deny.txt`, never committed). When something might come close, stop and ask the owner first.
- Rust: `anyhow::Result`, `perf_debug!` for hot paths, log module context but never transcript text, names or keys. Register commands in `src/lib.rs`. Use Tauri path APIs, never hard-coded paths.
- Naming: "microphone" and "system" for audio sources.
- No em dashes in code, text or docs (`pnpm lint` enforces it).
- Rust tests are inline `#[cfg(test)]` modules. Frontend tests are in `frontend/tests` (bun, plus one node test file).
- Branches: `enhance/*` for features, `fix/*` for bugs. One commit per work package.

## Architecture in five lines

- `audio/pipeline.rs` splits mic and system audio into a mixed recording path and a VAD-filtered transcription path (Whisper via whisper-rs, Parakeet via ort).
- `diarization/` detects speakers after the fact from saved audio; `people/` and `folders/` hold the directory and organization; `database/` is SQLite (sqlx, `migrations/`).
- `summary/` chunks the transcript and calls a provider (BuiltInAI sidecar, Ollama, or a cloud provider with a user key).
- The UI talks to Rust only through Tauri commands and events.
- Recordings are files (audio plus `metadata.json` and `transcripts.json` per meeting folder); everything else is in SQLite.

## Things Claude gets wrong

- Never edit an applied migration in `src-tauri/migrations/` (sqlx checks checksums). Add a new one.
- Run frontend tests with `pnpm test`. Plain `bun test` leaks `mock.module` between files and shows false failures; `--isolate` fixes it.
- The UI is a static export: no server routes, pass ids as `?id=`.
- `api/recording_files.rs::delete_recording_files` removes transcripts and the folder too. Do not use it to delete only audio.
- `pkill -f <pattern>` can kill your own shell. Find the pid with `ss -ltnp` (macOS: `lsof -nP -iTCP:3118 -sTCP:LISTEN`) and kill that.
- The first Next.js dev run compiles cold and can time out a UI test. Rerun when warm.
- Do not use em dashes, even in comments.

## Verifying your work

Before reporting any task complete, run these and paste the output:
1. `cd frontend && pnpm verify`
2. When Rust changed: `cd frontend/src-tauri && cargo check --lib && cargo test --lib` with the environment above.
3. When the UI changed: the Playwright screenshots, reviewed.

If a test fails, fix the code, not the test. For a bug: write a failing test first, run it and see it fail for the expected reason, commit it, then fix with `LOCK_TESTS=1` set so the hook blocks edits to existing tests. A phase is not done until an independent reviewer (a subagent or fresh session that did not see the build) has reviewed the diff against `spec.md` and `plan.md`.
