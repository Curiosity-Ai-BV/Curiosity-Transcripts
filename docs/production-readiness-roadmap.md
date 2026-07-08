# Production Readiness Roadmap

Date: 2026-07-07

## Assumptions

- "Production ready" means a non-developer macOS user can install a public DMG,
  complete first-run setup, record a meeting, transcribe locally, summarize
  locally, search, correct, export, and delete private data with clear recovery
  and privacy state.
- The first production target is macOS. The current release path is arm64 DMG;
  x64 or universal macOS support is a product/release decision, not assumed.
- Local-first remains the product invariant. Hosted providers and calendar
  connectors stay opt-in and must not weaken the manual local transcript loop.
- Existing deterministic tests should stay hardware-free, model-free,
  calendar-free, network-free, and explicit about skipped smoke checks.

## Current State

The app is past a paper MVP. The repository already has a working Rust workspace,
a Tauri/Vite desktop app, local recording commands, local Whisper transcription,
SQLite persistence, search, JSON/Markdown/SRT export, private-data delete, local
Ollama summary generation, contract validation, fail-loud smoke paths, and
signed DMG workflow scaffolding.

Important existing strengths:

- `apps/desktop` opens into a transcript workspace with meeting history,
  recording controls, transcript detail, summary, export/delete, and local
  settings.
- `crates/domain`, `crates/audio`, `crates/store`, `crates/transcription`,
  `crates/analysis`, and `crates/app` keep most core logic outside Tauri.
- `crates/store` already owns SQLite migrations, FTS search, transcript
  versions, edit history, exports, analysis results, and a `processing_jobs`
  table.
- CI runs publication checks, workflow checks, root Rust fmt/test/clippy,
  desktop Rust fmt/test/clippy, fail-loud audio and Whisper smoke assertions,
  desktop Vitest, and frontend build.
- Release and Pages workflows build macOS DMGs with Developer ID signing and
  notarization paths, then verify disk image, app signature, stapling, and
  Gatekeeper checks before upload.

The app is not production ready yet. The main blockers are first-run model
setup, production privacy/security hardening, release confidence on real
hardware, feature-matrix verification, signing credentials/process docs, and
deeper maintainability work.

Slice 0 public source-of-truth decisions:

- Shipped desktop export surface: JSON, Markdown, and SRT export through the UI
  and generic `export_meeting` Tauri command; `export_meeting_json` remains as
  the JSON compatibility path.
- Integration export surface: JSON remains the deterministic integration
  format. Markdown and SRT are user-facing transcript formats backed by
  `crates/transcription`.
- Pages download flow: the stable Pages DMG is built as a Developer ID signed
  and notarized artifact, then stapling and Gatekeeper checks run before
  deployment.
- First public release architecture: arm64-only macOS DMG. The versioned
  GitHub Release asset is `macos-aarch64`; x64 and universal builds are out of
  scope until a later slice changes workflows, QA, and public copy together.

## Phase 0: Source Of Truth And Release Criteria

Goal: make the public promise match the product that actually ships.

Deliverables:

- Keep documentation and product behavior aligned around export formats. The
  desktop app exposes JSON/Markdown/SRT, with JSON retained as the deterministic
  integration format.
- Align public site wording with the current signed/notarized Pages DMG flow.
- Add a short release-candidate checklist covering clean-user install, macOS
  permissions, model setup, offline-after-setup behavior, recording,
  transcription, summary, export, delete, and uninstall/private-data handling.
- Document the first public release as arm64-only macOS. Keep release asset
  naming and site copy consistent with that decision unless a later slice adds
  x64 or universal builds.

Success criteria:

- README, docs, public site, release workflows, and actual UI commands describe
  the same supported behavior.
- A future contributor can tell which items are implemented, planned, or manual
  smoke requirements without reading old dated MVP notes.

## Phase 1: First-Run Model And Clean Install Loop

Goal: remove developer-only setup from the core user path.

Deliverables:

- Add a first-run model setup flow for Whisper:
  model discovery, download or file selection, compatibility checks, hash/size
  recording, model health state, and recovery guidance for missing or invalid
  models.
- Add first-run Ollama setup states:
  local server detection, selected model availability, guided pull/install
  instructions or an explicit "summaries unavailable" state.
