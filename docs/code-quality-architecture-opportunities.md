# Code Quality and Architecture Opportunities

Date: 2026-05-27

This audit treats the MVP as functionally close, and focuses on maintainability, reliability, and architectural seams that are likely to become expensive as the app moves from MVP to daily-use desktop software.

## Scope and Assumptions

- The review covered the Rust workspace crates, the separate Tauri backend crate, the React command shell, CI/release scripts, and current project docs.
- No implementation changes were made as part of this audit.
- Findings are prioritized by likely product risk and future maintenance cost, not by how hard the change is.
- The app should stay simple. Several proposals intentionally recommend narrow seams, typed boundaries, and small module splits instead of broad rewrites.

Implementation status:
The audit below is the pre-implementation assessment. Follow-up hardening work
has addressed the P1, P2, and P3 items with focused implementation commits.
The finding bodies remain historical evidence and proposals; check current code,
README, `CONTRIBUTING.md`, and release docs before treating any item as open.

## Priority Key

- P1: High confidence issue or architectural weakness that can cause incorrect persistence, false operational confidence, or blocked release behavior.
- P2: Maintainability or runtime-risk issue that should be addressed before expanding feature scope.
- P3: Cleanup or structure improvement that is useful, but should not interrupt reliability work.

## Recommended Sequence

1. Protect live recording and recovery invariants:
   block delete of the active meeting, make shutdown/cancel explicit, prevent repair from reviving deleted/tombstoned artifacts, and make mixed capture require requested system-audio evidence.

2. Strengthen truthfulness of verification:
   make zero-segment Whisper smoke non-passing, validate DMGs before publishing, and make local contributor checks match CI.

3. Stabilize error and DTO contracts:
   introduce typed store/app errors, replace shallow frontend contract checks with real schema validation, and add a non-copy readiness field to the desktop snapshot.

4. Split high-risk oversized modules along existing responsibility boundaries:
   start with `crates/store/src/lib.rs`, `crates/audio/src/lib.rs`, and `apps/desktop/src-tauri/src/main.rs`. Keep public facades stable while moving internals.

5. Add job ownership for expensive work:
   transcription and summary generation should become explicit jobs with status, cancellation, and a blocking boundary instead of synchronous command work.

## P1 Findings

### 1. Active Recording Can Be Deleted While Still Owned by the Backend

Evidence:
- `delete_meeting` acquires `DesktopCommandState` and calls deletion directly in `apps/desktop/src-tauri/src/main.rs:156`.
- `delete_meeting_for_app_root` deletes without checking `command_state.active_recording` in `apps/desktop/src-tauri/src/main.rs:581`.
- The active recorder remains owned in `DesktopCommandState.active_recording` in `apps/desktop/src-tauri/src/main.rs:861`.
- Later stop/finalization still runs through `stop_active_microphone_recording` in `apps/desktop/src-tauri/src/main.rs:1195`.

Why it matters:
Deleting the meeting that currently owns an active recorder can leave memory state and persisted state disagreeing. A later stop can attempt to finalize a meeting/session that was already tombstoned or removed. This is exactly the class of bug that is hard to see from UI tests because the UI usually disables unsafe paths, but the backend command still accepts the request.

Proposal:
- Reject `delete_meeting` when `active_recording.meeting_id == meeting_id`.
- If product wants delete to act as cancel, make that explicit: take the active recorder, stop or cancel it, persist failed/canceled state, then tombstone.
- Add a backend regression that starts a fake recording, calls delete for the active meeting, and asserts the active recorder is not orphaned and no deleted meeting is later revived.

### 2. Startup Repair Can Recover Artifacts After Delete Intent

Evidence:
- Delete intent marks the meeting `Deleted` and tombstones artifacts in `crates/store/src/lib.rs:1902`.
- `repair_startup` scans recoverable manifests in `crates/store/src/lib.rs:760`.
- `db_artifact_for_repair` loads artifact/session data but does not load meeting status, retained, or tombstoned state in `crates/store/src/lib.rs:2003`.
- Repair then marks artifacts recovered, sessions recovered, and running jobs as recovery in `crates/store/src/lib.rs:841`, `crates/store/src/lib.rs:855`, and `crates/store/src/lib.rs:887`.

