# Curiosity Audio

Phase 0 keeps audio capture pure and hardware-free under `cargo test --workspace`.

Manual macOS hardware capture is not wired yet. The explicit smoke placeholder can be run with:

```sh
cargo run -p curiosity-audio --bin audio-smoke
```

Until a real macOS adapter exists, the command reports `NotRun`; skipped or unwired hardware checks must not be counted as passed.
