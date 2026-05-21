use std::fs;
use std::path::{Path, PathBuf};

use curiosity_app::{
    dedupe_selected_segments, AppPermissionState, ArtifactSink, CommandRecordingState,
    FakeArtifactSink, ManualRecordingService, RawAudioRetentionPolicy, RecordingErrorKind,
    SpeechSegment, SpeechSource, StorageSetup, StorageSetupError,
};
use curiosity_audio::{
    AudioCapture, AudioFrame, CapturePermission, CapturePermissionError, DeviceSnapshot,
    FakeAudioCapture,
};
use curiosity_store::Store;

fn test_root(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "curiosity-app-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("test root");
    path
}

fn service(root: &Path) -> ManualRecordingService<FakeAudioCapture, FakeArtifactSink> {
    let store = Store::open(root.join("app.db"), root.to_path_buf()).expect("open store");
    store.migrate().expect("migrate");
    ManualRecordingService::new(
        store,
        FakeAudioCapture::new_deterministic(48_000, 2, 1_000),
        FakeArtifactSink::new(root.join("meetings")),
    )
}

#[test]
fn start_rejects_meeting_ids_that_escape_private_meeting_storage() {
    let root = test_root("unsafe-meeting-id");
    let unsafe_absolute = root.join("absolute-escape").to_string_lossy().to_string();
    let cases = vec![
        ("../escape", root.join("escape")),
        ("nested/id", root.join("meetings/nested")),
        (unsafe_absolute.as_str(), root.join("absolute-escape")),
    ];

    for (meeting_id, escaped_path) in cases {
        let mut service = service(&root);

        let err = service
            .start_manual_recording(meeting_id, "Unsafe", 1_000)
            .expect_err("unsafe meeting id should be rejected before storage setup");

        assert_eq!(err.kind, RecordingErrorKind::StorageUnavailable);
        assert!(service.active_recording().is_none());
        assert_eq!(service.store().count("meetings").expect("meetings"), 0);
        assert!(
            !escaped_path.exists(),
            "unsafe id {meeting_id} should not create {}",
            escaped_path.display()
        );
    }
}

#[test]
fn fake_artifact_sink_requires_meetings_prefixed_artifact_paths() {
    let root = test_root("artifact-path-prefix");
    let sink = FakeArtifactSink::new(root.join("meetings"));
    let setup = StorageSetup {
        relative_audio_dir: "meeting-1/audio".to_string(),
        artifact_path: "meeting-1/audio/mixed.pcm".to_string(),
    };
    let frame = AudioFrame {
        stream: curiosity_audio::StreamKind::Microphone,
        start_time_ms: 1_000,
        sample_rate_hz: 48_000,
        channel_count: 1,
        pcm_i16: vec![1, 2, 3],
    };

    let err = sink
        .write_frames(&setup, &[frame])
        .expect_err("artifact path without meetings/ prefix should be rejected");

    assert_eq!(err.kind, RecordingErrorKind::StorageUnavailable);
    assert!(!root.join("meetings/meeting-1/audio/mixed.pcm").exists());
}

#[test]
fn start_creates_recording_and_artifacts_before_reporting_active() {
    let root = test_root("start");
    let mut service = service(&root);

    let dto = service
        .start_manual_recording("meeting-1", "Planning", 1_000)
        .expect("start recording");

    assert_eq!(dto.state, CommandRecordingState::Recording);
    assert_eq!(dto.recording_id.as_deref(), Some("recording-meeting-1"));
    assert_eq!(dto.permission_state, AppPermissionState::Ready);
    assert_eq!(dto.raw_audio_retention, RawAudioRetentionPolicy::Retain);
    assert_eq!(dto.storage_location.app_private_path, "meetings/meeting-1/audio");
    assert!(service.active_recording().is_some());
    assert_eq!(service.store().count("meetings").expect("meetings"), 1);
    assert_eq!(
        service.store().count("recording_sessions").expect("sessions"),
        1
    );
    assert_eq!(
        service.store().count("audio_artifacts").expect("artifacts"),
        1
    );
}