Why it matters:
If the app crashes after delete intent commits but before manifest/file cleanup completes, the next launch can treat stale recoverable evidence as authoritative. That undermines the delete/privacy contract.

Proposal:
- Include `meetings.status`, `audio_artifacts.retained`, and `audio_artifacts.tombstoned` in the repair query.
- Skip or report a `RepairConflict` for deleted/tombstoned artifacts instead of recovering them.
- Add a regression that simulates a crash between delete intent and manifest deletion, then asserts repair does not recover artifacts, sessions, or jobs.

### 3. Mixed Desktop Recording Can Complete Without System-Audio Evidence

Evidence:
- Mixed desktop recording starts `StreamingWavRecorder` with `CaptureConfiguration::mixed()` in `crates/audio/src/lib.rs:1723`.
- ScreenCaptureKit starts at `crates/audio/src/lib.rs:1848`.
- On stop, `run_desktop_audio_writer` always validates microphone evidence, but only validates system-audio evidence if system samples were written or system errors were recorded in `crates/audio/src/lib.rs:2095`.

Why it matters:
If ScreenCaptureKit produces neither samples nor callbacks, mixed recording can produce a complete-looking manifest with only microphone output. That creates false user confidence and bad downstream transcription evidence.

Proposal:
- When the requested config is mixed, call `system_audio_capture_stream_result(wrote_system_samples, &system_errors)` unconditionally before returning a complete manifest.
- If the desired product behavior is fallback to microphone-only, persist that as an explicit degraded outcome rather than a complete mixed recording.
- Add a deterministic writer test for "mixed requested, mic samples written, no system samples or system errors" and assert it is not a successful mixed artifact.

### 4. Real Whisper Smoke Passes With Zero Segments

Evidence:
- `run_real_whisper_smoke` maps any successful backend result to `WhisperSmokeStatus::Passed` in `crates/transcription/src/lib.rs:790`.
- The reported `segment_count` can be `0` in `crates/transcription/src/lib.rs:792`.

Why it matters:
For a smoke test, "model loaded but produced no transcript" is weak evidence. It can hide bad model/WAV/pipeline combinations while still reporting success.

Proposal:
- Treat zero segments as `Failed` or a distinct non-passing status such as `NoSpeechDetected`.
- Add a library/CLI test that a fake real-backend path returning zero segments is not counted as passed.

### 5. Desktop Contract Readiness Is Controlled by Presentation Copy

Evidence:
- Frontend readiness depends on the exact string `Connected to local desktop commands.` in `apps/desktop/src/App.tsx:72` and `apps/desktop/src/App.tsx:100`.
- The backend emits that same display string in `apps/desktop/src-tauri/src/main.rs:480`.

Why it matters:
A harmless copy change can disable every command even when `fetchCommand` exists and Tauri is available.

Proposal:
- Add `commandSurface.ready: boolean` or `commandSurface.mode: "connected" | "preview" | "unavailable"`.
- Keep `detail` as presentation copy only.
- Add a frontend contract test proving copy changes do not affect command readiness.

### 6. Desktop DTO Contract Is Duplicated and Shallowly Validated

Evidence:
- Frontend DTOs are declared manually in `apps/desktop/src/commandAdapter.ts:19`.
- Backend views are independently declared in `apps/desktop/src-tauri/src/main.rs:2226`, `apps/desktop/src-tauri/src/main.rs:2406`, and `apps/desktop/src-tauri/src/main.rs:2523`.
- Runtime validation checks required paths in `apps/desktop/src/commandAdapter.ts:650`, but `requireContractPath` only checks presence/object/array shape in `apps/desktop/src/commandAdapter.ts:785`.

