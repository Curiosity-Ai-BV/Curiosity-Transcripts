use std::fs;
use std::path::{Path, PathBuf};

use curiosity_domain::SourceChannel;
use curiosity_transcription::{
    run_optional_real_whisper_smoke, FakeWhisperBackend, TranscriptionError, WhisperBackendSegment,
    WhisperSmokeStatus, WhisperTranscriber, WhisperTranscriptionRequest,
};

#[test]
fn whisper_rs_feature_maps_to_optional_native_dependency_without_enabling_default_builds() {
    let manifest = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
        .expect("transcription manifest");

    assert!(manifest.contains("whisper-rs = [\"dep:whisper-rs\"]"));
    assert!(manifest.contains("whisper-rs = { version = \"0.16\", optional = true }"));
}

#[test]
fn local_whisper_missing_model_path_returns_setup_guidance() {
    let dir = unique_test_dir("missing-model");
    fs::create_dir_all(&dir).expect("test dir");
    let audio_path = dir.join("audio.wav");
    write_minimal_wav(&audio_path);

    let request = WhisperTranscriptionRequest::new(
        "meeting-1",
        &audio_path,
        "sha256:audio",
        SourceChannel::Imported,
    );
    let transcriber = WhisperTranscriber::new(
        dir.join("missing-model.bin"),
        "ggml-base.en",
        FakeWhisperBackend::default(),
    );

    let error = transcriber
        .transcribe_wav(&request)
        .expect_err("missing model must be reported");

    match error {
        TranscriptionError::MissingModelPath { path, guidance } => {
            assert!(path.ends_with("missing-model.bin"));
            assert!(guidance.contains("CURIOSITY_WHISPER_MODEL"));
        }
        other => panic!("expected missing model path, got {other:?}"),
    }
}

#[test]
fn local_whisper_missing_audio_path_returns_file_recovery_guidance_not_format_guidance() {
    let dir = unique_test_dir("missing-audio");
    fs::create_dir_all(&dir).expect("test dir");
    let model_path = dir.join("model.bin");
    fs::write(&model_path, b"test model placeholder").expect("model placeholder");
    let audio_path = dir.join("missing.wav");

    let request = WhisperTranscriptionRequest::new(
        "meeting-1",
        &audio_path,
        "sha256:audio",
        SourceChannel::Imported,
    );
    let transcriber =
        WhisperTranscriber::new(model_path, "ggml-base.en", FakeWhisperBackend::default());

    let error = transcriber
        .transcribe_wav(&request)
        .expect_err("missing audio must be reported as an unavailable file");

    match error {
        TranscriptionError::AudioInputUnavailable { path, guidance } => {
            assert!(path.ends_with("missing.wav"));
            assert!(guidance.contains("does not exist") || guidance.contains("readable"));
            assert!(!guidance.contains("convert"));
        }
        other => panic!("expected unavailable audio input, got {other:?}"),
    }
}

#[test]
fn local_whisper_unsupported_audio_input_returns_user_facing_failure() {
    let dir = unique_test_dir("unsupported-audio");
    fs::create_dir_all(&dir).expect("test dir");
    let model_path = dir.join("model.bin");
    fs::write(&model_path, b"test model placeholder").expect("model placeholder");
    let audio_path = dir.join("audio.mp3");
    fs::write(&audio_path, b"not a wav").expect("unsupported audio");

    let request = WhisperTranscriptionRequest::new(
        "meeting-1",
        &audio_path,
        "sha256:audio",
        SourceChannel::Imported,
    );
    let transcriber =
        WhisperTranscriber::new(model_path, "ggml-base.en", FakeWhisperBackend::default());

    let error = transcriber
        .transcribe_wav(&request)
        .expect_err("unsupported audio must be reported");

    match error {
        TranscriptionError::UnsupportedAudioInput { path, guidance } => {
            assert!(path.ends_with("audio.mp3"));
            assert!(guidance.contains("WAV"));
        }
        other => panic!("expected unsupported audio input, got {other:?}"),
    }
}

