use curiosity_audio::{
    AudioCapture, CaptureConfiguration, CaptureError, CapturePermission, CapturePermissionError,
    CaptureUnavailable, DeviceIdentity, FakeAudioCapture, ManualSmokeCheck, ManualSmokeResult,
    ManualSmokeStatus, StreamKind,
};
use std::process::Command;
#[cfg(not(feature = "system-audio-screencapturekit"))]
use std::time::Duration;

#[test]
fn capture_configuration_accepts_mic_only_requests() {
    let config = CaptureConfiguration::mic_only().expect("mic-only config");

    assert_eq!(config.requested_streams(), vec![StreamKind::Microphone]);
}

#[test]
fn capture_configuration_accepts_system_only_requests() {
    let config = CaptureConfiguration::system_only().expect("system-only config");

    assert_eq!(config.requested_streams(), vec![StreamKind::SystemAudio]);
}

#[test]
fn capture_configuration_accepts_mixed_requests() {
    let config = CaptureConfiguration::mixed().expect("mixed config");

    assert_eq!(
        config.requested_streams(),
        vec![StreamKind::Microphone, StreamKind::SystemAudio]
    );
}

#[test]
fn capture_configuration_rejects_empty_requests() {
    let error = CaptureConfiguration::new(false, false).expect_err("empty request should fail");

    assert!(error.to_string().contains("at least one"));
}

#[test]
fn fake_capture_emits_deterministic_pcm_frames_and_device_snapshot() {
    let capture = FakeAudioCapture::new_deterministic(48_000, 2, 1_700_000_000_000);

    let snapshot = capture.device_snapshot().expect("fake snapshot");
    assert_eq!(snapshot.captured_at_ms, 1_700_000_000_000);
    assert_eq!(
        snapshot
            .mic
            .as_ref()
            .map(|device| device.identity.identity.as_str()),
        Some("fake-mic")
    );
    assert_eq!(
        snapshot
            .system
            .as_ref()
            .map(|device| device.identity.identity.as_str()),
        Some("fake-system")
    );

    let frames = capture.capture_frames().expect("fake frames");
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].stream, StreamKind::Microphone);
    assert_eq!(frames[0].sample_rate_hz, 48_000);
    assert_eq!(frames[0].channel_count, 2);
    assert_eq!(frames[0].pcm_i16, vec![0, 1000, 0, -1000]);
    assert_eq!(frames[1].stream, StreamKind::SystemAudio);
    assert_eq!(frames[1].pcm_i16, vec![500, 0, -500, 0]);
}

#[test]
fn permission_errors_map_to_actionable_user_recovery_guidance() {
    let mic = CapturePermissionError::denied(CapturePermission::Microphone);
    let mic_guidance = mic.recovery_guidance();
    assert!(mic_guidance.title.contains("Microphone"));
    assert!(mic_guidance
        .steps
        .iter()
        .any(|step| step.contains("System Settings")));
    assert!(mic_guidance
        .steps
        .iter()
        .any(|step| step.contains("Privacy & Security")));

    let system = CapturePermissionError::denied(CapturePermission::SystemAudioScreenRecording);
    let system_guidance = system.recovery_guidance();
    assert!(system_guidance.title.contains("Screen Recording"));
    assert!(system_guidance
        .steps
        .iter()
        .any(|step| step.contains("Screen Recording")));
    assert!(system_guidance
        .steps
        .iter()
        .any(|step| step.contains("restart")));
}

#[test]
fn unavailable_errors_map_to_actionable_user_recovery_guidance() {
    let mic = CaptureUnavailable::microphone("no default input device");
    let mic_guidance = mic.recovery_guidance();
    assert!(mic_guidance.title.contains("Microphone"));
    assert!(mic_guidance
        .steps
        .iter()
        .any(|step| step.contains("input device")));

    let system = CaptureUnavailable::system_audio(
        "ScreenCaptureKit audio capture requires a permissioned macOS adapter",
    );
    let system_guidance = system.recovery_guidance();
    assert!(system_guidance.title.contains("System audio"));
    assert!(system_guidance
        .steps
        .iter()
        .any(|step| step.contains("Screen Recording")));
}

#[test]
fn smoke_result_from_capture_error_never_reports_passed() {
    let result = ManualSmokeResult::from_capture_error(CaptureError::PermissionDenied(
        CapturePermissionError::denied(CapturePermission::Microphone),
    ));

    assert_eq!(result.status, ManualSmokeStatus::PermissionDenied);
    assert!(result.message.contains("Microphone"));
    assert_ne!(result.status, ManualSmokeStatus::Passed);
}

#[test]
fn smoke_placeholder_never_reports_passed_when_hardware_check_is_skipped() {
    let smoke = ManualSmokeCheck::macos_placeholder();
    let result = smoke.run_without_hardware();

    assert_eq!(result.status, ManualSmokeStatus::Skipped);
    assert!(result.message.contains("--attempt-mic"));
    assert_ne!(result.status, ManualSmokeStatus::Passed);
}

#[test]
fn smoke_binary_exits_nonzero_when_hardware_check_is_skipped() {
    let output = Command::new(env!("CARGO_BIN_EXE_audio-smoke"))
        .output()
        .expect("run audio-smoke");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Skipped"));
}

#[test]
#[cfg(not(feature = "system-audio-screencapturekit"))]
fn system_audio_smoke_path_does_not_report_fake_success_without_real_capture() {
    let root = std::env::temp_dir().join(format!(
        "curiosity-system-audio-smoke-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);

    let result = ManualSmokeCheck::macos_placeholder()
        .run_macos_system_audio_capture(&root, Duration::from_millis(1));

    assert_ne!(result.status, ManualSmokeStatus::Passed);
    assert!(
        result.message.contains("ScreenCaptureKit") || result.message.contains("Screen Recording")
    );
}

#[test]
#[cfg(not(feature = "system-audio-screencapturekit"))]
fn smoke_binary_exposes_system_audio_attempt_without_fake_success() {
    let output = Command::new(env!("CARGO_BIN_EXE_audio-smoke"))
        .args(["--attempt-system-audio", "--duration-ms", "1"])
        .output()
        .expect("run audio-smoke");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("System"));
}

#[test]
fn contracts_preserve_separate_mic_and_system_stream_metadata() {
    let capture = FakeAudioCapture::with_devices(
        DeviceIdentity::new("mic-1", "Studio Mic", "USB"),
        DeviceIdentity::new("system-1", "MacBook Output", "ScreenCaptureKit"),
        44_100,
        1,
        42,
    );

    let snapshot = capture.device_snapshot().expect("fake snapshot");
    let mic = snapshot.mic.expect("mic metadata");
    let system = snapshot.system.expect("system metadata");

    assert_eq!(mic.stream, StreamKind::Microphone);
    assert_eq!(mic.sample_rate_hz, 44_100);
    assert_eq!(mic.channel_count, 1);
    assert_eq!(mic.identity.display_name, "Studio Mic");
    assert_eq!(system.stream, StreamKind::SystemAudio);
    assert_eq!(system.identity.display_name, "MacBook Output");
}
