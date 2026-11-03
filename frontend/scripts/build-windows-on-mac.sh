#!/usr/bin/env bash
# Cross-builds the Windows installer on a Mac from the current commit. The PC's
# build_installer_windows.bat stays the reference build; this one cannot be run or tested here.
# One-time setup and the reasons for each setting are in CLAUDE.md (macOS section).
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
WORK="${QUIRE_WIN_WORKTREE:-$HOME/Developer/quire-win}"
TARGET=x86_64-pc-windows-msvc
BREW="$(brew --prefix)"
XWIN="$HOME/Library/Caches/cargo-xwin/xwin"

export PATH="$BREW/opt/llvm/bin:$BREW/opt/lld/bin:$BREW/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR="${QUIRE_WIN_TARGET_DIR:-$HOME/Developer/cargo-target/quire-win}"
# The home-folder remap keeps this computer's user name out of the binaries
export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS="-C target-cpu=x86-64-v2 --remap-path-prefix=$HOME=~"
# New Clang rejects pointer casts in llama.cpp that Microsoft's compiler accepts
export CFLAGS="-Wno-incompatible-pointer-types"
# clang-cl takes the path remap only through /clang:, which the Mac's own compiler would misread;
# cargo-xwin passes CL_FLAGS to the Windows compiler only
export CL_FLAGS="/clang:-ffile-prefix-map=$HOME=~"
# bindgen 0.69 with Homebrew's newer libclang leaves whisper_full_params empty
export LIBCLANG_PATH=/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib
export BINDGEN_EXTRA_CLANG_ARGS_x86_64_pc_windows_msvc="--target=$TARGET -isystem $XWIN/crt/include -isystem $XWIN/sdk/include/ucrt -isystem $XWIN/sdk/include/um -isystem $XWIN/sdk/include/shared"
# makensis crashes without a UTF-8 locale
export LC_ALL=en_US.UTF-8 LANG=en_US.UTF-8

commit="$(git -C "$REPO" rev-parse --short HEAD)"
echo "Preparing a build copy of $commit in $WORK"
if [ -d "$WORK" ]; then
  git -C "$REPO" worktree remove --force "$WORK"
fi
git -C "$REPO" worktree add --detach "$WORK" HEAD >/dev/null
export CMAKE_PROJECT_INCLUDE="$WORK/.github/force-portable-ggml.cmake"

# The PC checks migrations out with CRLF and sqlx checksums the bytes, so the Windows app must
# embed CRLF copies or it refuses an existing database. Never committed.
python3 -I - "$WORK/frontend/src-tauri/migrations" <<'EOF'
import pathlib, sys
for p in sorted(pathlib.Path(sys.argv[1]).glob("*.sql")):
    data = p.read_bytes().replace(b"\r\n", b"\n")
    p.write_bytes(data.replace(b"\n", b"\r\n"))
EOF

cd "$WORK"
# The speech and language engines cache their CMake set-up; start them fresh so the flags above
# (the path remap in particular) always reach every C and C++ file
rm -rf "$CARGO_TARGET_DIR/$TARGET/release/build"/llama-cpp-sys-2-* "$CARGO_TARGET_DIR/$TARGET/release/build"/whisper-rs-sys-*
cargo xwin build --release -p llama-helper --target "$TARGET"
mkdir -p frontend/src-tauri/binaries
cp "$CARGO_TARGET_DIR/$TARGET/release/llama-helper.exe" "frontend/src-tauri/binaries/llama-helper-$TARGET.exe"

cd frontend
pnpm install --frozen-lockfile
# The signing script needs PowerShell; on the PC it skips signing anyway, so the result matches
pnpm tauri build --runner cargo-xwin --target "$TARGET" --bundles nsis -c '{"bundle":{"windows":{"signCommand":null}}}'

installer="$(ls "$CARGO_TARGET_DIR/$TARGET/release/bundle/nsis/"*-setup.exe)"
out="$HOME/Desktop/$(basename "${installer%.exe}")-$commit.exe"
cp "$installer" "$out"
echo
echo "Windows installer: $out"
shasum -a 256 "$out"