#[test]
fn artifact_setup_failure_does_not_leave_half_created_active_recording() {
    let root = test_root("setup-failure");
    let store = Store::open(root.join("app.db"), root.to_path_buf()).expect("open store");
    store.migrate().expect("migrate");
    let capture = FakeAudioCapture::new_deterministic(48_000, 2, 1_000);
    let sink = FakeArtifactSink::failing(
        root.join("meetings"),
        StorageSetupError::disk_full("not enough space for raw audio"),
    );
    let mut service = ManualRecordingService::new(store, capture, sink);

    let err = service
        .start_manual_recording("meeting-1", "Planning", 1_000)
        .expect_err("setup should fail");

    assert_eq!(err.kind, RecordingErrorKind::DiskFull);
    assert_eq!(err.trust_state.state, CommandRecordingState::Interrupted);
    assert_eq!(err.trust_state.permission_state, AppPermissionState::Ready);
    assert!(err.trust_state.recovery_action.contains("Free disk space"));
    assert!(service.active_recording().is_none());
    assert_eq!(service.store().count("meetings").expect("meetings"), 0);
    assert_eq!(
        service.store().count("recording_sessions").expect("sessions"),
        0
    );
}

#[test]
fn permission_denied_failure_has_actionable_trust_state_without_requiring_hardware() {
    let root = test_root("permission");
    let store = Store::open(root.join("app.db"), root.to_path_buf()).expect("open store");
    store.migrate().expect("migrate");
    let capture = DeniedCapture::new(CapturePermission::Microphone);
    let sink = FakeArtifactSink::new(root.join("meetings"));
    let mut service = ManualRecordingService::new(store, capture, sink);

    let err = service
        .start_manual_recording("meeting-1", "Planning", 1_000)
        .expect_err("permission should fail");

    assert_eq!(err.kind, RecordingErrorKind::PermissionDenied);
    assert_eq!(err.trust_state.state, CommandRecordingState::Interrupted);
    assert_eq!(
        err.trust_state.permission_state,
        AppPermissionState::MicrophoneDenied
    );
    assert!(err.trust_state.recovery_action.contains("Microphone"));
    assert!(service.active_recording().is_none());
}

#[test]
fn recoverable_fake_interruption_persists_manifest_for_startup_repair() {
    let root = test_root("recoverable-manifest");
    let store = Store::open(root.join("app.db"), root.to_path_buf()).expect("open store");
    store.migrate().expect("migrate");
    let capture = FakeAudioCapture::new_deterministic(48_000, 2, 1_000);
    let sink = FakeArtifactSink::fail_after_writing_bytes(
        root.join("meetings"),
        StorageSetupError::disk_full("disk filled while writing chunks"),
    );
    let mut service = ManualRecordingService::new(store, capture, sink);

    service
        .start_manual_recording("meeting-1", "Planning", 1_000)
        .expect("start");
    service
        .write_fake_audio_chunk()
        .expect_err("chunk write should fail after writing recoverable evidence");
    drop(service);

    let fresh_store = Store::open(root.join("app.db"), root.to_path_buf()).expect("reopen store");
    fresh_store.migrate().expect("migrate reopened store");
    let report = fresh_store.repair_startup().expect("startup repair");

    assert_eq!(report.recovered_artifacts, vec!["artifact-meeting-1"]);
    assert_eq!(
        fresh_store
            .artifact_recovery_status("artifact-meeting-1")
            .expect("artifact recovery status"),
        curiosity_store::RepairStatus::Recovered
    );
}

#[test]
fn command_contract_covers_manual_recording_pause_stop_and_recover_states() {
    let root = test_root("workflow");
    let store = Store::open(root.join("app.db"), root.to_path_buf()).expect("open store");
    store.migrate().expect("migrate");
    let capture = FakeAudioCapture::new_deterministic(48_000, 2, 1_000);
    let sink = FakeArtifactSink::fail_after_writing_bytes(
        root.join("meetings"),
        StorageSetupError::disk_full("disk filled while writing chunks"),
    );
    let mut service = ManualRecordingService::new(store, capture, sink);

    let recording = service
        .start_manual_recording("meeting-1", "Planning", 1_000)
        .expect("start");
    let paused = service.pause_active_recording().expect("pause");
    let interrupted = service
        .write_fake_audio_chunk()
        .expect_err("chunk write should fail")
        .trust_state;
    let recovering = service
        .recover_interrupted_recording("meeting-1", "recording-meeting-1", 2_000)
        .expect("recover same interrupted recording");
    let stopping = service.stop_active_recording(2_500).expect("stop");

    assert_eq!(recording.state, CommandRecordingState::Recording);
    assert_eq!(paused.state, CommandRecordingState::Paused);
    assert_eq!(interrupted.state, CommandRecordingState::Interrupted);
    assert_eq!(recovering.state, CommandRecordingState::Recovering);
    assert_eq!(stopping.state, CommandRecordingState::Stopping);
    assert_eq!(service.store().meeting_status("meeting-1").expect("meeting status"), "Complete");
    assert_eq!(
        service
            .store()
            .recording_session_status("recording-meeting-1")
            .expect("recording status"),
        "Complete"
    );
    assert_eq!(
        service
            .store()
            .recording_session_ended_at_ms("recording-meeting-1")
            .expect("recording ended"),
        Some(2_500)
    );
    assert_eq!(recovering.permission_state, AppPermissionState::Ready);
    assert_eq!(
        recovering.raw_audio_retention,
        RawAudioRetentionPolicy::DeleteAfterTranscription
    );
}

