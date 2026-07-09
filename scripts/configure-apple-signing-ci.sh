#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "::error::Apple signing credentials can only be configured on macOS runners" >&2
  exit 1
fi

require_env() {
  local name="$1"
  if [[ -z "${!name:-}" ]]; then
    echo "::error::Missing required Apple signing environment variable: $name" >&2
    exit 1
  fi
}

require_any_notary_credentials() {
  local api_key_path="${APPLE_API_KEY_PATH:-}"
  local api_key_content="${APPLE_API_KEY_P8_BASE64:-}"
  local api_key_id="${APPLE_API_KEY_ID:-${APPLE_API_KEY:-}}"
  local has_api_input=0
  local has_apple_id_input=0

  if [[ -n "${APPLE_API_ISSUER:-}" || -n "$api_key_id" || -n "$api_key_path" || -n "$api_key_content" ]]; then
    has_api_input=1
    require_env APPLE_API_ISSUER
    if [[ -z "$api_key_id" ]]; then
      echo "::error::Missing required Apple signing environment variable: APPLE_API_KEY_ID" >&2
      exit 1
    fi
    if [[ -z "$api_key_path" && -z "$api_key_content" ]]; then
      echo "::error::Provide APPLE_API_KEY_P8_BASE64 or APPLE_API_KEY_PATH for App Store Connect API notarization" >&2
      exit 1
    fi
  fi

  if [[ -n "${APPLE_ID:-}" || -n "${APPLE_PASSWORD:-}" || -n "${APPLE_TEAM_ID:-}" ]]; then
    has_apple_id_input=1
    require_env APPLE_ID
    require_env APPLE_PASSWORD
    require_env APPLE_TEAM_ID
  fi

  if [[ "$has_api_input" -eq 0 && "$has_apple_id_input" -eq 0 ]]; then
    echo "::error::Missing notarization credentials. Configure App Store Connect API secrets or APPLE_ID, APPLE_PASSWORD, and APPLE_TEAM_ID." >&2
    exit 1
  fi
}

require_env APPLE_SIGNING_IDENTITY
require_env APPLE_CERTIFICATE_P12_BASE64
require_env APPLE_CERTIFICATE_PASSWORD
require_any_notary_credentials

runner_temp="${RUNNER_TEMP:-/tmp}"
certificate_path="$runner_temp/developer-id-application.p12"
keychain_path="$runner_temp/curiosity-transcripts-signing.keychain-db"
keychain_password="${APPLE_KEYCHAIN_PASSWORD:-$(uuidgen)}"

printf '%s' "$APPLE_CERTIFICATE_P12_BASE64" | base64 -D > "$certificate_path"

security create-keychain -p "$keychain_password" "$keychain_path"
security set-keychain-settings -lut 21600 "$keychain_path"
security unlock-keychain -p "$keychain_password" "$keychain_path"
security import "$certificate_path" \
  -P "$APPLE_CERTIFICATE_PASSWORD" \
  -A \
  -t cert \
  -f pkcs12 \
  -k "$keychain_path"
security set-key-partition-list \
  -S apple-tool:,apple:,codesign: \
  -s \
  -k "$keychain_password" \
  "$keychain_path"
security list-keychains -d user -s "$keychain_path"
security default-keychain -s "$keychain_path"

identity_output="$(security find-identity -v -p codesigning "$keychain_path")"
printf '%s\n' "$identity_output"
if ! grep -F -- "$APPLE_SIGNING_IDENTITY" <<< "$identity_output" >/dev/null; then
  echo "::error::Imported certificate does not include APPLE_SIGNING_IDENTITY: $APPLE_SIGNING_IDENTITY" >&2
  exit 1
fi

if [[ -n "${APPLE_API_KEY_P8_BASE64:-}" ]]; then
  api_key_id="${APPLE_API_KEY_ID:-${APPLE_API_KEY:-}}"
  api_key_path="$runner_temp/AuthKey_${api_key_id}.p8"
  printf '%s' "$APPLE_API_KEY_P8_BASE64" | base64 -D > "$api_key_path"
  chmod 600 "$api_key_path"
  {
    echo "APPLE_API_KEY_PATH=$api_key_path"
    echo "APPLE_API_KEY_ID=$api_key_id"
  } >> "$GITHUB_ENV"
fi

echo "APPLE_SIGNING_KEYCHAIN_PATH=$keychain_path" >> "$GITHUB_ENV"
