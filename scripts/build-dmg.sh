#!/usr/bin/env bash
# Universal team release. Without NOTARY_PROFILE the artifact is explicitly
# marked unnotarized. Credentials, when available, stay in the Keychain.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
: "${DEVELOPER_ID:?Set DEVELOPER_ID to the team Developer ID Application identity}"
export ARCHS="arm64 x86_64"
"$ROOT/scripts/build-dev.sh"
APP="$ROOT/.build/xcode/Build/Products/Release/Everyseek.app"
DIST="$ROOT/dist"
mkdir -p "$DIST"
VERSION=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP/Contents/Info.plist")
NAME="Everyseek-${VERSION}-universal"
if [[ -n "${NOTARY_PROFILE:-}" ]]; then
  UPLOAD="$(mktemp -d)"
  ditto -c -k --keepParent "$APP" "$UPLOAD/Everyseek.zip"
  xcrun notarytool submit "$UPLOAD/Everyseek.zip" --keychain-profile "$NOTARY_PROFILE" --wait
  rm -rf "$UPLOAD"
  xcrun stapler staple "$APP"
  xcrun stapler validate "$APP"
  spctl --assess --type execute --verbose=2 "$APP"
else
  NAME="$NAME-unnotarized"
  echo "No NOTARY_PROFILE: producing a Developer ID signed, unnotarized release."
fi
codesign --verify --strict "$APP"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
ditto "$APP" "$STAGE/Everyseek.app"
ln -s /Applications "$STAGE/Applications"
# Keep the installer root limited to the app and Applications shortcut.
hdiutil create -volname Everyseek -srcfolder "$STAGE" -ov -format UDZO "$DIST/$NAME.dmg"
codesign --sign "$DEVELOPER_ID" --timestamp "$DIST/$NAME.dmg"
if [[ -n "${NOTARY_PROFILE:-}" ]]; then
  xcrun notarytool submit "$DIST/$NAME.dmg" --keychain-profile "$NOTARY_PROFILE" --wait
  xcrun stapler staple "$DIST/$NAME.dmg"
  xcrun stapler validate "$DIST/$NAME.dmg"
fi
ditto -c -k --keepParent "$APP" "$DIST/$NAME.zip"
(cd "$DIST" && shasum -a 256 "$NAME.dmg" "$NAME.zip" > "$NAME.sha256")
echo "Team release: $DIST/$NAME.dmg"
