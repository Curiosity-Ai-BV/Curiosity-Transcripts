//! Shared domain model for meetings, recordings, transcripts, artifacts, jobs,
//! and analysis data.
//!
//! This crate owns portable product state and transition rules. It should not
//! own persistence, desktop command DTOs, audio capture, transcription, or
//! analysis provider behavior.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MeetingStatus {
    Created,
    Recording,
    Paused,
    Stopping,
    Interrupted,
    Recovered,
    Transcribing,
    Complete,
    Failed,
    Deleted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TranscriptState {
    NotStarted,
    Transcribing,
    Complete,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Meeting {
    pub id: String,
    pub title: String,
    pub started_at_ms: u64,
    pub ended_at_ms: Option<u64>,
    pub deleted_at_ms: Option<u64>,
    pub status: MeetingStatus,
    pub transcript_state: TranscriptState,
    pub transcript_segments: Vec<TranscriptSegment>,
}

/// Error returned when a domain transition would cross aggregate boundaries or
/// violate the expected source state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DomainTransitionError {
    MismatchedAggregateIds {
        meeting_id: String,
        session_meeting_id: String,
    },
    InvalidRecordingSessionStatus {
        transition: &'static str,
        expected: RecordingStatus,
        actual: RecordingStatus,
    },
}

impl std::fmt::Display for DomainTransitionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MismatchedAggregateIds {
                meeting_id,
                session_meeting_id,
            } => write!(
                formatter,
                "recording session belongs to meeting {session_meeting_id}, not {meeting_id}"
            ),
            Self::InvalidRecordingSessionStatus {
                transition,
                expected,
                actual,
            } => write!(
                formatter,
                "cannot {transition} from recording session status {actual:?}; expected {expected:?}"
            ),
        }
    }
}

impl std::error::Error for DomainTransitionError {}

