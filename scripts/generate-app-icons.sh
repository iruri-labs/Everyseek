#!/usr/bin/env bash
# Package the approved artwork without changing its design.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SOURCE="$ROOT/logo/everyseek.png"
DEST="$ROOT/App/Assets.xcassets/AppIcon.appiconset"
[[ -f "$SOURCE" ]] || { echo "Missing icon source: $SOURCE" >&2; exit 1; }
for SIZE in 16 32 64 128 256 512 1024; do
  sips -z "$SIZE" "$SIZE" "$SOURCE" --out "$DEST/icon_${SIZE}.png" >/dev/null
done