- Keep the app usable in local transcript mode when Ollama is absent.
- Validate that after model setup the record/transcribe/search/export/delete
  path works with network disabled.
- Run and document a clean macOS user DMG smoke:
  install, launch from `/Applications`, grant microphone and Screen Recording,
  record mic/system audio, transcribe with Whisper, summarize with local
  Ollama, export, delete, and relaunch.

Success criteria:

- A non-developer can reach a transcript without environment variables or repo
  commands.
- Missing models, incompatible models, denied permissions, and unavailable
  Ollama are typed states with actionable UI, not terminal-only setup notes.

## Phase 2: Privacy And Security Hardening

Goal: make the local-first trust promise enforceable in the shipped app.

Deliverables:

- Replace the null Tauri CSP with a restrictive policy appropriate for a local
  transcript renderer, guarded by `scripts/check-tauri-security.js` in
  publication readiness.
- Finish per-meeting privacy controls for raw-audio retention:
  retain, delete after transcription, never save where supported, storage
  location, provider/network use, and remaining exported files after deletion.
- Add an explicit at-rest data strategy. If encryption-at-rest is not in v1,
  document the decision and scope clearly; if it is in v1, introduce it behind
  a tested storage/key-management seam.
- Store future provider keys, OAuth tokens, calendar tokens, and encryption keys
  in the OS keychain, not SQLite or plain settings files.
- Add dependency and security automation:
  Dependabot or Renovate, `cargo audit` or `cargo deny`, `npm audit` or an
  equivalent gate, CodeQL, secret scanning expectations, and SBOM/license output.
- Keep hosted analysis behind explicit key selection and transcript disclosure
  confirmation.

Current Phase 2B status: desktop npm drift is gated by
`npm audit --audit-level=high` in CI, and Dependabot is configured for
`/apps/desktop` npm, root Cargo, desktop Tauri Cargo, and GitHub Actions
updates. CodeQL, cargo audit or cargo deny, SBOM, and license output remain
later Phase 2C hardening unless implemented in a separate slice.

Current Phase 2C visibility status: the desktop detail view exposes the existing
per-meeting privacy data state: private audio storage path, raw-audio retention,
local or hosted processing state, export status, and delete or remaining export
status. Retention controls, encryption/key management, and keychain-backed secrets
remain later Phase 2 work unless implemented in separate slices.

Success criteria:

- A release candidate has no obvious desktop renderer hardening gap.
- The app can show what data exists, where it lives, what left the device, and
  what deletion did not control.
- Dependency/security drift is visible in automation instead of manual review.

## Phase 3: Durable Jobs And Recovery

Goal: make expensive work survive app restarts and crashes without lying to the
user.

Deliverables:

- Current Phase 3B/3C/3D status: transcription and summary job start, cancel,
  finish, and restart recovery now persist through `processing_jobs`; retry UX
  remains later work. CI now gates no-Whisper desktop tests and the
  ScreenCaptureKit system-audio feature compile path on macOS.
- Move transcription and summary job ownership from in-memory Tauri state into
  durable `processing_jobs` records.
- Reconcile running/cancel-requested jobs on startup and mark them recovered,
  failed, canceled, or retryable with visible user state.
- Persist retry count, last error, started/finished timestamps, cancel state,
  and idempotency keys for transcription and summary jobs.
- Keep cancellation semantics deterministic: canceled jobs must not persist
  completed backend output after the cancel boundary.
- Keep feature-matrix verification in CI for default desktop builds,
  no-Whisper builds, and ScreenCaptureKit system-audio compile checks on macOS.
  Real-hardware smoke and release confidence remain manual/later work.

Success criteria:

- Killing the app during transcription or summary generation produces a truthful
  recovery state on relaunch.
- Duplicate commands cannot start conflicting work for the same meeting.
- PR CI or scheduled CI exercises the important feature combinations before a
  release branch is cut.

## Phase 4: Complete The Transcript Workflow

Goal: make the core transcript loop useful after the first successful recording.

Deliverables:

- Current Phase 4A status: transcript segment correction is wired through the
  existing store edit-history seam, desktop command surface, TS contract, and a
  minimal one-segment inline editor.
