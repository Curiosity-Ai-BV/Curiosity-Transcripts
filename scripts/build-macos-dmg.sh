#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DESKTOP_DIR="$ROOT_DIR/apps/desktop"

cd "$ROOT_DIR"
bash scripts/check-publication-readiness.sh

cd "$DESKTOP_DIR"

npm ci
npm run test
npm exec -- tauri build --features system-audio-screencapturekit --bundles app --ci "$@"

SKIP_DMG_SIGN=0
for arg in "$@"; do
  if [[ "$arg" == "--no-sign" ]]; then
    SKIP_DMG_SIGN=1
  fi
done

if [[ "$SKIP_DMG_SIGN" -eq 1 ]]; then
  CURIOSITY_SKIP_DMG_SIGN=1 "$ROOT_DIR/scripts/package-macos-dmg.sh"
else
  env -u CURIOSITY_SKIP_DMG_SIGN "$ROOT_DIR/scripts/package-macos-dmg.sh"
fi
