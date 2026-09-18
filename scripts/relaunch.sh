#!/usr/bin/env bash
# Relaunch without deleting the persistent index or its pending event journal.
set -euo pipefail
APP="${1:-/Applications/Everyseek.app}"
osascript -e 'if application id "com.everyseek.app" is running then tell application id "com.everyseek.app" to quit'
sleep 1
open "$APP"
