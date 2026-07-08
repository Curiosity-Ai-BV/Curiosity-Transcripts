# Release Candidate Checklist

Use this checklist before treating a build as a public release candidate. The
first public release target is an arm64 macOS DMG only; x64 and universal macOS
builds are out of scope until workflows, QA, and public copy change together.

Skipped smoke checks are not passes. Record the build, machine, macOS version,
model paths, and any skipped item in the release notes.

Automated release readiness must include `node scripts/check-tauri-security.js`
through `bash scripts/check-publication-readiness.sh`; the gate fails null or
loosened Tauri renderer CSP values before manual smoke starts.

Desktop npm dependencies must pass `npm audit --audit-level=high` after
`npm ci`, and `.github/dependabot.yml` must keep npm, Cargo, and GitHub Actions
update automation present. CodeQL, cargo audit or cargo deny, SBOM, and license
output are still future hardening gates unless a later slice adds them.

## Manual Smoke Items

- Clean-user install: on a clean macOS user account without the development
  checkout, download the signed/notarized arm64 DMG, open it, drag `Curiosity
  Transcripts.app` to `/Applications`, and launch from `/Applications`.
- macOS permissions: verify Microphone and Screen Recording prompts name
  `Curiosity Transcripts`, and denied permissions produce visible recovery
  states instead of silent success.
- Model setup: configure a local Whisper model path in Settings and record the
  path test's file size plus SHA-256 readability evidence. Treat the real
  Whisper smoke or a sample transcription as the compatibility check. Confirm
  local Ollama base URL/model state, record the installed Ollama model evidence
  reported by `/api/tags`, and verify missing models show the suggested
  `ollama pull <model>` command as manual setup guidance. Actual Ollama model
  pulls remain manual for now.
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
- Correction: edit one transcript segment, save it, relaunch, and verify the
  corrected text plus original-text indication are still visible.
- Summary: generate a local Ollama summary after transcription and verify
  summary, decisions, questions, action items, and citations are visible.
- Privacy data state: for a selected meeting, verify the detail row shows the
  app-private audio path, raw-audio retention, local or hosted processing state,
  export status, and delete or remaining-export status. Retention controls,
  encryption-at-rest, and keychain secrets remain future checks until implemented.
- Export: use the desktop export action for JSON, Markdown, and SRT, and verify
  each exported file path and format-specific status is reported. Treat JSON as
  the deterministic integration format.
- Delete: delete the meeting and verify app-private transcript, analysis, and
  audio artifacts are removed or explicitly reported as skipped.
- Uninstall and private-data handling: uninstall the app, inspect the documented
  app-private data location, and verify user-owned exported files remain
  outside app deletion control.
