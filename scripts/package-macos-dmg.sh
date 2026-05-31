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
NOTARIZED_DMG=0

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
    local codesign_args=(--force --deep --options runtime --timestamp --sign "$APPLE_SIGNING_IDENTITY")
    if [[ -n "${APPLE_SIGNING_KEYCHAIN_PATH:-}" ]]; then
      codesign_args+=(--keychain "$APPLE_SIGNING_KEYCHAIN_PATH")
    fi
    codesign "${codesign_args[@]}" "$APP_PATH"
  else
    codesign --force --deep --sign - "$APP_PATH"
  fi

  codesign --verify --deep --strict --verbose=2 "$APP_PATH"
}

has_notarization_credentials() {
  local api_key_id="${APPLE_API_KEY_ID:-${APPLE_API_KEY:-}}"

  [[ -n "${APPLE_API_ISSUER:-}" && -n "$api_key_id" && -n "${APPLE_API_KEY_PATH:-}" ]] ||
    [[ -n "${APPLE_ID:-}" && -n "${APPLE_PASSWORD:-}" && -n "${APPLE_TEAM_ID:-}" ]]
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

  if [[ "$NOTARIZED_DMG" -eq 1 ]]; then
    xcrun stapler validate "$DMG_PATH"
    spctl -a -vvv -t open --context context:primary-signature "$DMG_PATH"
    spctl -a -vvv -t exec "$VERIFY_MOUNT_DIR/$APP_NAME.app"
  fi

  hdiutil detach "$VERIFY_MOUNT_DIR"
  rm -rf "$VERIFY_MOUNT_DIR"
  VERIFY_MOUNT_DIR=""
}

if [[ -z "${CURIOSITY_SKIP_DMG_SIGN:-}" ]] && has_notarization_credentials && [[ -z "${APPLE_SIGNING_IDENTITY:-}" ]]; then
  echo "Notarization credentials were provided, but APPLE_SIGNING_IDENTITY is missing." >&2
  echo "macOS notarization requires a Developer ID signed app." >&2
  exit 1
fi

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
  dmg_codesign_args=(--force --timestamp --sign "$APPLE_SIGNING_IDENTITY")
  if [[ -n "${APPLE_SIGNING_KEYCHAIN_PATH:-}" ]]; then
    dmg_codesign_args+=(--keychain "$APPLE_SIGNING_KEYCHAIN_PATH")
  fi
  codesign "${dmg_codesign_args[@]}" "$DMG_PATH"
fi

if [[ -n "${CURIOSITY_SKIP_DMG_SIGN:-}" ]]; then
  echo "Skipping DMG signing and notarization because CURIOSITY_SKIP_DMG_SIGN is set."
elif [[ -n "${APPLE_API_ISSUER:-}" && -n "${APPLE_API_KEY_ID:-${APPLE_API_KEY:-}}" && -n "${APPLE_API_KEY_PATH:-}" ]]; then
  api_key_id="${APPLE_API_KEY_ID:-${APPLE_API_KEY:-}}"
  xcrun notarytool submit "$DMG_PATH" \
    --issuer "$APPLE_API_ISSUER" \
    --key-id "$api_key_id" \
    --key "$APPLE_API_KEY_PATH" \
    --wait
  xcrun stapler staple "$DMG_PATH"
  xcrun stapler validate "$DMG_PATH"
  NOTARIZED_DMG=1
elif [[ -n "${APPLE_ID:-}" && -n "${APPLE_PASSWORD:-}" && -n "${APPLE_TEAM_ID:-}" ]]; then
  xcrun notarytool submit "$DMG_PATH" \
    --apple-id "$APPLE_ID" \
    --password "$APPLE_PASSWORD" \
    --team-id "$APPLE_TEAM_ID" \
    --wait
  xcrun stapler staple "$DMG_PATH"
  xcrun stapler validate "$DMG_PATH"
  NOTARIZED_DMG=1
fi

verify_dmg

echo "Created DMG: $DMG_PATH"