#[test]
fn duplicate_speech_fixture_does_not_double_count_obvious_mic_system_overlap() {
    let segments = vec![
        SpeechSegment::new(SpeechSource::Microphone, 1_000, 2_000, "we should ship this"),
        SpeechSegment::new(SpeechSource::System, 1_030, 2_030, "We should ship this"),
        SpeechSegment::new(SpeechSource::Microphone, 2_500, 3_000, "next topic"),
    ];

    let mixed = dedupe_selected_segments(SpeechSource::Mixed, &segments);
    let mic_only = dedupe_selected_segments(SpeechSource::Microphone, &segments);

    assert_eq!(mixed.len(), 2);
    assert_eq!(mixed[0].text, "we should ship this");
    assert_eq!(mixed[1].text, "next topic");
    assert_eq!(mic_only.len(), 2);
}

struct DeniedCapture {
    permission: CapturePermission,
}

impl DeniedCapture {
    fn new(permission: CapturePermission) -> Self {
        Self { permission }
    }
}

impl AudioCapture for DeniedCapture {
    fn device_snapshot(&self) -> Result<DeviceSnapshot, CapturePermissionError> {
        Err(CapturePermissionError::denied(self.permission))
    }

    fn capture_frames(&self) -> Result<Vec<AudioFrame>, CapturePermissionError> {
        unreachable!("service should fail before capturing frames when permissions are denied")
    }
}

#[test]
fn disk_full_before_written_evidence_leaves_clean_failed_nonrecoverable_state() {
    let root = test_root("write-failure");
    let store = Store::open(root.join("app.db"), root.to_path_buf()).expect("open store");
    store.migrate().expect("migrate");
    let capture = FakeAudioCapture::new_deterministic(48_000, 2, 1_000);
    let sink = FakeArtifactSink::fail_after_setup(
        root.join("meetings"),
        StorageSetupError::disk_full("disk filled while writing chunks"),
    );
    let mut service = ManualRecordingService::new(store, capture, sink);

    service
        .start_manual_recording("meeting-1", "Planning", 1_000)
        .expect("start before write failure");
    let interrupted = service
        .write_fake_audio_chunk()
        .expect_err("chunk write should fail")
        .trust_state;

    assert_eq!(interrupted.state, CommandRecordingState::Interrupted);
    assert!(!interrupted.recoverable);
    assert_eq!(interrupted.recording_id, None);
    assert!(interrupted.recovery_action.contains("Free disk space"));
    assert!(service.active_recording().is_none());
    assert_eq!(service.store().meeting_status("meeting-1").expect("meeting status"), "Failed");
    assert_eq!(
        service
            .store()
            .recording_session_status("recording-meeting-1")
            .expect("recording status"),
        "Failed"
    );
}

#[test]
fn recovery_requires_the_same_interrupted_meeting_and_recording() {
    let root = test_root("recover-same-recording");
    let store = Store::open(root.join("app.db"), root.to_path_buf()).expect("open store");
    store.migrate().expect("migrate");
    let capture = FakeAudioCapture::new_deterministic(48_000, 2, 1_000);
    let sink = FakeArtifactSink::fail_after_writing_bytes(
        root.join("meetings"),
        StorageSetupError::disk_full("disk filled while writing chunks"),
    );
    let mut service = ManualRecordingService::new(store, capture, sink);

    service
        .start_manual_recording("meeting-1", "Planning", 1_000)
        .expect("start");
    service
        .write_fake_audio_chunk()
        .expect_err("chunk write should fail");

    let unrelated = service
        .recover_interrupted_recording("meeting-2", "recording-meeting-2", 2_000)
        .expect_err("unrelated meeting should not recover");
    let wrong_session = service
        .recover_interrupted_recording("meeting-1", "recording-other", 2_000)
        .expect_err("wrong recording should not recover");
    let recovered = service
        .recover_interrupted_recording("meeting-1", "recording-meeting-1", 2_000)
        .expect("same recording should recover");

    assert_eq!(unrelated.kind, RecordingErrorKind::NoRecoverableRecording);
    assert_eq!(wrong_session.kind, RecordingErrorKind::NoRecoverableRecording);
    assert_eq!(recovered.state, CommandRecordingState::Recovering);
    assert_eq!(recovered.recording_id.as_deref(), Some("recording-meeting-1"));
    assert_eq!(service.active_recording(), Some("recording-meeting-1"));
    assert_eq!(service.store().meeting_status("meeting-1").expect("meeting status"), "Recovered");
    assert_eq!(
        service
            .store()
            .recording_session_status("recording-meeting-1")
            .expect("recording status"),
        "Recovered"
    );
}

