#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_NAME="Curiosity Transcripts"
DESKTOP_DIR="$ROOT_DIR/apps/desktop"
BUNDLE_DIR="$DESKTOP_DIR/src-tauri/target/release/bundle"
APP_PATH="$BUNDLE_DIR/macos/$APP_NAME.app"
DMG_DIR="$BUNDLE_DIR/dmg"
VERSION="$(node -p "require('$DESKTOP_DIR/package.json').version")"
VERIFY_MOUNT_DIR=""

if [[ ! -d "$APP_PATH" ]]; then
  echo "Missing app bundle: $APP_PATH" >&2
  echo "Run npm run tauri:build:mac from apps/desktop first." >&2
  exit 1
fi

case "$(uname -m)" in
  arm64)
    ARCH="aarch64"
    ;;
  x86_64)
    ARCH="x64"
    ;;
  *)
    ARCH="$(uname -m)"
    ;;
esac

mkdir -p "$DMG_DIR"
DMG_PATH="$DMG_DIR/${APP_NAME}_${VERSION}_${ARCH}.dmg"
STAGING_DIR="$(mktemp -d)"

cleanup() {
  if [[ -n "$VERIFY_MOUNT_DIR" && -d "$VERIFY_MOUNT_DIR" ]]; then
    hdiutil detach "$VERIFY_MOUNT_DIR" >/dev/null 2>&1 || true
    rm -rf "$VERIFY_MOUNT_DIR"
  fi
  rm -rf "$STAGING_DIR"
}
trap cleanup EXIT

sign_app_bundle() {
  if [[ -z "${CURIOSITY_SKIP_DMG_SIGN:-}" && -n "${APPLE_SIGNING_IDENTITY:-}" ]]; then
    codesign --force --deep --options runtime --timestamp --sign "$APPLE_SIGNING_IDENTITY" "$APP_PATH"
  else
    codesign --force --deep --sign - "$APP_PATH"
  fi

  codesign --verify --deep --strict --verbose=2 "$APP_PATH"
}

verify_dmg() {
  hdiutil verify "$DMG_PATH"

  VERIFY_MOUNT_DIR="$(mktemp -d)"
  hdiutil attach "$DMG_PATH" -readonly -nobrowse -mountpoint "$VERIFY_MOUNT_DIR"

  if [[ ! -d "$VERIFY_MOUNT_DIR/$APP_NAME.app" ]]; then
    echo "Verified DMG is missing Curiosity Transcripts.app" >&2
    exit 1
  fi

  codesign --verify --deep --strict --verbose=2 "$VERIFY_MOUNT_DIR/$APP_NAME.app"

  hdiutil detach "$VERIFY_MOUNT_DIR"
  rm -rf "$VERIFY_MOUNT_DIR"
  VERIFY_MOUNT_DIR=""
}

sign_app_bundle

cp -R "$APP_PATH" "$STAGING_DIR/"
ln -s /Applications "$STAGING_DIR/Applications"

hdiutil create \
  -volname "$APP_NAME" \
  -srcfolder "$STAGING_DIR" \
  -ov \
  -format UDZO \
  "$DMG_PATH"

if [[ -z "${CURIOSITY_SKIP_DMG_SIGN:-}" && -n "${APPLE_SIGNING_IDENTITY:-}" ]]; then
  codesign --force --sign "$APPLE_SIGNING_IDENTITY" "$DMG_PATH"
fi

if [[ -n "${CURIOSITY_SKIP_DMG_SIGN:-}" ]]; then
  echo "Skipping DMG signing and notarization because CURIOSITY_SKIP_DMG_SIGN is set."
elif [[ -n "${APPLE_API_ISSUER:-}" && -n "${APPLE_API_KEY:-}" && -n "${APPLE_API_KEY_PATH:-}" ]]; then
  xcrun notarytool submit "$DMG_PATH" \
    --issuer "$APPLE_API_ISSUER" \
    --key-id "$APPLE_API_KEY" \
    --key "$APPLE_API_KEY_PATH" \
    --wait
  xcrun stapler staple "$DMG_PATH"
elif [[ -n "${APPLE_ID:-}" && -n "${APPLE_PASSWORD:-}" && -n "${APPLE_TEAM_ID:-}" ]]; then
  xcrun notarytool submit "$DMG_PATH" \
    --apple-id "$APPLE_ID" \
    --password "$APPLE_PASSWORD" \
    --team-id "$APPLE_TEAM_ID" \
    --wait
  xcrun stapler staple "$DMG_PATH"
fi

verify_dmg

echo "Created DMG: $DMG_PATH"
