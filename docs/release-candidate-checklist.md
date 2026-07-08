# Release Candidate Checklist

Use this checklist before treating a build as a public release candidate. The
first public release target is an arm64 macOS DMG only; x64 and universal macOS
builds are out of scope until workflows, QA, and public copy change together.

Skipped smoke checks are not passes. Record the build, machine, macOS version,
model paths, and any skipped item in the release notes.

Tag workflows create or update draft GitHub Releases. Keep the release as a
draft until filled manual smoke evidence validates with
`node scripts/check-release-smoke-evidence.js path/to/filled-evidence.json`;
after that passes, a maintainer must publish the draft manually.

Use `docs/release-candidate-smoke-evidence.template.json` as the starting point
for manual smoke evidence. Validate the checked-in template with no path:
`node scripts/check-release-smoke-evidence.js`. Validate a filled manual
evidence file by passing its path explicitly:
`node scripts/check-release-smoke-evidence.js path/to/filled-evidence.json`.
Template validation is not proof that smoke passed; it only proves the evidence
shape is still checkable. Filled non-template evidence fails validation by
default if any manual item is pending, skipped, or failed, or if required
build/signing/notarization/machine/model status fields remain pending. Use
`--allow-incomplete` only while collecting draft evidence, and never treat a
filled evidence file as a pass when any item or required status field remains
pending, skipped, or failed.

Automated release readiness must include `node scripts/check-tauri-security.js`
and `node scripts/check-tauri-command-surface.js` through
`bash scripts/check-publication-readiness.sh`; the gate fails null or loosened
Tauri renderer CSP values and prevents the debug/test-only `seed_dev_fixture`
command from entering the release invoke handler before manual smoke starts.
It must also run `node scripts/check-plain-secret-storage.js` to guard persisted
settings, app service DTOs, and desktop command/view DTO/contract shape against
plain API key, OAuth/access/refresh/calendar token, encryption key, hosted
provider secret, generic secret, credential, password, or serde rename alias
fields before manual smoke starts.
The same publication readiness gate validates the smoke evidence template and
runs `node scripts/check-release-smoke-evidence.js --self-test` so release
evidence drift fails before manual smoke starts.

Desktop npm dependencies must pass `npm audit --audit-level=high` after
`npm ci`, and `.github/dependabot.yml` must keep npm, Cargo, and GitHub Actions
update automation present. Rust dependencies must pass `cargo audit` at the
repository root and from `apps/desktop/src-tauri`; CI installs that helper with
`cargo install cargo-audit --version 0.22.2 --locked`, and publication readiness
rejects loosening or removing the exact install pin. Warning-class RustSec
advisories should be recorded for dependency triage even when no vulnerable
crate is present. CodeQL scans Rust and JavaScript/TypeScript on push, pull
request, and a weekly schedule with `build-mode: none` to avoid duplicating CI
build/test cost; confirm code scanning alerts are visible before treating a
build as a release candidate. CodeQL visibility does not replace
branch-protection or alert triage policy. GitHub Actions workflow syntax must
pass in CI before manual smoke starts: CI verifies the upstream
`actionlint_1.7.12_linux_amd64.tar.gz` archive checksum and runs
`actionlint -color=false` before publication readiness and the
workflow-specific Pages/release checks. Publication readiness rejects loosening
the pinned actionlint version, checksum, artifact name, command, or ordering.
Supply-chain artifact generation must run through
`node scripts/generate-supply-chain-artifacts.js`; CI uploads
`release-artifacts/supply-chain`, including the desktop npm CycloneDX
application SBOM, npm lockfile license metadata report, and deterministic Cargo
license metadata reports filtered to `aarch64-apple-darwin` for both Rust
dependency graphs. The script normalizes npm SBOM timestamp and serial-number
fields so repeated runs are stable. Treat this as a metadata/reporting check,
not a legal license allowlist. Secret scanning runs through `.github/workflows/secret-scanning.yml`
with the official digest-pinned Gitleaks CLI container
`ghcr.io/gitleaks/gitleaks:v8.30.0@sha256:691af3c7c5a48b16f187ce3446d5f194838f91238f27270ed36eef6359a574d9`,
full git history, redacted output, and default fail-on-detection behavior. The
workflow uses the CLI container instead of the Gitleaks Action so organization CI
does not depend on a `GITLEAKS_LICENSE` secret. GitHub secret scanning, branch
protection, and alert triage policy remain release governance work unless
configured separately. `cargo deny`, Rust CycloneDX tooling, `cargo-about`, and
license allowlists are not part of this gate.

