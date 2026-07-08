# Curiosity Transcripts

Curiosity Transcripts is a local-first transcript app foundation written as a Rust
workspace with a functional macOS-first desktop MVP in `apps/desktop`.

The current MVP can open a Tauri 2 desktop shell, start and stop local desktop
recordings, persist private microphone and system-audio WAV artifacts, and
transcribe a saved meeting with a user-provided local Whisper model. Desktop
builds include the native `whisper-rs` backend by default. System-audio meeting
recording is available through the feature-gated ScreenCaptureKit desktop
backend.

## Quick Start

Use this path for a fresh local checkout when you want the desktop app running
with the same command surface used by contributors and CI.

Prerequisites:

- Rust toolchain with `cargo`.
- Node.js 22 and npm.
- CMake for the default desktop build, because it compiles the native
  `whisper-rs` backend.
- macOS for real microphone capture, ScreenCaptureKit system-audio capture, and
  installer builds.

Install desktop dependencies and run the deterministic checks:

```sh
cd apps/desktop
npm ci
npm run test
npm run build
cd ../..
cargo test --workspace
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
```

Start the Tauri desktop app:

```sh
cd apps/desktop
npm run tauri:dev
```

That opens the real Tauri shell with local commands and the default local
Whisper backend. It can record microphone audio on macOS after permission is
granted. Transcription needs a readable whisper.cpp model file, either saved in
the desktop Settings pane or provided when launching:

```sh
cd apps/desktop
CURIOSITY_WHISPER_MODEL=/absolute/path/to/ggml-base.en.bin npm run tauri:dev
```

For full microphone plus ScreenCaptureKit system-audio recording, launch with
the system-audio feature and grant both Microphone and Screen Recording
permissions:

```sh
cd apps/desktop
CURIOSITY_WHISPER_MODEL=/absolute/path/to/ggml-base.en.bin npm run tauri:dev:system-audio
```

The Vite-only preview is useful for UI work, but it does not run local desktop
commands:

```sh
cd apps/desktop
npm run dev
```

