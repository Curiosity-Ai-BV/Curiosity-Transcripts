use curiosity_audio::{
    AudioCapture, CapturePermission, CapturePermissionError, DeviceIdentity, FakeAudioCapture,
    ManualSmokeCheck, ManualSmokeStatus, StreamKind,
};
use std::process::Command;

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
fn smoke_placeholder_never_reports_passed_when_hardware_check_is_skipped() {
    let smoke = ManualSmokeCheck::macos_placeholder();
    let result = smoke.run_without_hardware();

    assert_eq!(result.status, ManualSmokeStatus::NotRun);
    assert!(result.message.contains("manual"));
    assert_ne!(result.status, ManualSmokeStatus::Passed);
}

#[test]
fn smoke_binary_exits_nonzero_when_hardware_check_is_not_run() {
    let output = Command::new(env!("CARGO_BIN_EXE_audio-smoke"))
        .output()
        .expect("run audio-smoke");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("NotRun"));
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
