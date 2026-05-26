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

1. `npm ci` in `apps/desktop`.
2. The desktop Vitest suite.
3. `tauri build` with `system-audio-screencapturekit` enabled and the macOS
   `.app` bundle target.
4. `hdiutil create` against a staging folder containing the `.app` bundle and
   an `/Applications` symlink.

Expected outputs:

```text
apps/desktop/src-tauri/target/release/bundle/macos/Curiosity Transcripts.app
apps/desktop/src-tauri/target/release/bundle/dmg/Curiosity Transcripts_0.1.16_<arch>.dmg
```

For local unsigned verification when Apple signing credentials are unavailable:

```sh
./scripts/build-macos-dmg.sh --no-sign
```

Unsigned or ad-hoc signed builds are useful for local smoke checks, but they are
not release artifacts for browser download distribution.

## macOS Signing And Notarization

For distribution outside the Mac App Store, build on macOS with a Developer ID
Application certificate. Tauri can use either `bundle.macOS.signingIdentity` or
the `APPLE_SIGNING_IDENTITY` environment variable.

For notarization, provide either App Store Connect API credentials:

```sh
export APPLE_API_ISSUER="..."
export APPLE_API_KEY="..."
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

## Manual Installer Smoke

Use a clean macOS user account or a machine without the development checkout.

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
