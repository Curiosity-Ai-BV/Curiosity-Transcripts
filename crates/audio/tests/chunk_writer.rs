use std::fs;

use curiosity_audio::{
    AudioFrame, ChunkRecoveryState, ChunkStatus, ChunkWriter, ManifestStatus, RecordingMetadata,
    StreamKind,
};

fn test_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "curiosity-audio-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("test dir");
    path
}

fn mic_frame() -> AudioFrame {
    AudioFrame {
        stream: StreamKind::Microphone,
        start_time_ms: 1_000,
        sample_rate_hz: 48_000,
        channel_count: 1,
        pcm_i16: vec![1, -1, 2, -2],
    }
}

fn timed_frame(start_time_ms: u64, sample_count: usize, channel_count: u16) -> AudioFrame {
    AudioFrame {
        stream: StreamKind::Microphone,
        start_time_ms,
        sample_rate_hz: 48_000,
        channel_count,
        pcm_i16: vec![1; sample_count],
    }
}

#[test]
fn stop_marks_written_chunks_complete_and_manifest_not_recoverable() {
    let root = test_dir("stop");
    let mut writer = ChunkWriter::create(&root, RecordingMetadata::new("session-stop", 1_000))
        .expect("writer");

    writer.write_frame(&mic_frame()).expect("write frame");
    let manifest = writer.stop(1_010).expect("stop");

    assert_eq!(manifest.status, ManifestStatus::Complete);
    assert_eq!(manifest.ended_at_ms, Some(1_010));
    assert_eq!(manifest.chunks.len(), 1);
    assert_eq!(manifest.chunks[0].status, ChunkStatus::Complete);
    assert_eq!(manifest.chunks[0].recovery, ChunkRecoveryState::NotNeeded);
    assert!(manifest.chunks[0].bytes_written > 0);
    assert!(root.join("manifest.txt").read_link().is_err());
    assert!(root.join("session-stop").join("manifest.txt").exists());
}

#[test]
fn manifest_file_uses_explicit_stable_status_strings() {
    let root = test_dir("manifest-stable-strings");
    let mut writer = ChunkWriter::create(&root, RecordingMetadata::new("session-strings", 1_000))
        .expect("writer");

    writer.write_frame(&mic_frame()).expect("write frame");
    writer
        .cancel(1_010, "user canceled recording")
        .expect("cancel");

    let manifest_text =
        fs::read_to_string(root.join("session-strings").join("manifest.txt")).expect("manifest");
    assert!(manifest_text.contains("status=Canceled"));
    assert!(manifest_text.contains("chunk=Microphone,Interrupted,RecoverableInterrupted,"));
    assert!(!manifest_text.contains("status=ManifestStatus"));
}

#[test]
fn chunk_end_time_tracks_written_audio_duration_across_frames() {
    let root = test_dir("chunk-end-time");
    let mut writer = ChunkWriter::create(&root, RecordingMetadata::new("session-end-time", 1_000))
        .expect("writer");

    writer
        .write_frame(&timed_frame(1_000, 480, 1))
        .expect("first frame");
    writer
        .write_frame(&timed_frame(1_010, 480, 1))
        .expect("second frame");
    let manifest = writer.stop(1_020).expect("stop");

    assert_eq!(manifest.chunks[0].started_at_ms, 1_000);
    assert_eq!(manifest.chunks[0].ended_at_ms, Some(1_020));
}

#[test]
fn cancel_preserves_written_bytes_with_clear_recoverable_interrupted_metadata() {
    let root = test_dir("cancel");
    let mut writer = ChunkWriter::create(&root, RecordingMetadata::new("session-cancel", 2_000))
        .expect("writer");

    writer.write_frame(&mic_frame()).expect("write frame");
    let manifest = writer.cancel(2_005, "user canceled recording").expect("cancel");

    assert_eq!(manifest.status, ManifestStatus::Canceled);
    assert_eq!(manifest.recovery.as_ref().map(|r| r.recoverable), Some(true));
    assert_eq!(
        manifest.chunks[0].recovery,
        ChunkRecoveryState::RecoverableInterrupted
    );
    assert_eq!(manifest.chunks[0].status, ChunkStatus::Interrupted);
    assert!(manifest.chunks[0].bytes_written > 0);
    assert!(manifest
        .recovery
        .as_ref()
        .expect("recovery")
        .reason
        .contains("user canceled"));
}

#[test]
fn failure_without_written_bytes_is_failed_but_not_recoverable() {
    let root = test_dir("failure-empty");
    let writer = ChunkWriter::create(&root, RecordingMetadata::new("session-fail", 3_000))
        .expect("writer");

    let manifest = writer.fail(3_001, "disk permission denied").expect("fail");

    assert_eq!(manifest.status, ManifestStatus::Failed);
    assert_eq!(manifest.recovery.as_ref().map(|r| r.recoverable), Some(false));
    assert!(manifest.chunks.is_empty());
}

#[test]
fn failure_after_written_bytes_marks_chunks_recoverable_interrupted() {
    let root = test_dir("failure-written");
    let mut writer = ChunkWriter::create(&root, RecordingMetadata::new("session-fail-written", 4_000))
        .expect("writer");

    writer.write_frame(&mic_frame()).expect("write frame");
    let manifest = writer.fail(4_002, "disk full").expect("fail");

    assert_eq!(manifest.status, ManifestStatus::Failed);
    assert_eq!(manifest.recovery.as_ref().map(|r| r.recoverable), Some(true));
    assert_eq!(
        manifest.chunks[0].recovery,
        ChunkRecoveryState::RecoverableInterrupted
    );
    assert!(manifest.chunks[0].bytes_written > 0);
}