See [Hardware Smoke Checks](#hardware-smoke-checks), [Local Whisper](#local-whisper),
and [Local Ollama Summaries](#local-ollama-summaries) for optional real hardware,
model, and local summary verification.

## Current Status

Implemented MVP flows:

- React/Vite/Tauri 2 desktop app under `apps/desktop`.
- Tauri commands for desktop snapshots, settings, recording, transcription,
  search/export/delete, local summary generation, and smoke checks.
- Real macOS desktop capture through `MacosDesktopWavRecording`, `cpal`, and
  ScreenCaptureKit, writing separate private `raw-mic.wav` and
  `raw-system.wav` artifacts when run with the system-audio feature.
- Real local Whisper transcription through the default `whisper-rs` desktop feature,
  using the saved desktop setting or `CURIOSITY_WHISPER_MODEL` as the fallback
  model path. Meetings with both mic and system WAV artifacts are transcribed as
  one persisted transcript run with channel-tagged segments.
- Durable SQLite store for meetings, recording sessions, audio artifacts,
  transcription and summary processing jobs, transcript versions, edits, exports,
  search indexes, and analysis results.
- Deterministic fixture transcription plus transcript export helpers for
  Markdown, JSON, and SRT.
- Organizer APIs for meeting detail, list, rename, search, transcript export, and
  delete flows.
- Structured summary generation with citations, decisions, action items,
  questions, local Ollama wiring, and privacy-gated provider paths.
- Desktop command wiring for transcript search, JSON/Markdown/SRT export, delete, and
  summary generation after a transcript is ready.
- Imported local WAV workflow through the desktop command, UI, store, and
  transcription seams.
- Transcript segment correction through the desktop detail view, command layer,
  edit-history storage seam, and export/search refresh path.
- Checked-in desktop command/view contract fixture at
  `apps/desktop/contracts/desktop-command-view-contract.fixture.json`, guarded by
  Rust exact-equality and TS command adapter contract tests.
- Settings-pane first-run readiness guidance for local Whisper and Ollama setup:
  missing/readable Whisper paths are typed, readable paths remain compatibility
  unverified, explicit `Test path` evidence is persisted when run, and
  transcription is blocked until the saved Whisper path has matching valid
  file-size evidence. Ollama availability stays unknown until matching
  `Test Ollama` evidence exists. The last explicit Ollama observation is shown
  as available, missing-model, or unavailable-at-last-test evidence without
  becoming a background health check.
- Read-only Apple Calendar context status in the desktop snapshot and settings
  pane, plus an explicit macOS Apple Calendar permission request action. The
  macOS 14+ path requests full Calendar event access, while the macOS 13 support
  floor uses EventKit's older explicit event-access prompt. When access is
  granted, the app loads a bounded read-only list of upcoming local Calendar
  events for manual review, marks unsafe event shapes non-attachable, and lets
  the user manually attach a safe event as meeting context. Unknown EventKit
  privacy requires an explicit confirmation before attachment. Private, all-day,
  recurring, ambiguous, and overlapping events remain blocked. Calendar events
  never auto-start recordings, and the app does not store calendar tokens or
  sync hosted calendar data.
- Debug/test-only `seed_dev_fixture` Tauri command for seeding one private,
  transcript-ready local meeting without microphone, Whisper, or Ollama.

Remaining gaps:

- Model download/management UI for Whisper and Ollama.
- Richer privacy controls for retention, storage location, provider disclosure,
  and remaining exported files.
- App-level encryption-at-rest and keychain-backed secret storage are not
  implemented in v1. See `docs/at-rest-data-strategy.md` for the current
  app-private storage decision and future keychain boundary.
- Real-hardware release confidence for default, no-Whisper, and
  ScreenCaptureKit system-audio builds.
- Later cloud calendar connectors.
- Developer ID signed and notarized macOS DMG workflows exist; Apple signing
  credentials and expanded contributor onboarding/process docs remain.

## Supported Export Formats

The shipped desktop app exposes JSON, Markdown, and SRT export from the meeting
detail view. The generic Tauri `export_meeting` command accepts `json`,
`markdown`, or `srt`; the existing `export_meeting_json` command remains as a
compatibility path.

JSON remains the deterministic integration format for automation and downstream
tools. Markdown and SRT are user-facing transcript exports backed by the
transcription helpers in `crates/transcription`.

## Roadmap

The roadmap follows the trust-first direction in
`docs/local-transcript-app-plan.md`: keep the manual local transcript loop
dependable before adding automation, hosted providers, or broad integrations.
For the production-readiness sequence, see
`docs/production-readiness-roadmap.md`.

Near-term product hardening:

- Build on the implemented Settings-pane manual readiness guidance with model
  discovery, download/management, compatibility checks, and richer availability
  states for Whisper and Ollama.
- Finish per-meeting privacy controls for raw-audio retention, local-only versus
  hosted-provider use, storage location, and remaining exported files after
  deletion.
- Keep feature-matrix verification in CI, including macOS CI for the
  ScreenCaptureKit compile path, and add broader real-hardware release
  confidence for the supported desktop build variants.
- Keep the macOS installer path reproducible with ad-hoc signed local builds,
  then keep browser-distributed releases Developer ID signed and notarized.

Calendar roadmap:

- Build on the implemented Apple Calendar permission, read-only event listing,
  and safe manual attachment seam with clearer calendar-driven organization
  flows.
- Keep auto-start disabled until allowlist rules, ambiguous-event handling,
  private/all-day/recurring-event protections, and always-visible recording
  indicators are verified.
- Add Google Calendar and Outlook after Apple Calendar using explicit
  connect/disconnect flows, keychain-backed tokens, incremental sync, and no
  coupling to hosted transcription or hosted LLMs.

Search and intelligence roadmap:

- Keep SQLite FTS5 keyword search as the reliable baseline.
- Add semantic search only after deterministic rebuild and fallback behavior is
  tested with local embeddings.
- Add speaker labels, cross-meeting questions, and sentiment/tone later, with
  source citations, uncertainty, and editable local outputs.

Engineering hardening before broader contributors:

- Split the large Tauri, audio, store, and desktop UI modules behind their
  current public facades so command handling, recording, transcription, summary
  generation, settings, storage repair, search/export/delete, and platform
  capture code each have clear locality.
- Generate or lock the Rust-to-TypeScript command/view contracts so Tauri DTOs
  cannot drift silently from the frontend types.
- Maintain and extend the existing CI gate for root Rust, desktop Rust, desktop
  npm, release readiness, Pages/release workflow checks, and fail-loud smoke
  commands as new surfaces are added.
- Keep future secrets, OAuth tokens, provider keys, and encryption keys in the
  OS keychain rather than SQLite or plain settings files. This is a future
  implementation boundary, not current v1 support.

## Workspace Layout

```text
apps/
  desktop/        React/Vite/Tauri 2 desktop shell and local command bridge.
crates/
  audio/          Audio capture contracts, cpal microphone capture, smoke paths,
                  and feature-gated ScreenCaptureKit desktop/system-audio capture.
  domain/         Shared meeting, recording, transcript, artifact, job, and analysis domain types.
  store/          SQLite persistence, migrations, search, export, delete, recovery, and analysis storage.
  transcription/  Deterministic fixture transcriber, optional whisper-rs backend, and export formats.
  analysis/       Structured meeting analysis, provider presets, fake/Ollama/hosted provider gates.
  app/            Service/API-facing DTOs and commands over audio, store, transcription, and analysis.
```

The top-level Cargo workspace covers the Rust crates under `crates/`. The Tauri
backend for the desktop app is a separate Cargo manifest at
`apps/desktop/src-tauri/Cargo.toml`.

## Local Setup

Additional prerequisites and command details:

- Rust toolchain with `cargo` installed. `rustup` is the usual install path.
- Node.js 22 and npm for the desktop frontend. CI uses Node.js 22.
- macOS for real microphone and ScreenCaptureKit desktop capture checks and
  installer builds.
- CMake for default desktop builds, because they include the native
  `whisper-rs` backend. If `cmake` is missing, the native `whisper-rs-sys`
  build fails before local Whisper can be verified.
- Xcode Command Line Tools and a working Swift runtime for the optional
  ScreenCaptureKit system-audio feature.
- macOS Microphone permission for desktop recording and Screen Recording
  permission for ScreenCaptureKit system audio.

Deterministic workspace tests do not require hardware, network access, calendar
credentials, hosted provider keys, Ollama, or a Whisper model:

```sh
cargo test --workspace
```

Desktop frontend preview, build, and tests:

```sh
cd apps/desktop
npm ci
npm run test
npm run dev
npm run build
```

`npm run dev` serves the Vite frontend on `http://127.0.0.1:1420`. Outside the
Tauri runtime it uses the preview/mock command fallback instead of local desktop
commands.

Tauri desktop development run:

```sh
cd apps/desktop
npm run tauri:dev
```

The Tauri config uses `devUrl` `http://127.0.0.1:1420` and
`beforeDevCommand` `npm run dev`.

Full desktop recording with microphone plus system audio requires the
ScreenCaptureKit feature:

```sh
cd apps/desktop
CURIOSITY_WHISPER_MODEL=/absolute/path/to/ggml-base.en.bin npm run tauri:dev:system-audio
```

Plain `npm run tauri:dev` still builds the desktop shell and local Whisper
backend. In that build, recording falls back to microphone-only capture because
the system-audio backend is not compiled in. The normal recording flow always
attempts mic plus system audio where the backend and macOS permissions allow it,
then keeps a valid mic-only artifact when no system audio is available.

In debug/test Tauri builds, a harness can invoke `seed_dev_fixture` to create
one deterministic transcript-ready meeting in app-private storage. Release
builds do not register this command, and there is no production UI control for
it.

## macOS Installer Build

The desktop app is configured to produce a macOS `.app` bundle with the
ScreenCaptureKit system-audio feature enabled, then package it into a DMG:

```sh
./scripts/build-macos-dmg.sh
```

When Developer ID signing credentials are not available, local ad-hoc signed
verification can use:

```sh
./scripts/build-macos-dmg.sh --no-sign
```

Generated artifacts are written under:

```text
apps/desktop/src-tauri/target/release/bundle/macos/
apps/desktop/src-tauri/target/release/bundle/dmg/
```

Browser-distributed macOS releases require a Developer ID Application
certificate and notarization. See `docs/macos-dmg-release.md` for the release
checklist, GitHub secret names, signing environment variables, and manual
installer smoke path.

## GitHub Pages Homepage

The static homepage under `site/` is published by `.github/workflows/pages.yml`.
On each `main` deployment, GitHub Actions builds the Developer ID signed and
notarized macOS DMG on a macOS 26 runner, copies it into the Pages artifact, and
updates the stable download link at `downloads/Curiosity-Transcripts-latest.dmg`.

The public page describes the local-first MVP, links back to the source, and
credits CuriosityAI at `https://curiosityai.nl`.

## Versioning Rules

Release versions use SemVer without a leading `v` in package metadata. Release
tags use `vMAJOR.MINOR.PATCH` and must match the same version in:

- `apps/desktop/package.json`
- `apps/desktop/package-lock.json`
- root `Cargo.toml` `[workspace.package] version`
- `apps/desktop/src-tauri/Cargo.toml`
- `apps/desktop/src-tauri/tauri.conf.json`

For example, package version `0.1.18` is released from tag `v0.1.18`. A tag that
does not match the app metadata fails before upload.

The GitHub Pages workflow keeps the moving latest download at
`downloads/Curiosity-Transcripts-latest.dmg`. Versioned distribution happens
through GitHub Release assets. The release workflow uploads:

```text
Curiosity-Transcripts-<version>-macos-aarch64.dmg
Curiosity-Transcripts-<version>-macos-aarch64.dmg.sha256
```

Release DMGs are Developer ID signed and notarized before upload. Local
`--no-sign` builds remain available for testing the packaging path without
Apple credentials.

The first public release architecture is arm64-only macOS. The release workflow
asserts an `arm64` runner before publishing the `macos-aarch64` asset, and the
stable Pages DMG follows the same signed/notarized Apple Silicon release path.
Do not advertise x64 or universal macOS builds unless the workflows, release
assets, checklist, and public copy change together.

Before treating a build as a release candidate, run the deterministic gates and
the manual smoke checklist in `docs/release-candidate-checklist.md`.

## License And Attribution

Curiosity Transcripts is licensed under the Apache License, Version 2.0
(`Apache-2.0`). See `LICENSE` for the full license text and `NOTICE` for the
project attribution notice.

Commercial projects may use and redistribute the project under Apache-2.0, but
redistributions must preserve the required license and notice attribution. See
`ATTRIBUTION.md` for practical attribution guidance.

Security issues should be reported privately. See `SECURITY.md` before sharing
vulnerability details or private transcript data.

## Hardware Smoke Checks

Default smoke status does not claim hardware success:

```sh
cargo run -p curiosity-audio --bin audio-smoke
```

Without an explicit hardware flag this reports a skipped status and exits
nonzero. That is intentional.

Microphone smoke:

```sh
cargo run -p curiosity-audio --bin audio-smoke -- --attempt-mic --out audio-smoke-output --duration-ms 1000
```

This attempts real macOS microphone capture and only reports `Passed` after
samples are captured and a WAV artifact is finalized. Permission denial,
missing devices, or streams that produce no samples are reported as non-passing
states.

ScreenCaptureKit system-audio smoke:

```sh
cargo run -p curiosity-audio --features system-audio-screencapturekit --bin audio-smoke -- --attempt-system-audio --out audio-smoke-output --duration-ms 1000
```

This uses the same feature-gated ScreenCaptureKit capability required by full
desktop recording. It requires macOS Screen Recording permission plus a working
Swift/Xcode Command Line Tools runtime, and it should report unavailable or
permission-denied states honestly when prerequisites are missing.

## Local Whisper

Local Whisper is enabled in default desktop builds. First verify that native
prerequisites are installed:

```sh
command -v cmake
```

If that command prints nothing, install CMake before building the desktop app.
On macOS, `brew install cmake` is one common path.

Verify that the desktop backend and native Whisper dependency can compile:

```sh
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --no-run
```

Run the real local Whisper smoke against an existing whisper.cpp model file and
WAV artifact:

```sh
CURIOSITY_WHISPER_MODEL=/absolute/path/to/ggml-base.en.bin \
CURIOSITY_WHISPER_WAV=/absolute/path/to/audio.wav \
cargo run -p curiosity-transcription --features whisper-rs --bin whisper-smoke
```

The smoke exits zero only when real transcription passes. Missing env paths,
disabled native features, unreadable files, unsupported audio, and backend
failures exit nonzero so they are not counted as success.

Run the desktop app with local Whisper enabled:

```sh
cd apps/desktop
CURIOSITY_WHISPER_MODEL=/absolute/path/to/ggml-base.en.bin npm run tauri:dev:system-audio
```

The desktop settings pane can choose a local Whisper model file with a native
file picker, keep the path editable, save the path, and show first-run readiness
guidance for missing paths and readable-but-unverified files. Running `Test
path` stores the last explicit file-size and SHA-256 readability evidence for
the matching saved path; it does not prove model compatibility. The desktop
marks an existing file as untested and blocks transcription until that matching
valid Test path evidence exists and the current file size still matches. If no
path is saved, the desktop `transcribe_meeting` command falls back to
`CURIOSITY_WHISPER_MODEL`, which is subject to the same Test path evidence
requirement. Desktop builds include the native Whisper backend by default; use
`npm run tauri:dev:no-whisper` only when intentionally testing the
unavailable-backend state. If the effective model path is missing, the UI should
show an explicit missing-model state. Model download and management are not yet
implemented.

Copy `.env.example` for the optional Whisper smoke environment variables and
hosted/provider secret placeholders. Ollama base URL and model are configured in
the desktop Settings pane; the values in `.env.example` are documentation of the
current local defaults, not runtime env inputs.

## Local Ollama Summaries

Structured summaries can run locally through Ollama after a transcript exists.
Start Ollama, install the selected local model, then use the Settings pane to
test the configured server and model:

```sh
ollama serve
ollama pull qwen3.6:27b
```

`gemma4:31b` is also listed as a local candidate. The desktop defaults are
`http://127.0.0.1:11434` and `qwen3.6:27b`, and store settings are the runtime
source of truth. The local Ollama path accepts localhost/loopback URLs only; use
the hosted provider path, disclosure gate, and explicit secrets for any
networked provider. Snapshot readiness guidance does not probe Ollama; availability
is unknown until the user runs the Settings pane's `Test Ollama` action for the
matching saved local URL/model. Matching last-test evidence is reported as
available, missing-model, or unavailable-at-last-test guidance, including
installed-model evidence or the suggested pull command, but it is not treated as
current availability.

End-to-end expectation:

1. Start `ollama serve`.
2. Pull the chosen model, such as `ollama pull qwen3.6:27b`.
3. Open Settings, confirm the Ollama base URL/model, and run the connection
   test.
4. Record and transcribe a meeting.
5. Generate the summary from the selected meeting once transcript segments are
   present.

## Privacy And Providers

The default test suite and core local service flows are deterministic and
hardware/network-free. They do not require:

- Network access.
- Calendar access.
- OpenAI keys.
- Ollama.
- Hosted analysis providers.

Local analysis presets currently include Ollama model candidates:

- `qwen3.6:27b`
- `gemma4:31b`

Hosted or networked analysis is gated. OpenAI-compatible hosted providers
require explicit key selection and explicit transcript data disclosure
confirmation before any provider call is made.

For at-rest storage, v1 uses app-private SQLite and local artifact files and
relies on OS/user-account file protections. App-level encryption-at-rest and
keychain-backed provider secret storage are not implemented yet. The documented
boundary for current local data, user-owned exports/source files, and future
keychain use lives in `docs/at-rest-data-strategy.md`.

`deepseek-v3.2:cloud` and DeepSeek V3.2 Speciale are not local defaults. They
are network/hosted options and must stay behind the hosted disclosure and
key-selection gates.

## Contributor Notes

- Keep deterministic tests independent of hardware, network, calendars, hosted
  model keys, local Ollama availability, and local Whisper model files.
- Use fake capture, fake transcriber, fake analyzer, or static provider clients
  for regular tests.
- Hardware smoke tests should fail loud with skipped, unavailable,
  permission-denied, or failed status when prerequisites are missing.
- Do not silently convert skipped hardware/provider behavior into passing tests.
- Prefer small, crate-local changes that preserve the current service/API
  contracts while the desktop shell matures.

This app was made by @CuriosityAI - https://curiosityai.nl
