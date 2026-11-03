#!/usr/bin/env bash
# Builds the Mac and Windows installers from the current commit and publishes them as the latest
# GitHub release, with stable file names so these links always fetch the newest build:
#   https://github.com/openedtools/Quire/releases/latest/download/Quire-mac-arm64.dmg
#   https://github.com/openedtools/Quire/releases/latest/download/Quire-windows-x64-setup.exe
# The release also carries FFmpeg's licence, its matching source code and build notes, because
# both installers bundle a GPL build of FFmpeg.
# Usage: frontend/scripts/release.sh [--windows-tested]   (set the flag after testing on the PC)
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
FFMPEG_SRC="${QUIRE_FFMPEG_SRC:-$HOME/Developer/cargo-target/ffmpeg-src}"
MAC_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/Developer/cargo-target/quire}"
WIN_TARGET_DIR="${QUIRE_WIN_TARGET_DIR:-$HOME/Developer/cargo-target/quire-win}"
WINDOWS_TESTED=no
[ "${1:-}" = "--windows-tested" ] && WINDOWS_TESTED=yes

cd "$REPO"
if [ -n "$(git status --porcelain)" ]; then
  echo "Commit or stash your changes first: a release is built from a commit." >&2
  exit 1
fi
commit="$(git rev-parse HEAD)"
short="$(git rev-parse --short HEAD)"
git fetch -q origin
if [ -z "$(git branch -r --contains "$commit")" ]; then
  echo "Push $short first: the release tag points at a commit on GitHub." >&2
  exit 1
fi
version="$(node -p "require('./frontend/src-tauri/tauri.conf.json').version")"
tag="quire-v$version-$short"
for f in ffmpeg-8.0.tar.xz ffmpeg-8.0.1.tar.xz; do
  [ -f "$FFMPEG_SRC/$f" ] || { echo "Missing $FFMPEG_SRC/$f (download it from https://ffmpeg.org/releases/)" >&2; exit 1; }
done

BREW="$(brew --prefix)"
export PATH="$BREW/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
# Binaries must not carry this computer's home folder (it names the owner)
export RUSTFLAGS="--remap-path-prefix=$HOME=~"
export CFLAGS="-ffile-prefix-map=$HOME=~" CXXFLAGS="-ffile-prefix-map=$HOME=~"
(cd frontend && node scripts/lint/privacy.mjs)

echo "== Mac build of $short"
CARGO_TARGET_DIR="$MAC_TARGET_DIR" cargo build --release -p llama-helper --features metal
mkdir -p frontend/src-tauri/binaries
cp "$MAC_TARGET_DIR/release/llama-helper" frontend/src-tauri/binaries/llama-helper-aarch64-apple-darwin
(cd frontend && pnpm install --frozen-lockfile && CARGO_TARGET_DIR="$MAC_TARGET_DIR" pnpm tauri:build)

echo "== Windows build of $short"
# RUSTFLAGS would override the Windows target flags (portable CPU, path remap) set by that script
env -u RUSTFLAGS -u CXXFLAGS QUIRE_WIN_TARGET_DIR="$WIN_TARGET_DIR" frontend/scripts/build-windows-on-mac.sh

out="$HOME/Developer/cargo-target/release/$tag"
rm -rf "$out"
mkdir -p "$out"
cp "$(ls "$MAC_TARGET_DIR"/release/bundle/dmg/*.dmg)" "$out/Quire-mac-arm64.dmg"
cp "$(ls "$WIN_TARGET_DIR"/x86_64-pc-windows-msvc/release/bundle/nsis/*-setup.exe)" "$out/Quire-windows-x64-setup.exe"
cp "$FFMPEG_SRC/ffmpeg-8.0.tar.xz" "$out/ffmpeg-8.0-source.tar.xz"
cp "$FFMPEG_SRC/ffmpeg-8.0.1.tar.xz" "$out/ffmpeg-8.0.1-source.tar.xz"
tar -xJOf "$FFMPEG_SRC/ffmpeg-8.0.tar.xz" ffmpeg-8.0/COPYING.GPLv2 > "$out/FFMPEG-LICENSE-GPLv2.txt"
tar -xJOf "$FFMPEG_SRC/ffmpeg-8.0.1.tar.xz" ffmpeg-8.0.1/COPYING.GPLv3 > "$out/FFMPEG-LICENSE-GPLv3.txt"
mac_config="$(frontend/src-tauri/binaries/ffmpeg-aarch64-apple-darwin -version 2>&1 | grep '^configuration:' || true)"

