# Curiosity Transcripts

Curiosity Transcripts is a local-first transcript app foundation written as a Rust workspace.

The current repository is not a finished desktop app yet. There is no full Tauri UI, no production packaging, and no real macOS audio capture path wired today. What exists is the testable service/API foundation for local recording state, durable storage, deterministic transcription/export, meeting organization, and privacy-gated analysis providers.

## Current Status

Implemented foundations:

- Audio capture contracts, permission/recovery types, fake capture fixtures, and an explicit manual smoke placeholder.
- Durable SQLite store for meetings, recording sessions, audio artifacts, processing jobs, transcript versions, edits, exports, search indexes, and analysis results.
- Fake manual recording workflow for local command/service behavior, including start, pause, stop, interruption, recoverable artifact handling, and storage failure reporting.
- Deterministic fixture transcription and transcript export helpers for Markdown, JSON, and SRT.
- Organizer APIs for meeting detail, list, rename, search, JSON export, and delete flows.
- Structured summary generation with citations, decisions, action items, questions, and privacy-gated provider paths.

Not implemented yet:

- Full desktop UI.
- Real macOS microphone/system-audio capture.
- Real Whisper integration.
- Calendar integration.
- Production packaging or installer flow.

## Workspace Layout

```text
crates/
  audio/          Audio capture contracts, fake capture, permission guidance, smoke placeholder.
  domain/         Shared meeting, recording, transcript, artifact, job, and analysis domain types.
  store/          SQLite persistence, migrations, search, export, delete, recovery, and analysis storage.
  transcription/  Deterministic fixture transcriber and transcript export formats.
  analysis/       Structured meeting analysis, provider presets, fake/Ollama/hosted provider gates.
  app/            Service/API-facing DTOs and commands over audio, store, transcription, and analysis.
```

The workspace manifest is the top-level `Cargo.toml`, and all crates are included in `cargo test --workspace`.

## Local Setup

Prerequisites:

- Rust toolchain with `cargo` installed. `rustup` is the usual install path.
- No hosted provider keys, calendar credentials, Ollama server, OpenAI key, or network access are required for the deterministic workspace tests and core local flows.

Build and test:

```sh
cargo test --workspace
```

Optional formatting and linting, if the components are installed locally:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets
```

These commands are useful for contributor hygiene, but this README does not claim they are required or currently passing in every environment.

## Audio Smoke Placeholder

The audio crate exposes a manual smoke binary:

```sh
cargo run -p curiosity-audio --bin audio-smoke
```

Current behavior: this is a placeholder until real hardware capture is wired. It reports `NotRun` and exits nonzero. That is intentional. Hardware smoke checks must report skipped/not-run/unavailable states honestly and must not be counted as passing until real capture exists.

## Privacy And Providers

The default test suite and core local service flows are deterministic and hardware/network-free. They do not require:

- Network access.
- Calendar access.
- OpenAI keys.
- Ollama.
- Hosted analysis providers.

Local analysis presets currently include Ollama model candidates:

- `qwen3.6:27b`
- `gemma4:31b`

Hosted or networked analysis is gated. OpenAI-compatible hosted providers require explicit key selection and explicit transcript data disclosure confirmation before any provider call is made.

`deepseek-v3.2:cloud` and DeepSeek V3.2 Speciale are not local defaults. They are network/hosted options and must stay behind the hosted disclosure and key-selection gates.

## Contributor Notes

- Keep deterministic tests independent of hardware, network, calendars, hosted model keys, and local Ollama availability.
- Use fake capture, fake transcriber, fake analyzer, or static provider clients for regular tests.
- Hardware smoke tests should fail loud with `NotRun`, `Skipped`, `Unavailable`, or permission-denied status when prerequisites are missing.
- Do not silently convert skipped hardware/provider behavior into passing tests.
- Prefer small, crate-local changes that preserve the current service/API contracts until the desktop UI exists.

## Roadmap

Near-term gaps are wiring real macOS capture, adding a real local transcription backend such as Whisper, building the desktop UI, adding calendar integration behind explicit permissions, and creating production packaging. Until those land, treat this repository as the local-first Rust foundation for Curiosity Transcripts rather than a shippable desktop application.