Why it matters:
The current contract check catches missing fields, but not enum drift, string/null mismatches, number/string swaps, or optional fields becoming nullable.

Proposal:
- Prefer generated TypeScript from Rust DTOs if that can stay lightweight.
- If generation is too much for now, define one runtime schema in `commandAdapter` that validates types and enum values.
- Make UI tests exercise the production adapter path, not only injected raw `CommandFetcher` results.

### 7. Release Tag Workflow Does Not Enforce All Documented Version Sources

Evidence:
- `scripts/check-release-workflow.js` validates `tauri.conf.json` version in `scripts/check-release-workflow.js:88`.
- The release workflow tag staging checks package, desktop Cargo, and package-lock versions in `.github/workflows/release.yml:40`, but not `apps/desktop/src-tauri/tauri.conf.json`.

Why it matters:
A tag push can publish a DMG whose app metadata version drifts from the tag, even though the repository has a script that knows how to detect that.

Proposal:
- Run `node scripts/check-release-workflow.js` inside `.github/workflows/release.yml` before building/staging assets.
- Keep version validation in one script instead of duplicating partial checks in shell.

### 8. Contributor Gate Omits the Separate Desktop Rust Backend

Evidence:
- `CONTRIBUTING.md:9` asks contributors to run root `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`, then frontend npm checks.
- The README correctly notes that the Tauri backend is a separate manifest at `README.md:182`.
- CI separately runs desktop Rust format/test/clippy in `.github/workflows/ci.yml:58`.

Why it matters:
Contributors can follow the documented local gate and still miss the desktop backend crate that CI requires.

Proposal:
- Add the desktop backend commands from CI to `CONTRIBUTING.md`.
- Better: add `scripts/check-ci-local.sh` or similar and have both docs and CI point to the same command list.

## P2 Findings

### 9. Store Errors Are Erased at the Library Boundary

Evidence:
- `StoreResult<T>` is `Result<T, Box<dyn std::error::Error + Send + Sync>>` in `crates/store/src/lib.rs:15`.
- Store APIs cover SQLite, filesystem, JSON, path safety, schema migration, replay conflicts, not-found cases, and repair conflicts.
- Several domain conflicts are string errors, for example `crates/store/src/lib.rs:1322`, `crates/store/src/lib.rs:1509`, and `crates/store/src/lib.rs:1720`.

Why it matters:
Callers cannot reliably distinguish corruption, unavailable storage, unsafe paths, migration failure, conflict, or not-found. Tests end up matching substrings instead of variants.

Proposal:
- Introduce `StoreError` with variants such as `Io`, `Sqlite`, `Serde`, `ReplayConflict`, `UnsafePath`, `NotFound`, `RepairConflict`, and `InvariantViolation`.
- Preserve current `Display` text for user-facing messages while allowing callers/tests to match variants.
- Convert at the app/Tauri boundary to user-safe DTOs.

### 10. App Command Error Layering Is Inconsistent

Evidence:
- `AppResult<T> = Result<T, RecordingError>` is declared in `crates/app/src/lib.rs:14`.
- Non-recording helpers return `curiosity_store::StoreResult` directly, including `meeting_detail_dto` in `crates/app/src/lib.rs:173`, `list_meetings_dto` in `crates/app/src/lib.rs:199`, and `generate_summary_command` in `crates/app/src/lib.rs:268`.

Why it matters:
The app crate is becoming the shell-facing orchestration boundary, but it does not yet own one command error contract. That makes it harder to keep frontend failures consistent as features expand.

Proposal:
- Add `AppError` or `CommandError` with variants for store, recording, transcription, analysis, and validation failures.
- Keep recording-specific trust DTOs where useful, but wrap them under a common command boundary.

### 11. Transcript Correction Is Not Transactionally Atomic

Evidence:
- `correct_transcript_segment` inserts edit history, updates segment text, updates version metadata, queries meeting ID, and refreshes search without a transaction in `crates/store/src/lib.rs:1585`.
- Transcript persistence uses a transaction in `crates/store/src/lib.rs:1295`, showing the codebase already values atomic transcript state changes.

