# MVP Status And End-To-End Test Plan

Date: 2026-05-22

## Assumptions

- macOS is the MVP platform.
- "Full cycle" means: configure local models, create a meeting, transcribe locally with Whisper, generate a local Ollama summary, search transcript content, export JSON, and delete app-private data.
- Whisper is the transcription engine. Ollama is the local summary/analysis runtime.
- Calendar integration, production packaging, model downloads, and system audio in the main recording UI are outside this MVP-completion pass.
- Default automated tests must remain hardware-free, network-free, model-free, and Ollama-free.

## Current Assessment

The MVP implementation is now functionally wired for local testing. The desktop shell has real settings, recording/transcription commands, SQLite-backed meeting management, export/delete, local Ollama summaries, and a debug/test fixture seed for deterministic full-cycle validation.

Remaining work before a real-user smoke on this machine is local environment setup, not core MVP wiring:

- Install CMake so the optional `whisper-rs` native dependency can compile.
- Provide a local whisper.cpp model file.
- Run Ollama locally and pull the selected model.
- Grant macOS microphone permission for real recording.

## Implemented Slices

1. Persistent local settings:
   - SQLite-backed Whisper model path and local Ollama settings.
   - Settings pane controls for Whisper path, Ollama base URL, Ollama model, path testing, and Ollama connection testing.
   - `.env.example` documents optional Whisper smoke variables and hosted secret placeholders.

2. Desktop meeting management:
   - Tauri/React wiring for SQLite-backed search, rename, JSON export, and private-data delete.
   - Export/delete outcomes remain visible after command completion.
   - Delete reports skipped private artifacts and preserves user-owned exports.

3. Local Ollama summaries:
   - Local HTTP client for `/api/generate` and `/api/tags`.
   - Loopback-only local Ollama URL validation.
   - Hosted/cloud model tags are rejected on the local privacy path.
   - Structured summary persistence and visible setup/error states.

4. Whisper readiness:
   - `whisper-smoke` CLI for real local Whisper smoke checks.
   - README documents CMake, `whisper-rs`, model path, smoke, and Tauri dev commands.
   - Smoke exits nonzero for skipped/unavailable/failed states.

5. Deterministic full-cycle fixture:
   - Debug/test-only `seed_dev_fixture` command.
   - Seeds one transcript-ready private meeting without microphone, Whisper, or Ollama.
   - Release builds do not register the fixture command.
   - Tests cover search, export, delete, summary with fake Ollama, idempotency, and partial fixture failure.

## Verification Snapshot

Passing checks from the completed slices:

| Check | Result |
| --- | --- |
| `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` | Passed |
| `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --release` | Passed |
| `cargo test --workspace` | Passed |
| `cargo test -p curiosity-transcription` | Passed |
| `npm test -- --run` in `apps/desktop` | Passed |
| `npm run build` in `apps/desktop` | Passed |
| `git diff --check` | Passed |
| `cargo clippy --all-targets --all-features --locked -- -D warnings` | Passed after installing the Rust clippy component. |
| `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --no-run` | Passed with the default `whisper-rs` desktop backend. |

Not verified in this environment:

- Real microphone permission and capture.
- Full system-audio meeting capture in the main recording UI.
- Live Ollama summary generation against a running local Ollama server.
- Packaged app behavior.

## End-To-End Test Plan

### Deterministic fixture path

Use this when you want to validate the app workflow without hardware, Whisper, or Ollama:

1. Run the Tauri app in debug mode.
2. Invoke the debug/test-only `seed_dev_fixture` command from a harness.
3. Confirm the seeded "Dev Fixture Full Cycle" meeting appears with two transcript segments.
4. Search for `deterministic` or `Fixture`.
5. Export JSON and verify the export path.
6. Generate a summary in tests with an injected fake Ollama client, or use real Ollama manually after starting it.
7. Delete private data and verify the meeting disappears while exported JSON remains.

### Real Whisper and Ollama path

1. Install native prerequisites:

   ```sh
   command -v cmake
   ```

   If this prints nothing, install CMake before testing `whisper-rs`.

2. Verify the native Whisper build:

   ```sh
   cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --no-run
   ```

3. Run a real Whisper smoke with an existing WAV:

   ```sh
   CURIOSITY_WHISPER_MODEL=/absolute/path/to/ggml-base.en.bin \
   CURIOSITY_WHISPER_WAV=/absolute/path/to/audio.wav \
   cargo run -p curiosity-transcription --features whisper-rs --bin whisper-smoke
   ```

4. Start Ollama and install the selected local model:

   ```sh
   ollama serve
   ollama pull qwen3.6:27b
   ```

5. Run the desktop app with Whisper enabled:

   ```sh
   cd apps/desktop
   CURIOSITY_WHISPER_MODEL=/absolute/path/to/ggml-base.en.bin npm run tauri:dev
   ```

6. In Settings:
   - Save or confirm the Whisper model path.
   - Confirm Ollama base URL `http://127.0.0.1:11434`.
   - Confirm the Ollama model.
   - Run the Whisper path test and Ollama connection test.

7. Record a microphone meeting, stop recording, transcribe it, generate a local summary, search transcript text, export JSON, and delete private data.

## Remaining Non-MVP Work

- Production packaging and installer flow.
- Calendar integration.
- System-audio capture in the main recording UI.
- Whisper model download/management UI.
- Hosted provider key selection and disclosure UX beyond the guarded backend paths.
