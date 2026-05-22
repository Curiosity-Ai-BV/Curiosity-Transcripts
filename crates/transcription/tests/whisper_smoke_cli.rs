use std::process::Command;

#[test]
fn whisper_smoke_cli_returns_nonzero_skipped_guidance_without_env_paths() {
    let output = Command::new(whisper_smoke_bin())
        .env_remove("CURIOSITY_WHISPER_MODEL")
        .env_remove("CURIOSITY_WHISPER_WAV")
        .output()
        .expect("run whisper-smoke");

    assert!(
        !output.status.success(),
        "smoke skip must not count as success"
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Skipped"),
        "stdout should name skipped status"
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("CURIOSITY_WHISPER_MODEL"),
        "stdout should tell the user how to provide the model path"
    );
}

#[cfg(not(feature = "whisper-rs"))]
#[test]
fn whisper_smoke_cli_returns_nonzero_unavailable_guidance_when_feature_disabled() {
    let output = Command::new(whisper_smoke_bin())
        .env(
            "CURIOSITY_WHISPER_MODEL",
            "/tmp/curiosity-whisper-model.bin",
        )
        .env("CURIOSITY_WHISPER_WAV", "/tmp/curiosity-whisper.wav")
        .output()
        .expect("run whisper-smoke");

    assert!(
        !output.status.success(),
        "feature-disabled smoke must not count as success"
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Unavailable"),
        "stdout should name unavailable status"
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("--features whisper-rs"),
        "stdout should tell the user to enable the native feature"
    );
}

fn whisper_smoke_bin() -> String {
    std::env::var("CARGO_BIN_EXE_whisper-smoke").expect("whisper-smoke binary path")
}
