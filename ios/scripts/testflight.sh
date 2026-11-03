#!/bin/bash
# Builds the iPhone app and uploads it to TestFlight (internal testing), from this Mac with no one
# at it. Signing is automatic, through an App Store Connect API key.
#
# Account details never enter the repository. They live in ~/.config/quire/testflight.env:
#   TEAM_ID=...            the Apple Developer team ID
#   ASC_KEY_ID=...         the App Store Connect API key ID
#   ASC_ISSUER_ID=...      the issuer ID shown above the keys
#   ASC_KEY_PATH=...       the .p8 file, kept outside the repository with mode 600
#
# Usage: ios/scripts/testflight.sh            build and upload
#        ios/scripts/testflight.sh --check    build and sign only, no upload
set -euo pipefail

here="$(cd "$(dirname "$0")/.." && pwd)"
settings="${QUIRE_TESTFLIGHT_ENV:-$HOME/.config/quire/testflight.env}"
work="${QUIRE_BUILD_DIR:-$HOME/Developer/cargo-target}/testflight"
upload=1
[[ "${1:-}" == "--check" ]] && upload=0

[[ -f "$settings" ]] || { echo "Missing $settings (see the top of this script)." >&2; exit 1; }
# shellcheck disable=SC1090
source "$settings"
for name in TEAM_ID ASC_KEY_ID ASC_ISSUER_ID ASC_KEY_PATH; do
  [[ -n "${!name:-}" ]] || { echo "$name is not set in $settings" >&2; exit 1; }
done
[[ -f "$ASC_KEY_PATH" ]] || { echo "The API key file named in $settings is missing." >&2; exit 1; }
case "$ASC_KEY_PATH" in "$here"/*|"$(dirname "$here")"/*) echo "The API key must live outside the repository." >&2; exit 1;; esac
[[ "$(stat -f %Lp "$ASC_KEY_PATH")" == "600" ]] || { echo "The API key file must have mode 600." >&2; exit 1; }

# Each upload needs a higher build number: the commit count only grows
build="$(git -C "$here" rev-list --count HEAD)"
if [[ -n "$(git -C "$here" status --porcelain -- .)" ]]; then
  echo "Note: ios/ has uncommitted changes; the build includes them." >&2
fi

auth=(-allowProvisioningUpdates -authenticationKeyPath "$ASC_KEY_PATH" -authenticationKeyID "$ASC_KEY_ID" -authenticationKeyIssuerID "$ASC_ISSUER_ID")
rm -rf "$work" && mkdir -p "$work"

(cd "$here" && xcodegen -q)
echo "Archiving build $build"
xcodebuild archive -project "$here/Quire.xcodeproj" -scheme Quire -configuration Release \
  -destination 'generic/platform=iOS' -archivePath "$work/Quire.xcarchive" \
  -derivedDataPath "$work/dd" -clonedSourcePackagesDirPath "${QUIRE_BUILD_DIR:-$HOME/Developer/cargo-target}/ios-spm" \
  -disableAutomaticPackageResolution \
  DEVELOPMENT_TEAM="$TEAM_ID" CODE_SIGN_STYLE=Automatic CURRENT_PROJECT_VERSION="$build" \
  "${auth[@]}" > "$work/archive.log" 2>&1 || { grep -E "error:" "$work/archive.log" | sort -u | head -20; echo "Archive failed; full log: $work/archive.log" >&2; exit 1; }

destination=export
[[ $upload == 1 ]] && destination=upload
cat > "$work/ExportOptions.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>method</key><string>app-store-connect</string>
  <key>destination</key><string>$destination</string>
  <key>teamID</key><string>$TEAM_ID</string>
  <key>signingStyle</key><string>automatic</string>
  <key>uploadSymbols</key><true/>
  <key>manageAppVersionAndBuildNumber</key><false/>
  <key>testFlightInternalTestingOnly</key><true/>
</dict>
</plist>
PLIST

echo "$([[ $upload == 1 ]] && echo Uploading || echo Exporting) build $build"
xcodebuild -exportArchive -archivePath "$work/Quire.xcarchive" -exportOptionsPlist "$work/ExportOptions.plist" \
  -exportPath "$work/export" "${auth[@]}" > "$work/export.log" 2>&1 || { grep -E "error:" "$work/export.log" | sort -u | head -20; echo "Export failed; full log: $work/export.log" >&2; exit 1; }
rm -f "$work/ExportOptions.plist"
if [[ $upload == 1 ]]; then
  echo "Uploaded build $build. TestFlight shows it after Apple processes it (usually 5 to 30 minutes)."
else
  echo "Signed and exported build $build to $work/export (not uploaded)."
fi