#[test]
fn recovery_refuses_to_replace_an_active_recording() {
    let root = test_root("recover-while-active");
    let store = Store::open(root.join("app.db"), root.to_path_buf()).expect("open store");
    store.migrate().expect("migrate");
    let capture = FakeAudioCapture::new_deterministic(48_000, 2, 1_000);
    let sink = FakeArtifactSink::fail_after_writing_bytes(
        root.join("meetings"),
        StorageSetupError::disk_full("disk filled while writing chunks"),
    );
    let mut service = ManualRecordingService::new(store, capture, sink);

    service
        .start_manual_recording("meeting-1", "Planning", 1_000)
        .expect("start first");
    service
        .write_fake_audio_chunk()
        .expect_err("first recording should become recoverable");
    service
        .start_manual_recording("meeting-2", "Another", 2_000)
        .expect("start another active recording");

    let err = service
        .recover_interrupted_recording("meeting-1", "recording-meeting-1", 3_000)
        .expect_err("active recording should block recovery");

    assert_eq!(err.kind, RecordingErrorKind::AlreadyRecording);
    assert_eq!(service.active_recording(), Some("recording-meeting-2"));
}

#[test]
fn recoverable_failure_dto_includes_recording_handle_needed_to_recover() {
    let root = test_root("recoverable-handle");
    let store = Store::open(root.join("app.db"), root.to_path_buf()).expect("open store");
    store.migrate().expect("migrate");
    let capture = FakeAudioCapture::new_deterministic(48_000, 2, 1_000);
    let sink = FakeArtifactSink::fail_after_writing_bytes(
        root.join("meetings"),
        StorageSetupError::disk_full("disk filled while writing chunks"),
    );
    let mut service = ManualRecordingService::new(store, capture, sink);

    service
        .start_manual_recording("meeting-1", "Planning", 1_000)
        .expect("start");
    let interrupted = service
        .write_fake_audio_chunk()
        .expect_err("chunk write should fail")
        .trust_state;

    assert!(interrupted.recoverable);
    assert_eq!(interrupted.meeting_id, "meeting-1");
    assert_eq!(interrupted.recording_id.as_deref(), Some("recording-meeting-1"));
    assert_eq!(service.store().meeting_status("meeting-1").expect("meeting status"), "Interrupted");
    assert_eq!(
        service
            .store()
            .recording_session_status("recording-meeting-1")
            .expect("recording status"),
        "Interrupted"
    );
}

#[test]
fn capture_failure_during_active_recording_clears_active_and_allows_restart() {
    let root = test_root("capture-failure-active");
    let store = Store::open(root.join("app.db"), root.to_path_buf()).expect("open store");
    store.migrate().expect("migrate");
    let capture = CaptureFailsAfterStart;
    let sink = FakeArtifactSink::new(root.join("meetings"));
    let mut service = ManualRecordingService::new(store, capture, sink);

    service
        .start_manual_recording("meeting-1", "Planning", 1_000)
        .expect("start");
    let failed = service
        .write_fake_audio_chunk()
        .expect_err("capture should fail")
        .trust_state;

    assert_eq!(failed.state, CommandRecordingState::Interrupted);
    assert!(!failed.recoverable);
    assert_eq!(failed.recording_id, None);
    assert!(service.active_recording().is_none());
    assert_eq!(service.store().meeting_status("meeting-1").expect("meeting status"), "Failed");
    assert!(service
        .start_manual_recording("meeting-2", "Restart", 2_000)
        .is_ok());
}

struct CaptureFailsAfterStart;

impl AudioCapture for CaptureFailsAfterStart {
    fn device_snapshot(&self) -> Result<DeviceSnapshot, CapturePermissionError> {
        Ok(DeviceSnapshot {
            captured_at_ms: 1_000,
            mic: None,
            system: None,
        })
    }

    fn capture_frames(&self) -> Result<Vec<AudioFrame>, CapturePermissionError> {
        Err(CapturePermissionError::denied(CapturePermission::Microphone))
    }
}
