use curiosity_domain::{
    ArtifactKind, AudioArtifact, DomainTransitionError, JobKind, JobStatus, Meeting, MeetingStatus,
    RecordingSession, RecordingSource, RecordingStatus, TranscriptSegment, TranscriptState,
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
    meeting
        .start_recording(&session)
        .expect("matching recording session starts meeting");
    assert_eq!(meeting.status, MeetingStatus::Recording);
    assert_eq!(session.status, RecordingStatus::Recording);

    let paused = session.clone().pause();
    assert_eq!(paused.status, RecordingStatus::Paused);
    let stopping = paused.stop(1_250);
    assert_eq!(stopping.status, RecordingStatus::Stopping);
    assert_eq!(stopping.ended_at_ms, Some(1_250));

    let interrupted = session.interrupt(1_500, "process exited while audio was open");
    meeting
        .mark_interrupted(&interrupted)
        .expect("interrupted session marks meeting interrupted");
    assert_eq!(meeting.status, MeetingStatus::Interrupted);
    assert_eq!(interrupted.status, RecordingStatus::Interrupted);
    assert!(interrupted
        .recovery_note
        .as_deref()
        .expect("note")
        .contains("process exited"));

    let recovered = interrupted.recover(1_800);
    meeting
        .mark_recovered(&recovered)
        .expect("recovered session marks meeting recovered");
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
fn meeting_recording_transitions_reject_sessions_from_other_meetings() {
    let transitions = [
        (
            RecordingStatus::Recording,
            Meeting::start_recording
                as fn(&mut Meeting, &RecordingSession) -> Result<(), DomainTransitionError>,
        ),
        (
            RecordingStatus::Interrupted,
            Meeting::mark_interrupted
                as fn(&mut Meeting, &RecordingSession) -> Result<(), DomainTransitionError>,
        ),
        (
            RecordingStatus::Recovered,
            Meeting::mark_recovered
                as fn(&mut Meeting, &RecordingSession) -> Result<(), DomainTransitionError>,
        ),
    ];

    for (status, apply_transition) in transitions {
        let mut meeting = Meeting::new_manual("meeting-1", "Weekly planning", 1_000);
        let mut session = RecordingSession::start(
            "session-1",
            "meeting-2",
            RecordingSource::Mixed,
            1_100,
            48_000,
        );
        session.status = status;

        let result = apply_transition(&mut meeting, &session);

        assert_eq!(
            result,
            Err(DomainTransitionError::MismatchedAggregateIds {
                meeting_id: "meeting-1".to_string(),
                session_meeting_id: "meeting-2".to_string(),
            })
        );
        assert_eq!(meeting.status, MeetingStatus::Created);
    }
}

#[test]
fn meeting_recording_transitions_reject_unexpected_session_statuses() {
    let transitions = [
        (
            "start recording",
            RecordingStatus::Recording,
            RecordingStatus::Paused,
            Meeting::start_recording
                as fn(&mut Meeting, &RecordingSession) -> Result<(), DomainTransitionError>,
        ),
        (
            "mark interrupted",
            RecordingStatus::Interrupted,
            RecordingStatus::Recording,
            Meeting::mark_interrupted
                as fn(&mut Meeting, &RecordingSession) -> Result<(), DomainTransitionError>,
        ),
        (
            "mark recovered",
            RecordingStatus::Recovered,
            RecordingStatus::Interrupted,
            Meeting::mark_recovered
                as fn(&mut Meeting, &RecordingSession) -> Result<(), DomainTransitionError>,
        ),
    ];

    for (transition, expected, actual, apply_transition) in transitions {
        let mut meeting = Meeting::new_manual("meeting-1", "Weekly planning", 1_000);
        let mut session = RecordingSession::start(
            "session-1",
            meeting.id.clone(),
            RecordingSource::Mixed,
            1_100,
            48_000,
        );
        session.status = actual;

        let result = apply_transition(&mut meeting, &session);

        assert_eq!(
            result,
            Err(DomainTransitionError::InvalidRecordingSessionStatus {
                transition,
                expected,
                actual,
            })
        );
        assert_eq!(meeting.status, MeetingStatus::Created);
    }
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

#[test]
fn processing_job_metadata_defaults_are_legacy_compatible() {
    let job = curiosity_domain::ProcessingJob::new(
        "job-1",
        "meeting-1",
        JobKind::Transcribe,
        JobStatus::Queued,
    );

    assert_eq!(job.started_at_ms, None);
    assert_eq!(job.finished_at_ms, None);
    assert!(!job.cancel_requested);
    assert_eq!(job.idempotency_key, None);
    assert_eq!(job.last_error, None);
}
