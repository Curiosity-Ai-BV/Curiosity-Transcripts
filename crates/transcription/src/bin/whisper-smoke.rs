use curiosity_transcription::{run_optional_real_whisper_smoke_from_env, WhisperSmokeStatus};

fn main() {
    let status = run_optional_real_whisper_smoke_from_env();

    match status {
        WhisperSmokeStatus::Passed { segment_count } => {
            println!("Passed: transcribed {segment_count} segment(s)");
        }
        WhisperSmokeStatus::Skipped { reason } => {
            println!("Skipped: {reason}");
            std::process::exit(2);
        }
        WhisperSmokeStatus::Unavailable { reason } => {
            println!("Unavailable: {reason}");
            std::process::exit(2);
        }
        WhisperSmokeStatus::Failed { message } => {
            println!("Failed: {message}");
            std::process::exit(1);
        }
    }
}