impl Meeting {
    pub fn new_manual(id: impl Into<String>, title: impl Into<String>, started_at_ms: u64) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            started_at_ms,
            ended_at_ms: None,
            deleted_at_ms: None,
            status: MeetingStatus::Created,
            transcript_state: TranscriptState::NotStarted,
            transcript_segments: Vec::new(),
        }
    }

    pub fn start_recording(
        &mut self,
        session: &RecordingSession,
    ) -> Result<(), DomainTransitionError> {
        self.validate_recording_session(session, RecordingStatus::Recording, "start recording")?;
        self.status = MeetingStatus::Recording;
        Ok(())
    }

    pub fn mark_interrupted(
        &mut self,
        session: &RecordingSession,
    ) -> Result<(), DomainTransitionError> {
        self.validate_recording_session(session, RecordingStatus::Interrupted, "mark interrupted")?;
        self.status = MeetingStatus::Interrupted;
        Ok(())
    }

    pub fn mark_recovered(
        &mut self,
        session: &RecordingSession,
    ) -> Result<(), DomainTransitionError> {
        self.validate_recording_session(session, RecordingStatus::Recovered, "mark recovered")?;
        self.status = MeetingStatus::Recovered;
        Ok(())
    }

    fn validate_recording_session(
        &self,
        session: &RecordingSession,
        expected: RecordingStatus,
        transition: &'static str,
    ) -> Result<(), DomainTransitionError> {
        if session.meeting_id != self.id {
            return Err(DomainTransitionError::MismatchedAggregateIds {
                meeting_id: self.id.clone(),
                session_meeting_id: session.meeting_id.clone(),
            });
        }
        if session.status != expected {
            return Err(DomainTransitionError::InvalidRecordingSessionStatus {
                transition,
                expected,
                actual: session.status,
            });
        }
        Ok(())
    }

    pub fn start_transcribing(&mut self) {
        self.status = MeetingStatus::Transcribing;
        self.transcript_state = TranscriptState::Transcribing;
    }

    pub fn complete(&mut self, ended_at_ms: u64, segments: Vec<TranscriptSegment>) {
        self.status = MeetingStatus::Complete;
        self.transcript_state = TranscriptState::Complete;
        self.ended_at_ms = Some(ended_at_ms);
        self.transcript_segments = segments;
    }

    pub fn delete(&mut self, deleted_at_ms: u64) {
        self.status = MeetingStatus::Deleted;
        self.deleted_at_ms = Some(deleted_at_ms);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordingSource {
    Microphone,
    System,
    Mixed,
    Imported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceChannel {
    Microphone,
    System,
    Mixed,
    Imported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordingStatus {
    Recording,
    Paused,
    Stopping,
    Interrupted,
    Recovered,
    Complete,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RawAudioRetentionPolicy {
    Retain,
    DeleteAfterTranscription,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordingSession {
    pub id: String,
    pub meeting_id: String,
    pub source: RecordingSource,
    pub started_at_ms: u64,
    pub ended_at_ms: Option<u64>,
    pub sample_rate_hz: u32,
    pub status: RecordingStatus,
    pub recovery_note: Option<String>,
    pub raw_audio_retention_policy: RawAudioRetentionPolicy,
}

impl RecordingSession {
    pub fn start(
        id: impl Into<String>,
        meeting_id: impl Into<String>,
        source: RecordingSource,
        started_at_ms: u64,
        sample_rate_hz: u32,
    ) -> Self {
        Self {
            id: id.into(),
            meeting_id: meeting_id.into(),
            source,
            started_at_ms,
            ended_at_ms: None,
            sample_rate_hz,
            status: RecordingStatus::Recording,
            recovery_note: None,
            raw_audio_retention_policy: RawAudioRetentionPolicy::Retain,
        }
    }

    pub fn with_raw_audio_retention_policy(mut self, policy: RawAudioRetentionPolicy) -> Self {
        self.raw_audio_retention_policy = policy;
        self
    }

    pub fn interrupt(mut self, ended_at_ms: u64, note: impl Into<String>) -> Self {
        self.status = RecordingStatus::Interrupted;
        self.ended_at_ms = Some(ended_at_ms);
        self.recovery_note = Some(note.into());
        self
    }

    pub fn pause(mut self) -> Self {
        self.status = RecordingStatus::Paused;
        self
    }

    pub fn stop(mut self, ended_at_ms: u64) -> Self {
        self.status = RecordingStatus::Stopping;
        self.ended_at_ms = Some(ended_at_ms);
        self
    }

    pub fn complete(mut self, ended_at_ms: u64) -> Self {
        self.status = RecordingStatus::Complete;
        self.ended_at_ms = Some(ended_at_ms);
        self
    }

    pub fn recover(mut self, recovered_at_ms: u64) -> Self {
        self.status = RecordingStatus::Recovered;
        self.recovery_note = Some(format!("recovered at {recovered_at_ms}"));
        self
    }

    pub fn fail(mut self, ended_at_ms: u64, note: impl Into<String>) -> Self {
        self.status = RecordingStatus::Failed;
        self.ended_at_ms = Some(ended_at_ms);
        self.recovery_note = Some(note.into());
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactKind {
    RawMic,
    RawSystem,
    Mixed,
    Imported,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioArtifact {
    pub id: String,
    pub recording_session_id: String,
    pub kind: ArtifactKind,
    pub path: String,
    pub sha256: String,
    pub retained: bool,
}

impl AudioArtifact {
    pub fn new_private(
        id: impl ToString,
        recording_session_id: impl ToString,
        kind: ArtifactKind,
        path: impl ToString,
        sha256: impl ToString,
    ) -> Self {
        Self {
            id: id.to_string(),
            recording_session_id: recording_session_id.to_string(),
            kind,
            path: path.to_string(),
            sha256: sha256.to_string(),
            retained: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobKind {
    Transcribe,
    Summarize,
    Export,
    Index,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Canceled,
    Retry,
    Recovery,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessingJob {
    pub id: String,
    pub meeting_id: String,
    pub kind: JobKind,
    pub status: JobStatus,
    pub attempts: u32,
    pub last_error: Option<String>,
    pub started_at_ms: Option<u64>,
    pub finished_at_ms: Option<u64>,
    pub cancel_requested: bool,
    pub idempotency_key: Option<String>,
}

impl ProcessingJob {
    pub fn new(
        id: impl ToString,
        meeting_id: impl ToString,
        kind: JobKind,
        status: JobStatus,
    ) -> Self {
        Self {
            id: id.to_string(),
            meeting_id: meeting_id.to_string(),
            kind,
            status,
            attempts: 0,
            last_error: None,
            started_at_ms: None,
            finished_at_ms: None,
            cancel_requested: false,
            idempotency_key: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelRun {
    pub id: String,
    pub meeting_id: String,
    pub source_artifact_sha256: String,
    pub provider: String,
    pub model_name: String,
    pub network_used: bool,
    pub created_at_ms: u64,
}

impl ModelRun {
    pub fn new(
        id: impl ToString,
        meeting_id: impl ToString,
        source_artifact_sha256: impl ToString,
        provider: impl ToString,
        model_name: impl ToString,
        network_used: bool,
        created_at_ms: u64,
    ) -> Self {
        Self {
            id: id.to_string(),
            meeting_id: meeting_id.to_string(),
            source_artifact_sha256: source_artifact_sha256.to_string(),
            provider: provider.to_string(),
            model_name: model_name.to_string(),
            network_used,
            created_at_ms,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TranscriptVersion {
    pub id: String,
    pub meeting_id: String,
    pub model_run_id: String,
    pub version: u32,
    pub created_at_ms: u64,
    pub edited_at_ms: Option<u64>,
}

impl TranscriptVersion {
    pub fn new(
        id: impl ToString,
        meeting_id: impl ToString,
        model_run_id: impl ToString,
        version: u32,
        created_at_ms: u64,
    ) -> Self {
        Self {
            id: id.to_string(),
            meeting_id: meeting_id.to_string(),
            model_run_id: model_run_id.to_string(),
            version,
            created_at_ms,
            edited_at_ms: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TranscriptSegment {
    pub id: String,
    pub meeting_id: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub source_channel: SourceChannel,
    pub model_run_id: String,
    pub transcript_version_id: String,
    pub original_text: Option<String>,
}

impl TranscriptSegment {
    pub fn new(
        id: impl Into<String>,
        meeting_id: impl Into<String>,
        start_ms: u64,
        end_ms: u64,
        text: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            meeting_id: meeting_id.into(),
            start_ms,
            end_ms,
            text: text.into(),
            source_channel: SourceChannel::Mixed,
            model_run_id: String::new(),
            transcript_version_id: String::new(),
            original_text: None,
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "metadata constructor mirrors persisted transcript segment fields"
    )]
    pub fn with_metadata(
        id: impl Into<String>,
        meeting_id: impl Into<String>,
        start_ms: u64,
        end_ms: u64,
        text: impl Into<String>,
        source_channel: SourceChannel,
        model_run_id: impl Into<String>,
        transcript_version_id: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            meeting_id: meeting_id.into(),
            start_ms,
            end_ms,
            text: text.into(),
            source_channel,
            model_run_id: model_run_id.into(),
            transcript_version_id: transcript_version_id.into(),
            original_text: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AnalysisCitation {
    pub segment_id: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AnalysisDecision {
    pub text: String,
    pub citations: Vec<AnalysisCitation>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AnalysisActionItem {
    pub text: String,
    pub owner: Option<String>,
    pub due_date: Option<String>,
    pub citations: Vec<AnalysisCitation>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AnalysisQuestion {
    pub text: String,
    pub citations: Vec<AnalysisCitation>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MeetingAnalysis {
    pub id: String,
    pub meeting_id: String,
    pub provider: String,
    pub model_name: String,
    pub network_used: bool,
    pub created_at_ms: u64,
    pub prompt_template_version: String,
    pub summary: String,
    pub decisions: Vec<AnalysisDecision>,
    pub action_items: Vec<AnalysisActionItem>,
    pub questions: Vec<AnalysisQuestion>,
    pub citations: Vec<AnalysisCitation>,
}