Why it matters:
A mid-path SQLite or FTS error can leave edit history, segment text, version timestamp, and search index out of sync.

Proposal:
- Wrap correction writes and search refresh in one transaction.
- If search is intentionally eventually consistent, isolate search refresh so DB mutation success is not returned as a failure after the correction is already committed.
- Add a failure-path test that proves no partial correction state survives.

### 12. Manifest Discovery Follows Surprising Filesystem Entries

Evidence:
- `manifest_paths` uses `entry.path().join("manifest.json")` and `path.exists()` in `crates/store/src/lib.rs:2290`.
- `exists()` follows symlinks.
- Callers read or delete those manifests during repair/delete flows in `crates/store/src/lib.rs:762` and `crates/store/src/lib.rs:1985`.

Why it matters:
Artifact paths have containment checks, but manifest files themselves do not get the same treatment. A symlinked `manifest.json` under `meetings` can point outside app storage.

Proposal:
- Use `symlink_metadata`.
- Require a regular file.
- Canonicalize and require the manifest path to stay under `canonical_app_root/meetings`.
- Add a Unix test with `meetings/<id>/manifest.json` symlinked outside the app root.

### 13. Long-Running Work Has No Job Ownership or Cancellation

Evidence:
- `transcribe_meeting` invokes synchronous Whisper work through `apps/desktop/src-tauri/src/main.rs:344` and `apps/desktop/src-tauri/src/main.rs:1367`.
- `RealWhisperBackend::transcribe` loads/resamples audio, initializes Whisper, and calls `state.full(...)` synchronously in `crates/transcription/src/lib.rs:391`.
- `generate_summary` performs blocking Ollama work through `apps/desktop/src-tauri/src/main.rs:171` and `apps/desktop/src-tauri/src/main.rs:1831`.
- The `ureq` transport has read timeouts up to 120 seconds in `apps/desktop/src-tauri/src/main.rs:1929`.

Why it matters:
The MVP can tolerate simple synchronous commands, but transcription and analysis are natural background jobs. Without job IDs, status, cancellation, and a blocking boundary, the app will become harder to reason about under repeated user actions, app shutdown, or large recordings.

Proposal:
- Introduce explicit jobs: `{ job_id, meeting_id, kind, state, progress, cancel_requested }`.
- Run Whisper/LLM work behind a blocking boundary owned by the app/Tauri layer.
- Surface status via polling or events.
- Persist enough job state to recover honestly after restart.

### 14. Active Recorder Shutdown and Cancellation Are Implicit

Evidence:
- Tauri setup stores `DesktopCommandState` and runs the app in `apps/desktop/src-tauri/src/main.rs:39`.
- `ActiveDesktopRecording` owns the live recorder handle in `apps/desktop/src-tauri/src/main.rs:907`.
- Audio handles finalize through consuming `stop(...)` methods, for example `crates/audio/src/lib.rs:1496`, `crates/audio/src/lib.rs:1913`, and `crates/audio/src/lib.rs:2233`.

Why it matters:
If the window/app exits or a handle is dropped while recording, finalization depends on incidental drop/channel behavior and startup repair. That is weaker than an explicit owner that marks complete, canceled, or failed.

Proposal:
- Add an exit/window-close handler that takes the active recorder and performs bounded stop or cancel.
- Add `cancel(reason)` or equivalent to recording handles, writing a non-complete manifest state.
- Add tests around "dropped/canceled recording never leaves successful-looking artifacts."

### 15. Command Mutex Is Held Across Disk and Store Work

Evidence:
- `rename_meeting`, `export_meeting_json`, and `delete_meeting` lock `DesktopCommandState` before invoking helpers in `apps/desktop/src-tauri/src/main.rs:138`, `apps/desktop/src-tauri/src/main.rs:152`, and `apps/desktop/src-tauri/src/main.rs:166`.
- Those helpers open/migrate/repair the store and perform filesystem work before returning snapshots.

