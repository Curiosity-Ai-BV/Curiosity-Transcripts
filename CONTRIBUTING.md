# Contributing

Curiosity Transcripts is a local-first Rust and Tauri desktop app. Keep changes
small, testable, and aligned with the existing crate and desktop command
boundaries.

## Development Setup

Run deterministic checks from the repository root:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Run desktop Rust backend checks from the repository root:

```sh
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --check
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings
```

Run desktop checks from `apps/desktop`:

```sh
npm ci
npm run test
npm run build
```

Run the publication metadata check from the repository root:

```sh
bash scripts/check-publication-readiness.sh
```

Hardware, Whisper, Ollama, and ScreenCaptureKit checks are smoke tests. They
must fail loud when prerequisites are missing instead of being counted as
deterministic CI success.

## Test-First Changes

Use a test-driven flow for behavior changes:

1. Add the smallest failing test that describes the intended behavior.
2. Run the focused test and confirm it fails for the expected reason.
3. Implement the smallest change that makes it pass.
4. Run the focused test again, then run the relevant broader checks.

Tests should use real code where practical. Use fakes for hardware, hosted
providers, model processes, and OS integrations so regular tests stay
deterministic.

## Rust Quality

Follow idiomatic Rust conventions:

- Prefer explicit `Result` errors over panics in production code.
- Keep public crate APIs documented when they are intended for reuse.
- Fix Clippy warnings instead of silencing them.
- Keep ownership simple; borrow where ownership is not needed.

## Pull Requests

Before opening a pull request, include:

- What changed and why.
- The red/green test evidence for behavior changes.
- Exact commands run.
- Any skipped checks or local prerequisites that were unavailable.
