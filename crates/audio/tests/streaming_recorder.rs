use std::fs;

use curiosity_audio::{
    AudioCapture, CaptureConfiguration, FakeAudioCapture, ManifestStatus, RecordingMetadata,
    StreamKind, StreamingWavRecorder,
};

fn test_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "curiosity-audio-streaming-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("test dir");
    path
}

#[test]
fn write_frame_updates_manifest_with_recoverable_in_progress_wav_artifact() {
    let root = test_dir("in-progress-wav");
    let capture = FakeAudioCapture::new_deterministic(48_000, 2, 1_700_000_000_000);
    let snapshot = capture.device_snapshot().expect("fake snapshot");
    let mut recorder = StreamingWavRecorder::start(
        &root,
        RecordingMetadata::new("session-in-progress", 1_700_000_000_000),
        CaptureConfiguration::mic_only().expect("mic-only config"),
        snapshot,
    )
    .expect("start recorder");
    let frame = capture
        .capture_frames()
        .expect("fake frames")
        .into_iter()
        .find(|frame| frame.stream == StreamKind::Microphone)
        .expect("mic frame");

    recorder.write_frame(&frame).expect("write frame");

    let manifest_text = fs::read_to_string(
        root.join("session-in-progress").join("manifest.txt"),
    )
    .expect("manifest text");
    assert!(manifest_text.contains("status=Recording"));
    assert!(manifest_text.contains("recoverable=true"));
    assert!(manifest_text.contains("recovery_reason=recording active; WAV artifact can be recovered if interrupted"));
    let artifact_line = manifest_text
        .lines()
        .find(|line| line.starts_with("artifact=Microphone,raw-mic.wav,Writing,"))
        .expect("in-progress artifact line");
    let bytes_written = artifact_line
        .rsplit_once(',')
        .expect("artifact byte count")
        .1
        .parse::<u64>()
        .expect("numeric byte count");
    assert!(bytes_written > 0);
    assert!(manifest_text.contains("artifact_started_at_ms=1700000000000"));
    assert!(manifest_text.contains("artifact_ended_at_ms=Some(1700000000001)"));
    assert!(manifest_text.contains("duration_ms=1"));
    assert!(manifest_text.contains("sample_rate_hz=48000"));
    assert!(manifest_text.contains("channel_count=2"));
    assert!(manifest_text.contains("device_identity=fake-mic"));
    assert!(manifest_text.contains("device_display_name=Fake Microphone"));
    assert!(manifest_text.contains("device_transport=test"));
    assert!(root
        .join("session-in-progress")
        .join("raw-mic.wav")
        .exists());
}

#[test]
fn stop_writes_recoverable_wav_artifact_metadata_and_complete_manifest() {
    let root = test_dir("stop-wav");
    let capture = FakeAudioCapture::new_deterministic(48_000, 2, 1_700_000_000_000);
    let snapshot = capture.device_snapshot().expect("fake snapshot");
    let mut recorder = StreamingWavRecorder::start(
        &root,
        RecordingMetadata::new("session-wav", 1_700_000_000_000),
        CaptureConfiguration::mic_only().expect("mic-only config"),
        snapshot,
    )
    .expect("start recorder");

    for frame in capture
        .capture_frames()
        .expect("fake frames")
        .iter()
        .filter(|frame| frame.stream == StreamKind::Microphone)
    {
        recorder.write_frame(frame).expect("write frame");
    }
    let manifest = recorder.stop(1_700_000_000_001).expect("stop");

    assert_eq!(manifest.status, ManifestStatus::Complete);
    assert_eq!(manifest.artifacts.len(), 1);
    let artifact = &manifest.artifacts[0];
    assert_eq!(artifact.stream, StreamKind::Microphone);
    assert_eq!(artifact.sample_rate_hz, 48_000);
    assert_eq!(artifact.channel_count, 2);
    assert_eq!(artifact.identity.identity, "fake-mic");
    assert_eq!(artifact.file_name, "raw-mic.wav");
    assert_eq!(artifact.sha256.len(), 64);
    assert!(artifact.duration_ms > 0);
    assert!(artifact.bytes_written > 0);

    let reader = hound::WavReader::open(&artifact.path).expect("read wav");
    assert_eq!(reader.spec().sample_rate, 48_000);
    assert_eq!(reader.spec().channels, 2);
    assert_eq!(reader.duration(), 2);

    let manifest_text =
        fs::read_to_string(root.join("session-wav").join("manifest.txt")).expect("manifest text");
    assert!(manifest_text.contains("status=Complete"));
    assert!(manifest_text.contains("artifact=Microphone,raw-mic.wav,Complete,"));
    assert!(manifest_text.contains("artifact_started_at_ms=1700000000000"));
    assert!(manifest_text.contains("artifact_ended_at_ms=Some(1700000000001)"));
    assert!(manifest_text.contains("duration_ms=1"));
    assert!(manifest_text.contains("sample_rate_hz=48000"));
    assert!(manifest_text.contains("channel_count=2"));
    assert!(manifest_text.contains("device_identity=fake-mic"));
    assert!(manifest_text.contains("device_display_name=Fake Microphone"));
    assert!(manifest_text.contains("device_transport=test"));
    assert!(manifest_text.contains("sha256="));
}
