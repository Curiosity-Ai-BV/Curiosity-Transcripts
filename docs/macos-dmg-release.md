# macOS DMG Release

Date: 2026-05-22

## Scope

This document covers producing a macOS `.app` bundle and `.dmg` installer for
Curiosity Transcripts. It does not change the runtime model setup behavior:
Whisper and Ollama model download/management still need a separate first-run
model manager.

## Release Build

From the repository root:

```sh
./scripts/build-macos-dmg.sh
```

The script runs:

1. `bash scripts/check-publication-readiness.sh` from the repository root.
2. `npm ci` in `apps/desktop`.
3. The desktop Vitest suite.
4. `tauri build` with `system-audio-screencapturekit` enabled and the macOS
   `.app` bundle target.
5. `hdiutil create` against a staging folder containing the `.app` bundle and
   an `/Applications` symlink.
6. `codesign` seals the `.app` bundle with Developer ID when credentials are
   available, otherwise with an ad-hoc local signature.
7. `hdiutil verify` against the produced DMG.
8. `notarytool` submits the signed DMG and `stapler` attaches the notarization
   ticket when notarization credentials are available.
9. A read-only attach of the DMG to confirm `Curiosity Transcripts.app` exists
   and passes strict code-signature verification before the script reports
   success. Notarized builds also run `stapler validate` and Gatekeeper
   assessment checks.

Expected outputs:

```text
apps/desktop/src-tauri/target/release/bundle/macos/Curiosity Transcripts.app
apps/desktop/src-tauri/target/release/bundle/dmg/Curiosity Transcripts_0.1.18_<arch>.dmg
```

For local ad-hoc signed verification when Apple signing credentials are unavailable:

```sh
./scripts/build-macos-dmg.sh --no-sign
```

Ad-hoc signed builds are useful for local smoke checks. They are sealed so macOS
does not report a malformed bundle, but browser download distribution requires
Developer ID signing and notarization.

## macOS Signing And Notarization

For distribution outside the Mac App Store, build on macOS with a Developer ID
Application certificate. Public DMGs are Developer ID signed and notarized.
Tauri can use either `bundle.macOS.signingIdentity` or the
`APPLE_SIGNING_IDENTITY` environment variable.

For notarization, provide either App Store Connect API credentials:

```sh
export APPLE_API_ISSUER="..."
export APPLE_API_KEY_ID="..."
export APPLE_API_KEY_PATH="/absolute/path/to/AuthKey_....p8"
```

or Apple ID credentials:

```sh
export APPLE_ID="developer@example.com"
export APPLE_PASSWORD="app-specific-password"
export APPLE_TEAM_ID="TEAMID1234"
```

Then rerun:

```sh
./scripts/build-macos-dmg.sh
```

The packaging script signs the generated DMG when `APPLE_SIGNING_IDENTITY` is
set. It submits and staples the DMG when either App Store Connect API
credentials or Apple ID notarization credentials are present.

## GitHub Release Signing Secrets

The GitHub Release and Pages workflows require Apple signing secrets before they
publish a public DMG. Store the Release and Pages workflow Apple secrets in the
protected `macos-signing` environment. Local code cannot enforce GitHub
environment rules; repository settings must allow the intended Pages `main`
dispatch and release tag path, for example protected `v*` tags, and block
unintended refs before they can reach the signing path. The Pages `latest` DMG
publication also requires manual workflow dispatch confirmation that filled
smoke evidence validates with
`node scripts/check-release-smoke-evidence.js path/to/filled-evidence.json`.
Configure these secrets:

```text
APPLE_SIGNING_IDENTITY=Developer ID Application: Example Org (TEAMID1234)
APPLE_CERTIFICATE_P12_BASE64=<base64 encoded Developer ID Application .p12>
APPLE_CERTIFICATE_PASSWORD=<.p12 export password>
APPLE_KEYCHAIN_PASSWORD=<temporary CI keychain password>
```

For notarization, prefer App Store Connect API credentials:

```text
APPLE_API_ISSUER=<issuer UUID>
APPLE_API_KEY_ID=<key ID>
APPLE_API_KEY_P8_BASE64=<base64 encoded AuthKey_....p8>
```

Alternatively, use Apple ID notarization credentials:

```text
APPLE_ID=developer@example.com
APPLE_PASSWORD=<app-specific password>
APPLE_TEAM_ID=TEAMID1234
```

The CI helper imports the `.p12` into an ephemeral macOS keychain, writes the
notary API key to a temporary file when provided, and exports
`APPLE_API_KEY_PATH` for the packaging script. Missing or partial credentials
fail the workflow before an ad-hoc public release can be uploaded.

The release workflow verifies the uploaded candidate with:

```sh
hdiutil verify Curiosity-Transcripts-<version>-macos-aarch64.dmg
xcrun stapler validate Curiosity-Transcripts-<version>-macos-aarch64.dmg
spctl -a -vvv -t open --context context:primary-signature Curiosity-Transcripts-<version>-macos-aarch64.dmg
```

## Manual Installer Smoke

Use a clean macOS user account or a machine without the development checkout.
The release-candidate source of truth is `docs/release-candidate-checklist.md`;
the shorter list below is only the installer-specific subset.

1. Open the generated DMG.
2. Drag `Curiosity Transcripts.app` to `/Applications`.
3. Launch from `/Applications`.
4. Confirm macOS prompts show the packaged app name for microphone and screen
   recording permissions.
5. Configure a local Whisper model path.
6. Start a short recording, stop, transcribe, export JSON, and delete private
   data.

After model setup, repeat the recording/transcription flow with network
disabled to confirm the local path remains offline.
