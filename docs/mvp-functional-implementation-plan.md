# MVP Functional Implementation Plan

Date: 2026-05-21

This plan turns the current Rust foundation into a usable macOS-first MVP. It is scoped to the README gaps: desktop UI, real macOS microphone/system-audio capture, and real Whisper transcription.

## Assumptions

- macOS is the first supported platform.
- The desktop app uses Tauri 2 with a React/Vite frontend.
- Default automated tests stay deterministic and do not require microphone hardware, Screen Recording permission, Whisper model files, Ollama, hosted keys, or network access.
- Hardware and model checks are explicit smoke paths and must report `NotRun`, `Skipped`, `Unavailable`, or `PermissionDenied` honestly when prerequisites are missing.
- System audio is implemented through a macOS-specific adapter where available. Mic-only recording remains a working fallback when system audio is unavailable or denied.
- Local Whisper uses a user-provided model path first. Model download/management is separate from transcription execution.

## MVP Acceptance

- A user can open the desktop app and see the transcript workspace, not a landing page.
- A user can start and stop a microphone recording on macOS and the app stores a private audio artifact with device/sample metadata.
- System-audio capture is either functional on permissioned macOS hardware or represented as a typed unavailable/permission state with clear recovery guidance; it is never silently treated as passing.
- A user can configure a local Whisper model path and transcribe a saved WAV artifact into persisted transcript segments.
- Meeting detail shows transcript, source/storage/privacy state, exports, and structured summary controls already supported by the Rust foundation.
- Export/delete/search/summary flows remain available through the desktop shell.
- Hosted/network analysis remains opt-in and visibly gated.

## Slice 1: Local Whisper Transcription

Owner: transcription crate and narrow app command integration.

Tests first:

- Missing model path returns setup guidance, not a crash.
- Unsupported audio input returns a typed user-facing failure.
- A fake Whisper backend maps timestamped output into ordered transcript segments with provider, model, source hash, model run id, and transcript version id.
- Optional real Whisper smoke runs only when `CURIOSITY_WHISPER_MODEL` and an input WAV are provided, and skipped smoke is not reported as passed.

Acceptance:

- `cargo test --workspace` passes without a model file.
- The crate exposes a real local Whisper transcriber path for WAV artifacts.
- The app layer can request transcription of a stored artifact and persist the transcript through existing store APIs.

## Slice 2: macOS Audio Capture

Owner: audio crate and narrow app recording integration.

Tests first:

- Capture configuration validates mic-only, system-only, and mixed requests.
- Permission errors map to actionable guidance for microphone and Screen Recording.
- A fake streaming recorder proves start/stop writes a recoverable WAV artifact and manifest metadata.
- Hardware smoke reports explicit status and never claims success when hardware or permission prerequisites are absent.

Acceptance:

- On macOS hardware, mic capture can write `raw-mic.wav` with sample rate, channel count, device identity, start time, duration, and sha256.
- System-audio capture is wired through a macOS adapter where feasible; denied/unavailable states are typed and visible.
- Mic-only recording remains usable when system audio is denied or unavailable.
- Existing fake recording tests continue to pass.

## Slice 3: Desktop UI

Owner: `apps/desktop`.

Design direction:

- Use a quiet, utilitarian transcript workspace. No marketing landing page, decorative hero, or inactive future-feature controls.
- Prioritize dense but readable panes: meeting list/search, recording strip, meeting detail transcript, summary/export/delete/privacy state, and settings.
- Use neutral colors with clear status accents; avoid purple/blue gradient themes.
- Use icon buttons for tools where appropriate and keep controls stable across desktop and narrow widths.

Tests first:

- UI contract tests cover command-state mapping for recording, permissions, model status, search, export, delete, and summary.
- Component tests cover empty, loading, unavailable, permission-denied, recording, transcribing, and ready states.
- Build verification catches TypeScript/CSS regressions.

Acceptance:

- `npm run build` succeeds for the desktop frontend.
- Tauri command wrappers compile and expose only implemented backend capabilities.
- The first screen supports the core local workflow without dead controls.

## Slice 4: End-To-End MVP Integration

Owner: app crate plus Tauri command wiring.

Tests first:

- A fake end-to-end flow creates a meeting, records/imports audio, transcribes, indexes, searches, exports, analyzes, and deletes.
- Failure states for missing mic permission, missing Whisper model, unavailable Ollama, and hosted-analysis gating are visible through command DTOs.

Acceptance:

- `cargo test --workspace` passes.
- Desktop build succeeds.
- Manual smoke instructions document how to run mic capture, Whisper transcription, and the Tauri app locally.