Coverage artifact visibility must run in CI before manual smoke starts. CI
installs the Rust coverage helper with
`cargo install cargo-llvm-cov --version 0.8.7 --locked`, and publication
readiness rejects loosening or removing the exact install pin. The frontend
report is generated by `npm run test:coverage` into
`release-artifacts/coverage/frontend`, Rust LCOV reports are generated under
`release-artifacts/coverage/rust`, and `node scripts/check-coverage-artifacts.js`
must pass before CI uploads `release-artifacts/coverage`. Treat this as report
visibility for critical seams, with no global percentage threshold. The checker
expects LCOV source paths for `apps/desktop/src/App.tsx`,
`apps/desktop/src/commandAdapter.ts`, `crates/store/src/lib.rs`, and
`apps/desktop/src-tauri/src/main.rs`, each with at least one positive `DA` line
in the matching source record; it is not proof of generated DTOs, module splitting,
or complete privacy/deletion/recovery coverage.

## Manual Smoke Items

- Clean-user install: on a clean macOS user account without the development
  checkout, download the signed/notarized arm64 DMG, open it, drag `Curiosity
  Transcripts.app` to `/Applications`, and launch from `/Applications`.
- macOS permissions: verify Microphone and Screen Recording prompts name
  `Curiosity Transcripts`, and denied permissions produce visible recovery
  states instead of silent success.
- Model setup: verify the Settings readiness panel shows missing/readable
  Whisper guidance, treats an existing Whisper file as untested until matching
  `Test path` evidence exists, and shows Ollama availability as unknown until
  matching `Test Ollama` evidence exists. Verify the manual setup options list
  existing-file Whisper setup and local Ollama candidate tags without starting
  downloads, pulls, saves, or tests.
  Configure a local Whisper model path in Settings and record the path test's
  file size plus SHA-256 readability evidence. Run a sample transcription and
  confirm the separate last-successful-transcription evidence appears for the
  same model file size and modified time without changing the Test path
  readiness requirement. Treat the real Whisper smoke or a sample transcription
  as the compatibility check. Confirm local Ollama base URL/model state, record
  the installed Ollama model evidence reported by `/api/tags`, and verify
  missing models show the suggested `ollama pull <model>` command as manual setup
  guidance. Actual Ollama model pulls remain manual for now.
- Offline-after-setup: after Whisper and Ollama setup, disable network access
  and confirm the local record, transcribe, search, JSON/Markdown/SRT export,
  and delete path still works. Local Ollama may require the local server to
  remain running.
- Recording: start and stop a short recording with microphone plus
  ScreenCaptureKit system audio where permissions allow; verify private WAV
  artifacts and truthful mic-only fallback when system audio is unavailable.
- Import WAV: enter a local `.wav` source path, import it, verify the copied
  artifact is stored under app-private meeting storage, then delete the meeting
  and confirm the original source file remains untouched.
- Transcription: transcribe the recorded meeting with the configured local
  Whisper model and verify channel-tagged transcript segments appear.
- Durable job recovery: during one transcription and one summary run, quit and
  relaunch the app to verify durable job recovery shows a truthful recovered,
  retryable, failed, or completed state.
- Correction: edit one transcript segment, save it, relaunch, and verify the
  corrected text plus original-text indication are still visible.
- Summary: generate a local Ollama summary after transcription and verify
  summary, decisions, questions, action items, and citations are visible.
- Privacy data state: for a selected meeting, verify the detail row shows the
  app-private audio path, captured raw-audio retention, local or hosted
  processing state, export status, and delete or remaining-export status. Verify
  Settings can save `Retain` and `DeleteAfterTranscription` as defaults for
  future recordings/imports, and that a successful delete-after transcription
  removes safe app-private raw WAV artifacts while preserving transcript,
  summary/search/export rows, meetings, and user-owned source files. `NeverSave`
  capture is unsupported and should not appear in the UI.
- At-rest disclosure: verify the release notes disclose that app-level
  encryption-at-rest is not implemented in v1, app-private storage relies on
  OS/user-account file protections, app delete controls app-private meeting data,
  and user-owned source files plus exported files can remain outside app delete
  control.
- Export: use the desktop export action for JSON, Markdown, and SRT, and verify
  each exported file path and format-specific status is reported. Treat JSON as
  the deterministic integration format.
- Contract fixture: confirm
  `apps/desktop/contracts/desktop-command-view-contract.fixture.json` and
  `apps/desktop/contracts/desktop-command-view-contract.schema.json` are present
  in the build source. Run `node scripts/check-desktop-command-view-contract.js`
  plus the Rust/TS contract checks before manual smoke starts. Treat this as a
  fixture-derived shape lock, not generated DTO ownership.
- Delete: delete the meeting and verify app-private transcript, analysis,
  manifests, meeting-scoped private DB rows, `processing_jobs`, and
  `meeting_search` rows are removed. Verify app-private audio artifacts are
  removed or explicitly reported as skipped. If cleanup is interrupted after a
  recorded delete intent or raw-audio tombstone, relaunch and verify startup finalizes pending
  app-private cleanup; user-owned exports remain outside app deletion control.
- Uninstall and private-data handling: uninstall the app, inspect the documented
  app-private data location, and verify user-owned exported files remain
  outside app deletion control.
