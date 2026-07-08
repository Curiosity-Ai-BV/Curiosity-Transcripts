# At-Rest Data Strategy

Date: 2026-07-08

## V1 Decision

Curiosity Transcripts v1 uses app-private local storage and relies on the
operating system and user account for file protection. App-level
encryption-at-rest is not implemented yet.

This means the SQLite database, private meeting audio artifacts, transcripts,
analysis results, app settings, and export artifacts are not encrypted by a
Curiosity-specific database or file encryption layer in v1. The app should not
claim otherwise in product copy, release notes, or support material.

## Data Boundaries

App-private local data is data the desktop app creates or copies into its own
storage area for the transcript workflow. It is controlled by the app's meeting
delete and storage-repair behavior.

User-owned files are outside that app-private boundary. They include source
files selected for import and exported files written under a configured export
directory. Deleting app-private meeting data must not imply deletion of original
source files or user-owned exports unless the app explicitly owns those files
and reports that action.

## Current Data At Rest

Known v1 local data includes:

- SQLite database rows for meetings, recording sessions, audio artifact
  manifests, transcript versions, transcript edits/history, exports, search
  indexes, processing jobs, and analysis results.
- Private meeting audio artifacts, including recorded or copied WAV files under
  app-private meeting storage.
- Transcript text, corrected transcript text, edit history, and generated
  analysis output.
- App settings such as local Whisper model path, local Ollama base URL, and
  selected local Ollama model.
- User-requested exports under the configured export directory, including JSON,
  Markdown, and SRT transcript exports.
- Logs only where existing desktop, OS, build, or test tooling writes them; logs
  must not be treated as a secret-storage location.

## Data Not Stored Today

The app does not deliberately store provider API keys, OAuth tokens, calendar
tokens, encryption keys, or hosted provider secrets in SQLite, app settings,
plain JSON files, or logs.

If a future feature needs any of those secrets, adding the feature must include
the secure-storage boundary described below.

## Future Keychain Boundary

Provider API keys, OAuth tokens, calendar tokens, encryption keys, hosted
provider secrets, and similar credentials must use the operating system
keychain or equivalent secure storage.

These secrets must not be stored in SQLite, app settings, plain JSON files,
exported transcript files, or logs. Non-secret metadata may live in SQLite when
needed, but the secret value itself belongs in the OS keychain boundary.

## Future Encryption Boundary

If app-level encryption-at-rest is introduced later, it needs a tested storage
and key-management seam before product copy can claim encryption support. That
work must include migration, recovery, delete, backup/restore, and failure-mode
tests.

Encryption keys must be protected by the OS keychain or equivalent secure
storage. The current v1 storage model does not include that encryption seam.

## Release Notes And Manual Smoke

Release notes for v1 must disclose that app-level encryption-at-rest is not
implemented. They should explain that local app-private data relies on OS and
user-account file protections, that app deletion controls app-private meeting
data, and that user-owned source files and exports can remain outside the app's
delete boundary.

Manual release-candidate smoke should verify that release notes or support
material describe the app-private storage boundary, source-file boundary, export
boundary, and current encryption-at-rest status.
