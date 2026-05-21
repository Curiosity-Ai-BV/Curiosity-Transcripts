# Curiosity Transcripts

Curiosity Transcripts is a local-first transcript app foundation written as a Rust
workspace with a functional macOS-first desktop MVP in `apps/desktop`.

The current MVP can open a Tauri 2 desktop shell, start and stop local microphone
recordings, persist private WAV artifacts and meeting metadata, and transcribe a
saved meeting with a user-provided local Whisper model when the optional
`whisper-rs` feature is enabled. System-audio capture exists as a feature-gated
ScreenCaptureKit smoke path; the main recording UI is still microphone-only.

## Current Status

Implemented MVP flows:

- React/Vite/Tauri 2 desktop app under `apps/desktop`.
- Tauri commands for `desktop_snapshot`, `start_microphone_recording`,
  `stop_microphone_recording`, `transcribe_meeting`, `audio_smoke_status`, and
  `system_audio_smoke_recording`.
- Real macOS microphone capture through `MacosMicrophoneWavRecording` and
  `cpal`, writing private local WAV artifacts.
- Optional real local Whisper transcription behind the `whisper-rs` feature,
  using `CURIOSITY_WHISPER_MODEL` as the model path.
- Durable SQLite store for meetings, recording sessions, audio artifacts,
  processing jobs, transcript versions, edits, exports, search indexes, and
  analysis results.
- Deterministic fixture transcription and transcript export helpers for
  Markdown, JSON, and SRT.
- Organizer APIs for meeting detail, list, rename, search, JSON export, and
  delete flows.
- Structured summary generation with citations, decisions, action items,
  questions, and privacy-gated provider paths.

Remaining gaps:

- Production packaging and installer flow.
- Calendar integration.
- System-audio recording in the main desktop recording UI.
- Model download and management UI for Whisper models.
- Desktop command wiring for some service foundations, including summary,
  export, and delete actions.

## Workspace Layout

```text
apps/
  desktop/        React/Vite/Tauri 2 desktop shell and local command bridge.
crates/
  audio/          Audio capture contracts, cpal microphone capture, smoke paths,
                  and feature-gated ScreenCaptureKit system-audio capture.
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

Prerequisites:

- Rust toolchain with `cargo` installed. `rustup` is the usual install path.
- Node.js and npm for the desktop frontend.
- macOS for real microphone and ScreenCaptureKit smoke checks.
- CMake when building the optional `whisper-rs` feature. If `cmake` is missing,
  the native `whisper-rs-sys` build fails before local Whisper can be verified.
- Xcode Command Line Tools and a working Swift runtime for the optional
  ScreenCaptureKit system-audio feature.
- macOS Microphone permission for microphone recording and Screen Recording
  permission for ScreenCaptureKit system audio.

Deterministic workspace tests do not require hardware, network access, calendar
credentials, hosted provider keys, Ollama, or a Whisper model:

```sh
cargo test --workspace
```

Desktop frontend preview, build, and tests:

```sh
cd apps/desktop
npm install
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
npm exec -- tauri dev
```

The Tauri config uses `devUrl` `http://127.0.0.1:1420` and
`beforeDevCommand` `npm run dev`.

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

This is a feature-gated smoke path, not the main desktop recording flow. It
requires macOS Screen Recording permission plus a working Swift/Xcode Command
Line Tools runtime, and it should report unavailable or permission-denied states
honestly when prerequisites are missing.

## Local Whisper

Local Whisper is optional and is not enabled in default tests or default desktop
builds. Provide a local whisper.cpp model file and build the desktop backend
with the feature enabled:

```sh
cd apps/desktop
CURIOSITY_WHISPER_MODEL=/absolute/path/to/ggml-base.en.bin npm exec -- tauri dev --features whisper-rs
```

The desktop `transcribe_meeting` command reads `CURIOSITY_WHISPER_MODEL`. If the
feature is disabled or the path is missing, the UI should show an explicit
unavailable/missing-model state. Model download and management are not yet
implemented.

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
