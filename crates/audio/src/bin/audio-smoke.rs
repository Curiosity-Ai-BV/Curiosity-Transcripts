use curiosity_audio::{ManualSmokeCheck, ManualSmokeStatus};
use std::path::PathBuf;
use std::time::Duration;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let attempt_mic = args.iter().any(|arg| arg == "--attempt-mic");
    let attempt_system_audio = args.iter().any(|arg| arg == "--attempt-system-audio");
    let output_root = option_value(&args, "--out")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("audio-smoke-output"));
    let duration_ms = option_value(&args, "--duration-ms")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(1_000);

    let smoke = ManualSmokeCheck::macos_placeholder();
    let result = if attempt_system_audio {
        smoke.run_macos_system_audio_capture(&output_root, Duration::from_millis(duration_ms))
    } else if attempt_mic {
        smoke.run_macos_microphone_capture(&output_root, Duration::from_millis(duration_ms))
    } else {
        smoke.run_without_hardware()
    };
    println!("{:?}: {}", result.status, result.message);

    if result.status != ManualSmokeStatus::Passed {
        std::process::exit(2);
    }
}

fn option_value(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|window| window[0] == name)
        .map(|window| window[1].clone())
}
