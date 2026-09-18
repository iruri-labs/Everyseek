#!/usr/bin/env bash
# Builds a self-contained app. Set DEVELOPER_ID for a stable team signature.
# INSTALL=1 copies it to /Applications; the default leaves an inspectable artifact.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:$PATH"
export ARCHS="${ARCHS:-$(uname -m)}"
"$ROOT/scripts/generate-app-icons.sh"
"$ROOT/scripts/build-core.sh"
xcodegen generate --spec "$ROOT/App/project.yml"
xcodebuild -project "$ROOT/App/EverythingMac.xcodeproj" -scheme EverythingMac \
  -configuration Release -destination "generic/platform=macOS" -derivedDataPath "$ROOT/.build/xcode" \
  ARCHS="$ARCHS" ONLY_ACTIVE_ARCH=NO CODE_SIGNING_ALLOWED=NO build
APP="$ROOT/.build/xcode/Build/Products/Release/Everyseek.app"
python3 "$ROOT/scripts/generate-notices.py" "$APP/Contents/Resources/ThirdPartyNotices.txt"
if [[ -n "${DEVELOPER_ID:-}" ]]; then
  codesign --force --options runtime --timestamp --sign "$DEVELOPER_ID" "$APP"
else
  codesign --force --sign - "$APP"
fi
codesign --verify --strict "$APP"
if [[ "${INSTALL:-0}" == 1 ]]; then
  ditto "$APP" /Applications/Everyseek.app
fi
echo "Built: $APP"