#[test]
fn local_whisper_model_run_ids_do_not_collide_when_raw_identities_share_sanitized_form() {
    let dir = unique_test_dir("model-run-collision");
    fs::create_dir_all(&dir).expect("test dir");
    let model_path = dir.join("model.bin");
    fs::write(&model_path, b"test model placeholder").expect("model placeholder");
    let audio_path = dir.join("audio.wav");
    write_minimal_wav(&audio_path);
    let transcriber =
        WhisperTranscriber::new(model_path, "fixture-whisper", FakeWhisperBackend::default());

    let first = transcriber
        .transcribe_wav(&WhisperTranscriptionRequest::new(
            "meeting:a",
            &audio_path,
            "sha256:audio",
            SourceChannel::Imported,
        ))
        .expect("first transcription");
    let second = transcriber
        .transcribe_wav(&WhisperTranscriptionRequest::new(
            "meeting/a",
            &audio_path,
            "sha256:audio",
            SourceChannel::Imported,
        ))
        .expect("second transcription");

    assert_ne!(first.model_run_id, second.model_run_id);
    assert_ne!(first.transcript_version_id, second.transcript_version_id);
}

#[test]
fn fake_whisper_backend_maps_timestamped_output_into_ordered_metadata_segments() {
    let dir = unique_test_dir("fake-backend");
    fs::create_dir_all(&dir).expect("test dir");
    let model_path = dir.join("model.bin");
    fs::write(&model_path, b"test model placeholder").expect("model placeholder");
    let audio_path = dir.join("audio.wav");
    write_minimal_wav(&audio_path);

    let backend = FakeWhisperBackend::new(vec![
        WhisperBackendSegment::new(1_200, 1_800, "second"),
        WhisperBackendSegment::new(0, 900, "first"),
    ]);
    let transcriber = WhisperTranscriber::new(model_path, "fixture-whisper", backend);
    let request = WhisperTranscriptionRequest::new(
        "meeting-1",
        &audio_path,
        "sha256:audio-fixture",
        SourceChannel::Imported,
    );

    let document = transcriber.transcribe_wav(&request).expect("fake backend");

    assert_eq!(document.provider, "local-whisper");
    assert_eq!(document.model_name, "fixture-whisper");
    assert_eq!(document.source_artifact_sha256, "sha256:audio-fixture");
    assert_eq!(document.segments.len(), 2);
    assert_eq!(document.segments[0].text, "first");
    assert_eq!(document.segments[0].start_ms, 0);
    assert_eq!(document.segments[0].source_channel, SourceChannel::Imported);
    assert_eq!(document.segments[1].text, "second");
    assert!(!document.model_run_id.is_empty());
    assert!(!document.transcript_version_id.is_empty());
    assert_eq!(document.segments[0].model_run_id, document.model_run_id);
    assert_eq!(
        document.segments[0].transcript_version_id,
        document.transcript_version_id
    );
}

#[test]
fn optional_real_whisper_smoke_is_skipped_without_explicit_paths_and_not_counted_as_passed() {
    let status = run_optional_real_whisper_smoke(None::<PathBuf>, None::<PathBuf>);

    match status {
        WhisperSmokeStatus::Skipped { ref reason } => {
            assert!(reason.contains("CURIOSITY_WHISPER_MODEL"));
        }
        other => panic!("expected skipped smoke, got {other:?}"),
    }
    assert!(!status.was_run());
    assert!(!status.passed());
}

fn unique_test_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "curiosity-transcription-{name}-{}",
        std::process::id()
    ))
}

fn write_minimal_wav(path: &Path) {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&36u32.to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&16_000u32.to_le_bytes());
    bytes.extend_from_slice(&32_000u32.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&0u32.to_le_bytes());
    fs::write(path, bytes).expect("minimal wav");
}