- Current Phase 4B status: Markdown and SRT are productized beside JSON through
  the app command layer, generic Tauri `export_meeting` command, format-aware
  desktop UI/state, focused Rust/Tauri/TS/React tests, and docs. JSON remains the
  deterministic integration format.
- Current Phase 4C status: local `.wav` import is wired through the desktop
  command surface and existing store/transcription/export seams. The command
  copies a validated user-provided WAV path into app-private meeting storage,
  persists a completed imported recording artifact with the private relative path
  and final copied-file SHA-256, and leaves MP3/M4A, transcoding, drag/drop,
  batch import, metadata extraction, and native file picking out of scope.

Later work: native file picking, richer import metadata, broader import formats,
and more complete correction review workflows.

Success criteria:

- A user can import existing audio, correct obvious transcript mistakes, export
  in all documented formats, and still inspect what raw/private/exported data
  remains.

## Phase 5: Contract And Maintainability Hardening

Goal: reduce drift risk before adding calendar and broader providers.

Deliverables:

- Generate TypeScript command/view DTOs from Rust or lock them with a schema
  artifact that is checked in CI.
- Keep runtime contract validation, but make it a backup rather than the primary
  drift detector.
- Split the large Tauri, store, audio, and desktop UI modules behind their
  current public facades. Do this only after the production seams above are
  stable.
- Add coverage reporting for critical Rust/frontend seams. Avoid chasing a
  vanity percentage; gate the paths that protect privacy, deletion, recovery,
  contracts, provider disclosure, and release metadata.

Current Phase 5A status: the Rust-produced desktop command/view contract fixture
is checked in at `apps/desktop/contracts/desktop-command-view-contract.fixture.json`.
Rust tests guard exact equality against the generated command/view payload, and
TS command adapter contract tests consume the same fixture.

Later work: generated DTOs, module splitting, and coverage reporting.

Success criteria:

- Command DTO drift fails before manual QA.
- The largest files no longer force unrelated recording, settings,
  transcription, summary, search, export, delete, and release work into the same
  edit surface.

## Phase 6: Apple Calendar Context

Goal: add calendar value without introducing silent recording risk.

Deliverables:

- Add Apple Calendar permission and availability states.
- Show upcoming events and allow a user to manually attach a recording to an
  event for naming/context.
- Protect private, all-day, recurring, ambiguous, and overlapping events.
- Keep auto-start disabled until allowlists, ambiguous-event handling, and
  always-visible recording indicators are implemented and tested.

Success criteria:

- Calendar context helps organization without recording automatically or sending
  calendar/transcript data to hosted services.

## Phase 7: Later Intelligence And Providers

Goal: expand only after the local trust loop is dependable.

Candidates:

- Hosted transcription and hosted LLM providers with keychain-backed secrets,
  explicit disclosure, and no coupling to local defaults.
- Local embeddings and semantic search with deterministic rebuild/fallback.
- Speaker labels after the transcript correction/export loop is stable.
- Cross-meeting questions, sentiment/tone, and follow-up drafting with source
  citations and uncertainty.

Do not pull these forward if first-run setup, privacy controls, feature-matrix
release confidence, or contract/maintainability hardening are still incomplete.

## Release Candidate Gate

A production release candidate should pass:

```sh
cargo fmt --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --check
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings
node scripts/check-tauri-security.js
bash scripts/check-publication-readiness.sh
```

```sh
cd apps/desktop
npm ci
npm run test
npm run build
```

Manual or macOS-runner release gates:

```sh
cargo run -p curiosity-audio --bin audio-smoke -- --attempt-mic --out audio-smoke-output --duration-ms 1000
cargo run -p curiosity-audio --features system-audio-screencapturekit --bin audio-smoke -- --attempt-system-audio --out audio-smoke-output --duration-ms 1000
CURIOSITY_WHISPER_MODEL=/absolute/path/to/model.bin CURIOSITY_WHISPER_WAV=/absolute/path/to/audio.wav cargo run -p curiosity-transcription --features whisper-rs --bin whisper-smoke
./scripts/build-macos-dmg.sh --no-sign
./scripts/build-macos-dmg.sh
```

Skipped smoke checks are not passes. If hardware, models, Apple signing, or
Ollama are unavailable, the release notes must say exactly which gates were not
run.
