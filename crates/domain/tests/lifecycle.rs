use curiosity_domain::{
    ArtifactKind, AudioArtifact, JobKind, JobStatus, Meeting, MeetingStatus, RecordingSession,
    RecordingSource, RecordingStatus, TranscriptSegment, TranscriptState,
};

#[test]
fn meeting_recording_lifecycle_preserves_recovery_before_completion_and_delete() {
    let mut meeting = Meeting::new_manual("meeting-1", "Weekly planning", 1_000);
    assert_eq!(meeting.status, MeetingStatus::Created);
    assert_eq!(meeting.transcript_state, TranscriptState::NotStarted);

    let session = RecordingSession::start(
        "session-1",
        meeting.id.clone(),
        RecordingSource::Mixed,
        1_100,
        48_000,
    );
    meeting.start_recording(&session);
    assert_eq!(meeting.status, MeetingStatus::Recording);
    assert_eq!(session.status, RecordingStatus::Recording);

    let interrupted = session.interrupt(1_500, "process exited while audio was open");
    meeting.mark_interrupted(&interrupted);
    assert_eq!(meeting.status, MeetingStatus::Interrupted);
    assert_eq!(interrupted.status, RecordingStatus::Interrupted);
    assert!(interrupted.recovery_note.as_deref().expect("note").contains("process exited"));

    let recovered = interrupted.recover(1_800);
    meeting.mark_recovered(&recovered);
    assert_eq!(meeting.status, MeetingStatus::Recovered);
    assert_eq!(recovered.status, RecordingStatus::Recovered);

    meeting.start_transcribing();
    assert_eq!(meeting.status, MeetingStatus::Transcribing);
    assert_eq!(meeting.transcript_state, TranscriptState::Transcribing);

    let segment = TranscriptSegment::new("segment-1", meeting.id.clone(), 0, 2_000, "hello");
    meeting.complete(2_500, vec![segment]);
    assert_eq!(meeting.status, MeetingStatus::Complete);
    assert_eq!(meeting.transcript_state, TranscriptState::Complete);
    assert_eq!(meeting.ended_at_ms, Some(2_500));

    meeting.delete(3_000);
    assert_eq!(meeting.status, MeetingStatus::Deleted);
    assert_eq!(meeting.deleted_at_ms, Some(3_000));
}

#[test]
fn domain_models_include_phase_one_artifact_and_job_states() {
    let artifact = AudioArtifact::new_private(
        "artifact-1",
        "session-1",
        ArtifactKind::RawMic,
        "meetings/meeting-1/audio/raw-mic.wav",
        "sha256:abc",
    );
    assert!(artifact.retained);

    let statuses = [
        JobStatus::Queued,
        JobStatus::Running,
        JobStatus::Succeeded,
        JobStatus::Failed,
        JobStatus::Canceled,
        JobStatus::Retry,
        JobStatus::Recovery,
    ];

    for status in statuses {
        let job = curiosity_domain::ProcessingJob::new(
            format!("job-{status:?}"),
            "meeting-1",
            JobKind::Transcribe,
            status,
        );
        assert_eq!(job.status, status);
    }
}
