# Curiosity Audio

Automated tests keep audio capture hardware-free under `cargo test --workspace`.

The default smoke command does not touch hardware and exits non-zero with an explicit skipped status:

```sh
cargo run -p curiosity-audio --bin audio-smoke
```

To attempt real macOS microphone capture, grant Microphone permission to the terminal/app running Cargo, then run:

```sh
cargo run -p curiosity-audio --bin audio-smoke -- --attempt-mic --duration-ms 1000 --out /tmp/curiosity-audio-smoke
```

On success, the smoke writes `mic-smoke/raw-mic.wav` under the output root and reports sample rate, channel count, device name, duration, and sha256 metadata. `Skipped`, `Unavailable`, or `PermissionDenied` are never treated as pass statuses.

System audio is not claimed as functional in this slice. `screencapturekit` 6.0.1 is available as a Rust ScreenCaptureKit binding, but this crate does not yet own a safe app-scoped ScreenCaptureKit stream lifecycle for Screen Recording permission prompts. The exposed `ScreenCaptureKitSystemAudioAdapter` returns a typed `Unavailable` state with Screen Recording recovery guidance until that lifecycle is implemented.