Why it matters:
This can delay the command that matters most during live recording: stop/finalize. It also makes command-state ownership harder to audit.

Proposal:
- Read the minimal state needed under lock.
- Drop the lock for store/filesystem work.
- Reacquire only to publish command outcome state and snapshot metadata.

### 16. Artifact Storage Ownership Is Split Across App, Store, and Audio

Evidence:
- App defines `ArtifactSink` in `crates/app/src/lib.rs:433`.
- Audio defines `ArtifactManifest` in `crates/audio/src/lib.rs:600`.
- Store defines a separate manifest shape in `crates/store/src/lib.rs:2209`.

Why it matters:
The recovery/delete path crosses all three layers. Multiple manifest concepts make it easier for behavior to drift.

Proposal:
- Pick one lower-layer owner for artifact path/manifests.
- Have app orchestration consume abstractions, but avoid redefining manifest storage concepts.
- If two manifest shapes are still needed, document the boundary and keep conversion explicit and tested.

### 17. Large Modules Are Hiding Responsibility Boundaries

Evidence:
- `apps/desktop/src-tauri/src/main.rs` is 4,479 lines.
- `crates/audio/src/lib.rs` is 2,996 lines.
- `crates/store/src/lib.rs` is 2,704 lines.
- `apps/desktop/src/App.tsx` is 969 lines.

Why it matters:
The problem is not line count by itself. These files mix behaviors where state-machine correctness matters: delete plus repair, capture plus smoke, DTOs plus command orchestration, and UI command dispatch plus rendering.

Proposal:
- Split `crates/store` into internal modules like `schema`, `manifest`, `repair`, `delete`, `transcript`, `settings`, and `search`, keeping `Store` as the facade.
- Split `crates/audio` into internal modules like `types`, `streaming_wav`, `macos_mic`, `macos_system`, `macos_desktop`, `smoke`, and `drift`.
- Split Tauri backend into command modules: `snapshot`, `recording`, `transcription`, `analysis`, `settings`, `export_delete`, and `views`.
- Split React app only along current seams: `useDesktopWorkspaceState`, `MeetingPane`, `MeetingDetail`, and `SettingsPane`. Avoid adding a state library.

### 18. Frontend Commands Leak Past the Adapter Boundary

Evidence:
- `getDesktopCommandFetcher` validates snapshot-returning Tauri commands in `apps/desktop/src/commandAdapter.ts:825`.
- `App` still calls raw command strings and casts results to `DesktopSnapshot` in `apps/desktop/src/App.tsx:215` and `apps/desktop/src/App.tsx:310`.

Why it matters:
Most UI tests can inject simple functions that return snapshots without exercising production adapter validation. That lowers confidence in the command bridge.

Proposal:
- Expose typed methods from `commandAdapter`, such as `commands.startRecording(title)`, `commands.renameMeeting(id, title)`, and `commands.generateSummary(id)`.
- Inject that typed command facade into `App`.
- Keep tests fast by using a fake facade, but add adapter integration tests that validate production command mapping.

### 19. Release and Packaging Commands Can Drift

Evidence:
- `scripts/build-macos-dmg.sh` runs `npm ci`, tests, `tauri build --ci`, and packaging in `scripts/build-macos-dmg.sh:9`.
- `apps/desktop/package.json:16` exposes direct release-ish build scripts that bypass `npm ci` and `--ci`.
- `scripts/package-macos-dmg.sh` creates a DMG in `scripts/package-macos-dmg.sh:42`, but does not verify or attach it before reporting success.
- `.github/workflows/release.yml:59` checks that a DMG exists and copies/checksums it, but does not verify installer integrity.

Why it matters:
Release workflows are already important for this repo. Multiple command surfaces with slightly different guarantees will eventually produce confusing release failures.

