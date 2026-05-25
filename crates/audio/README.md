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

To attempt real macOS ScreenCaptureKit system-audio capture, grant Screen Recording permission to the terminal/app running Cargo, then run:

```sh
cargo run -p curiosity-audio --features system-audio-screencapturekit --bin audio-smoke -- --attempt-system-audio --duration-ms 1000 --out /tmp/curiosity-audio-smoke
```

On success, the smoke writes `system-audio-smoke/raw-system.wav` under the output root and reports the same artifact metadata. Without macOS, Screen Recording permission, or the `system-audio-screencapturekit` feature, the command reports a non-passing unavailable or permission-denied status instead of claiming hardware success.