# Refuse to publish an installer that still names this computer's home folder
leak_check="$(mktemp -d)"
hdiutil attach -quiet -readonly -nobrowse -mountpoint "$leak_check/mac" "$out/Quire-mac-arm64.dmg"
mac_leaks="$(find "$leak_check/mac" -type f -perm +111 -exec strings {} \; 2>/dev/null | grep -c "$HOME" || true)"
hdiutil detach -quiet "$leak_check/mac"
7zz x -y -o"$leak_check/win" "$out/Quire-windows-x64-setup.exe" >/dev/null
win_leaks="$(find "$leak_check/win" -type f \( -name '*.exe' -o -name '*.dll' \) -exec strings {} \; 2>/dev/null | grep -c "$HOME" || true)"
rm -rf "$leak_check"
if [ "$mac_leaks" != "0" ] || [ "$win_leaks" != "0" ]; then
  echo "Not publishing: the installers still contain this computer's home folder ($mac_leaks Mac, $win_leaks Windows)." >&2
  exit 1
fi

cat > "$out/FFMPEG-NOTICE.txt" <<EOF
FFmpeg in Quire

Quire runs FFmpeg as a separate program for audio conversion. Quire's own code is under the MIT
licence; FFmpeg is not part of it and keeps its own licence. FFmpeg is a trademark of Fabrice Bellard.

Mac installer (Quire-mac-arm64.dmg)
  FFmpeg 8.0, a third-party static build for Apple silicon, configured with --enable-gpl, so it is
  distributed under the GNU General Public License version 2 or later (FFMPEG-LICENSE-GPLv2.txt).
  Source code: ffmpeg-8.0-source.tar.xz (from https://ffmpeg.org/releases/).
  $mac_config

Windows installer (Quire-windows-x64-setup.exe)
  FFmpeg 8.0.1, the "essentials" build from https://www.gyan.dev/ffmpeg/builds/, distributed under the
  GNU General Public License version 3 (FFMPEG-LICENSE-GPLv3.txt).
  Source code: ffmpeg-8.0.1-source.tar.xz (from https://ffmpeg.org/releases/).

Both builds link further open-source libraries (for example x264, x265, libvpx, opus, lame), listed in
the configuration above and on the builders' pages, whose source is published by each project. For
three years from this release, a copy of the complete corresponding source for either build is
available on request: open an issue at https://github.com/openedtools/Quire/issues.
EOF

(cd "$out" && shasum -a 256 Quire-mac-arm64.dmg Quire-windows-x64-setup.exe ffmpeg-8.0-source.tar.xz ffmpeg-8.0.1-source.tar.xz > SHA256SUMS.txt)

previous="$(git tag --list 'quire-v*' --sort=-creatordate | head -1)"
if [ -n "$previous" ]; then
  changes="$(git log --format='- %s' "$previous..$commit")"
else
  changes="$(git log --format='- %s' -15 "$commit")"
fi
if [ "$WINDOWS_TESTED" = yes ]; then
  windows_line="The Windows installer is cross-built on a Mac and has been tested on Windows."
else
  windows_line="The Windows installer is cross-built on a Mac and has not been tested on Windows yet."
fi

cat > "$out/NOTES.md" <<EOF
Quire $version, built from commit $short on $(date -u +%Y-%m-%d).

**Downloads**
- Mac (Apple silicon): Quire-mac-arm64.dmg
- Windows (64-bit): Quire-windows-x64-setup.exe

Neither installer is code-signed. On a Mac, right-click Quire, choose Open, then Open again. On
Windows, choose More info, then Run anyway. $windows_line

For personal use only. Recording and transcription rules differ by
place: tell people when you record them and get a clear yes. This is not legal advice.

Quire runs FFmpeg as a separate program
under the GNU GPL; the FFmpeg licence, the matching source code and build notes are attached
(FFMPEG-NOTICE.txt). Checksums: SHA256SUMS.txt.

**Changes in this build**
$changes
EOF

echo "== Publishing $tag"
gh release create "$tag" --repo openedtools/Quire --target "$commit" --latest \
  --title "Quire $version ($short)" --notes-file "$out/NOTES.md" \
  "$out/Quire-mac-arm64.dmg" "$out/Quire-windows-x64-setup.exe" "$out/SHA256SUMS.txt" \
  "$out/FFMPEG-NOTICE.txt" "$out/FFMPEG-LICENSE-GPLv2.txt" "$out/FFMPEG-LICENSE-GPLv3.txt" \
  "$out/ffmpeg-8.0-source.tar.xz" "$out/ffmpeg-8.0.1-source.tar.xz"
git fetch -q --tags origin
echo
echo "Published: https://github.com/openedtools/Quire/releases/tag/$tag"
echo "Latest Mac link:     https://github.com/openedtools/Quire/releases/latest/download/Quire-mac-arm64.dmg"
echo "Latest Windows link: https://github.com/openedtools/Quire/releases/latest/download/Quire-windows-x64-setup.exe"