Proposal:
- Make npm release scripts delegate to the root release script, or clearly mark them as developer shortcuts.
- Add `hdiutil verify "$DMG_PATH"` after creation.
- Add a read-only attach check that confirms `Curiosity Transcripts.app` exists before upload.
- Derive release asset architecture from `uname -m`, or assert the runner architecture before naming the release `macos-aarch64`.

## P3 Findings

### 20. Workspace Policy Is Under-Centralized

Status, 2026-05-31: addressed by follow-up P3 workspace-policy hardening.

Evidence:
- Root `Cargo.toml` only defines members and package metadata.
- Each crate repeats `version = "0.1.16"` and `edition = "2021"`.
- There are no workspace lint settings for common hygiene.

Proposal:
- Move shared version and edition into `[workspace.package]`.
- Consider `[workspace.dependencies]` for common dependencies once version drift appears.
- Add workspace lints gradually. Do not turn on `missing_docs` or strict pedantic lints all at once.

### 21. Domain Transitions Silently Ignore Invalid Inputs

Status, 2026-05-31: addressed by follow-up P3 domain-transition hardening.

Evidence:
- `Meeting::start_recording` mutates only if session meeting ID matches in `crates/domain/src/lib.rs:50`.
- Similar patterns exist in `mark_interrupted` and `mark_recovered`.

Why it matters:
Silent no-ops can hide bugs at command/store boundaries.

Proposal:
- Return a small `DomainTransitionError` for mismatched aggregate IDs or invalid state.
- If that is too disruptive, make these helpers private and enforce transitions at the app/store boundary.

### 22. Public Crate APIs Need Minimal Boundary Docs

Status, 2026-05-31: addressed by follow-up P3 boundary-doc hardening.

Evidence:
- Several crates start directly with `use` statements and expose many public DTOs/traits without crate-level docs, including `crates/domain/src/lib.rs:1`, `crates/analysis/src/lib.rs:1`, and `crates/app/src/lib.rs:1`.

Proposal:
- Add concise `//!` crate-level docs explaining each crate's responsibility and what it should not own.
- Add doc comments for public traits and error types first.
- Use docs to defend boundaries, not to narrate implementation.

### 23. Historical Status Docs Can Mislead New Work

Status, 2026-05-31: addressed by follow-up P3 docs-status alignment.

Evidence:
- `docs/mvp-status-and-full-cycle-plan.md` still describes some release/packaging work as outside or remaining even though release/CI workflows now exist.
- README sections around remaining gaps should be kept aligned with the current CI/release state.

Proposal:
- Mark older planning docs as historical snapshots, or update their status blocks.
- Keep README as the current onboarding source of truth.

## Suggested Regression Tests

- Backend delete while recording:
  start a fake recording, call backend delete for that meeting, assert delete is rejected or cancel semantics are explicit, then assert stop cannot revive deleted state.

- Repair after delete intent:
  persist a recoverable manifest, commit delete intent/tombstone state, leave manifest behind, run `repair_startup`, assert no artifacts/jobs/sessions are recovered.

- Mixed capture evidence:
  simulate mixed writer stop with microphone samples but no system samples/errors, assert the result is failed/degraded rather than complete mixed capture.

- Zero-segment Whisper smoke:
  simulate backend success with zero segments, assert smoke status is non-passing.

- DTO contract schema:
  feed snapshots with wrong enum values, wrong scalar types, and nulls in required fields, assert the adapter rejects them.

- Desktop root bootstrap:
  test successful Tauri snapshot load and failed Tauri load without falling back to preview meetings.

- DMG verification:
  add a script-level check that `hdiutil verify` and read-only attach pass before release upload.

## Notes on What Is Already Working Well

- The repo already has broad deterministic Rust, frontend, smoke fail-loud, Pages, and release checks in CI.
- The store already uses transactions for several sensitive paths, especially recording start and transcript persistence.
- The frontend has strong workflow tests; the gap is contract ownership, not basic interaction coverage.
- The codebase already separates core crates from the Tauri backend, which gives a good foundation for the module and boundary cleanup proposed above.
