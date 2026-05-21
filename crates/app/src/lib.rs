use std::fs;
use std::path::PathBuf;

use curiosity_audio::{AudioCapture, AudioFrame, CapturePermission, CapturePermissionError};
use curiosity_domain::{
    ArtifactKind, AudioArtifact, Meeting, MeetingStatus, RecordingSession, RecordingSource,
    RecordingStatus,
};
use curiosity_store::Store;
use serde::{Deserialize, Serialize};

pub type AppResult<T> = Result<T, RecordingError>;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum CommandRecordingState {
    Recording,
    Paused,
    Stopping,
    Interrupted,
    Recovering,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AppPermissionState {
    Ready,
    MicrophoneDenied,
    SystemAudioDenied,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum RawAudioRetentionPolicy {
    Retain,
    DeleteAfterTranscription,
    NeverSave,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StorageLocationDto {
    pub app_private_path: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommandRecordingDto {
    pub meeting_id: String,
    pub recording_id: Option<String>,
    pub state: CommandRecordingState,
    pub permission_state: AppPermissionState,
    pub storage_location: StorageLocationDto,
    pub raw_audio_retention: RawAudioRetentionPolicy,
    pub recoverable: bool,
    pub recovery_action: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MeetingDetailDto {
    pub meeting_id: String,
    pub title: String,
    pub transcript_segments: Vec<TranscriptSegmentDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TranscriptSegmentDto {
    pub id: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub source_channel: String,
    pub model_run_id: String,
    pub transcript_version_id: String,
    pub original_text: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MeetingSummaryDto {
    pub meeting_id: String,
    pub title: String,
    pub started_at_ms: u64,
    pub ended_at_ms: Option<u64>,
    pub status: String,
    pub transcript_state: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MeetingSearchResultDto {
    pub meeting_id: String,
    pub title: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExportedMeetingDto {
    pub meeting_id: String,
    pub path: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DeletedMeetingDto {
    pub meeting_id: String,
    pub deleted_private_artifacts: Vec<String>,
    pub skipped_private_artifacts: Vec<String>,
    pub remaining_exports: Vec<String>,
}

pub fn meeting_detail_dto(
    store: &Store,
    meeting_id: &str,
) -> curiosity_store::StoreResult<MeetingDetailDto> {
    let title = store.meeting_title(meeting_id)?;
    let transcript_segments = store
        .transcript_segments(meeting_id)?
        .into_iter()
        .map(|segment| TranscriptSegmentDto {
            id: segment.id,
            start_ms: segment.start_ms,
            end_ms: segment.end_ms,
            text: segment.text,
            source_channel: format!("{:?}", segment.source_channel),
            model_run_id: segment.model_run_id,
            transcript_version_id: segment.transcript_version_id,
            original_text: segment.original_text,
        })
        .collect();
    Ok(MeetingDetailDto {
        meeting_id: meeting_id.to_string(),
        title,
        transcript_segments,
    })
}

pub fn list_meetings_dto(store: &Store) -> curiosity_store::StoreResult<Vec<MeetingSummaryDto>> {
    Ok(store
        .list_meetings()?
        .into_iter()
        .map(meeting_summary_dto)
        .collect())
}

pub fn search_meetings_dto(
    store: &Store,
    query: &str,
) -> curiosity_store::StoreResult<Vec<MeetingSearchResultDto>> {
    Ok(store
        .search_meetings(query)?
        .into_iter()
        .map(|result| MeetingSearchResultDto {
            meeting_id: result.meeting_id,
            title: result.title,
        })
        .collect())
}

pub fn rename_meeting_command(
    store: &Store,
    meeting_id: &str,
    title: &str,
) -> curiosity_store::StoreResult<MeetingSummaryDto> {
    Ok(meeting_summary_dto(store.rename_meeting(meeting_id, title)?))
}

pub fn export_meeting_json_command(
    store: &Store,
    meeting_id: &str,
    export_root: impl AsRef<std::path::Path>,
) -> curiosity_store::StoreResult<ExportedMeetingDto> {
    let path = store.export_meeting_json(meeting_id, export_root.as_ref())?;
    Ok(ExportedMeetingDto {
        meeting_id: meeting_id.to_string(),
        path: path.to_string_lossy().to_string(),
    })
}

pub fn delete_meeting_command(
    store: &Store,
    meeting_id: &str,
) -> curiosity_store::StoreResult<DeletedMeetingDto> {
    let report = store.delete_meeting(meeting_id)?;
    Ok(DeletedMeetingDto {
        meeting_id: meeting_id.to_string(),
        deleted_private_artifacts: report
            .deleted_private_artifacts
            .into_iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect(),
        skipped_private_artifacts: report
            .skipped_private_artifacts
            .into_iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect(),
        remaining_exports: report
            .exported_files_outside_app_control
            .into_iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect(),
    })
}

fn meeting_summary_dto(summary: curiosity_store::MeetingSummary) -> MeetingSummaryDto {
    MeetingSummaryDto {
        meeting_id: summary.meeting_id,
        title: summary.title,
        started_at_ms: summary.started_at_ms,
        ended_at_ms: summary.ended_at_ms,
        status: summary.status,
        transcript_state: summary.transcript_state,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordingErrorKind {
    DiskFull,
    PermissionDenied,
    StorageUnavailable,
    NoActiveRecording,
    AlreadyRecording,
    NoRecoverableRecording,
}

#[derive(Debug)]
pub struct RecordingError {
    pub kind: RecordingErrorKind,
    pub message: String,
    pub trust_state: CommandRecordingDto,
}

impl std::fmt::Display for RecordingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for RecordingError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageSetup {
    pub relative_audio_dir: String,
    pub artifact_path: String,
}

pub trait ArtifactSink {
    fn setup_recording(&self, meeting_id: &str) -> Result<StorageSetup, StorageSetupError>;
    fn write_frames(&self, setup: &StorageSetup, frames: &[AudioFrame])
        -> Result<(), StorageSetupError>;
    fn has_recovery_evidence(&self, setup: &StorageSetup) -> bool;
    fn recover_recording(&self, setup: &StorageSetup) -> Result<(), StorageSetupError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageSetupError {
    pub kind: RecordingErrorKind,
    pub message: String,
}

impl StorageSetupError {
    pub fn disk_full(message: impl Into<String>) -> Self {
        Self {
            kind: RecordingErrorKind::DiskFull,
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct FakeArtifactSink {
    meetings_root: PathBuf,
    setup_error: Option<StorageSetupError>,
    write_failure: Option<FakeWriteFailure>,
}

#[derive(Clone, Debug)]
enum FakeWriteFailure {
    BeforeBytes(StorageSetupError),
    AfterBytes(StorageSetupError),
}

impl FakeArtifactSink {
    pub fn new(meetings_root: PathBuf) -> Self {
        Self {
            meetings_root,
            setup_error: None,
            write_failure: None,
        }
    }

    pub fn failing(meetings_root: PathBuf, error: StorageSetupError) -> Self {
        Self {
            meetings_root,
            setup_error: Some(error),
            write_failure: None,
        }
    }

    pub fn fail_after_setup(meetings_root: PathBuf, error: StorageSetupError) -> Self {
        Self {
            meetings_root,
            setup_error: None,
            write_failure: Some(FakeWriteFailure::BeforeBytes(error)),
        }
    }

    pub fn fail_after_writing_bytes(meetings_root: PathBuf, error: StorageSetupError) -> Self {
        Self {
            meetings_root,
            setup_error: None,
            write_failure: Some(FakeWriteFailure::AfterBytes(error)),
        }
    }
}

impl ArtifactSink for FakeArtifactSink {
    fn setup_recording(&self, meeting_id: &str) -> Result<StorageSetup, StorageSetupError> {
        if let Some(error) = &self.setup_error {
            return Err(error.clone());
        }
        let audio_dir = self.meetings_root.join(meeting_id).join("audio");
        fs::create_dir_all(&audio_dir).map_err(|err| StorageSetupError {
            kind: RecordingErrorKind::StorageUnavailable,
            message: err.to_string(),
        })?;
        Ok(StorageSetup {
            relative_audio_dir: format!("meetings/{meeting_id}/audio"),
            artifact_path: format!("meetings/{meeting_id}/audio/mixed.pcm"),
        })
    }

    fn write_frames(
        &self,
        setup: &StorageSetup,
        frames: &[AudioFrame],
    ) -> Result<(), StorageSetupError> {
        if let Some(FakeWriteFailure::BeforeBytes(error)) = &self.write_failure {
            return Err(error.clone());
        }
        let artifact_path = relative_to_meetings_root(&self.meetings_root, &setup.artifact_path);
        if let Some(parent) = artifact_path.parent() {
            fs::create_dir_all(parent).map_err(|err| StorageSetupError {
                kind: RecordingErrorKind::StorageUnavailable,
                message: err.to_string(),
            })?;
        }
        let mut bytes = Vec::new();
        for frame in frames {
            for sample in &frame.pcm_i16 {
                bytes.extend_from_slice(&sample.to_le_bytes());
            }
        }
        fs::write(&artifact_path, bytes).map_err(|err| StorageSetupError {
            kind: RecordingErrorKind::StorageUnavailable,
            message: err.to_string(),
        })?;
        if let Some(FakeWriteFailure::AfterBytes(error)) = &self.write_failure {
            return Err(error.clone());
        }
        Ok(())
    }

    fn has_recovery_evidence(&self, setup: &StorageSetup) -> bool {
        let artifact_path = relative_to_meetings_root(&self.meetings_root, &setup.artifact_path);
        artifact_path
            .metadata()
            .map(|metadata| metadata.len() > 0)
            .unwrap_or(false)
    }

    fn recover_recording(&self, setup: &StorageSetup) -> Result<(), StorageSetupError> {
        if self.has_recovery_evidence(setup) {
            Ok(())
        } else {
            Err(StorageSetupError {
                kind: RecordingErrorKind::StorageUnavailable,
                message: "recoverable recording artifact directory is missing".to_string(),
            })
        }
    }
}

pub struct ManualRecordingService<C, S> {
    store: Store,
    capture: C,
    sink: S,
    active: Option<ActiveRecording>,
    interrupted: Option<InterruptedRecording>,
}

impl<C, S> ManualRecordingService<C, S>
where
    C: AudioCapture,
    S: ArtifactSink,
{
    pub fn new(store: Store, capture: C, sink: S) -> Self {
        Self {
            store,
            capture,
            sink,
            active: None,
            interrupted: None,
        }
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn active_recording(&self) -> Option<&str> {
        self.active.as_ref().map(|active| active.recording_id.as_str())
    }

    pub fn start_manual_recording(
        &mut self,
        meeting_id: &str,
        title: &str,
        started_at_ms: u64,
    ) -> AppResult<CommandRecordingDto> {
        if self.active.is_some() {
            return Err(simple_error(
                RecordingErrorKind::AlreadyRecording,
                meeting_id,
                CommandRecordingState::Recording,
                "Stop the current recording before starting another one",
            ));
        }

        let snapshot = self.capture.device_snapshot().map_err(|err| {
            permission_error(meeting_id, &storage_location(meeting_id), err)
        })?;
        let setup = self.sink.setup_recording(meeting_id).map_err(|err| {
            storage_error(meeting_id, CommandRecordingState::Interrupted, false, err)
        })?;

        let recording_id = format!("recording-{meeting_id}");
        let mut meeting = Meeting::new_manual(meeting_id, title, started_at_ms);
        let session = RecordingSession::start(
            &recording_id,
            meeting_id,
            RecordingSource::Mixed,
            started_at_ms,
            snapshot
                .mic
                .as_ref()
                .or(snapshot.system.as_ref())
                .map(|stream| stream.sample_rate_hz)
                .unwrap_or(48_000),
        );
        meeting.start_recording(&session);
        let artifact = AudioArtifact::new_private(
            format!("artifact-{meeting_id}"),
            &recording_id,
            ArtifactKind::Mixed,
            &setup.artifact_path,
            "sha256:pending",
        );

        self.store
            .insert_meeting(&meeting)
            .map_err(|err| store_error(meeting_id, err.to_string()))?;
        self.store
            .insert_recording_session(&session)
            .map_err(|err| store_error(meeting_id, err.to_string()))?;
        self.store
            .insert_audio_artifact(&artifact)
            .map_err(|err| store_error(meeting_id, err.to_string()))?;

        self.active = Some(ActiveRecording {
            meeting_id: meeting_id.to_string(),
            recording_id: recording_id.clone(),
            setup,
            status: RecordingStatus::Recording,
        });

        Ok(dto(
            meeting_id,
            Some(recording_id),
            CommandRecordingState::Recording,
            AppPermissionState::Ready,
            RawAudioRetentionPolicy::Retain,
            false,
            "Recording locally to private app storage",
        ))
    }

    pub fn pause_active_recording(&mut self) -> AppResult<CommandRecordingDto> {
        let active = self.active.as_mut().ok_or_else(|| {
            simple_error(
                RecordingErrorKind::NoActiveRecording,
                "unknown",
                CommandRecordingState::Interrupted,
                "Start a recording before pausing",
            )
        })?;
        active.status = RecordingStatus::Paused;
        self.store
            .update_meeting_status(&active.meeting_id, MeetingStatus::Paused, None)
            .map_err(|err| store_error(&active.meeting_id, err.to_string()))?;
        self.store
            .update_recording_session_status(&active.recording_id, RecordingStatus::Paused, None, None)
            .map_err(|err| store_error(&active.meeting_id, err.to_string()))?;
        Ok(dto(
            &active.meeting_id,
            Some(active.recording_id.clone()),
            CommandRecordingState::Paused,
            AppPermissionState::Ready,
            RawAudioRetentionPolicy::Retain,
            false,
            "Recording is paused; resume or stop when ready",
        ))
    }

    pub fn stop_active_recording(&mut self, ended_at_ms: u64) -> AppResult<CommandRecordingDto> {
        let mut active = self.active.take().ok_or_else(|| {
            simple_error(
                RecordingErrorKind::NoActiveRecording,
                "unknown",
                CommandRecordingState::Interrupted,
                "Start a recording before stopping",
            )
        })?;
        active.status = RecordingStatus::Stopping;
        self.store
            .update_meeting_status(&active.meeting_id, MeetingStatus::Complete, Some(ended_at_ms))
            .map_err(|err| store_error(&active.meeting_id, err.to_string()))?;
        self.store
            .update_recording_session_status(
                &active.recording_id,
                RecordingStatus::Complete,
                Some(ended_at_ms),
                None,
            )
            .map_err(|err| store_error(&active.meeting_id, err.to_string()))?;
        Ok(dto(
            &active.meeting_id,
            Some(active.recording_id),
            CommandRecordingState::Stopping,
            AppPermissionState::Ready,
            RawAudioRetentionPolicy::Retain,
            false,
            "Finalizing local recording artifacts",
        ))
    }

    pub fn recover_interrupted_recording(
        &mut self,
        meeting_id: &str,
        recording_id: &str,
        _recovered_at_ms: u64,
    ) -> AppResult<CommandRecordingDto> {
        let interrupted = self.interrupted.as_ref().ok_or_else(|| {
            no_recoverable_recording_error(meeting_id, recording_id)
        })?;
        if interrupted.meeting_id != meeting_id || interrupted.recording_id != recording_id {
            return Err(no_recoverable_recording_error(meeting_id, recording_id));
        }

        self.sink
            .recover_recording(&interrupted.setup)
            .map_err(|err| storage_error(meeting_id, CommandRecordingState::Interrupted, true, err))?;
        let interrupted = self.interrupted.take().expect("checked");
        self.store
            .update_meeting_status(&interrupted.meeting_id, MeetingStatus::Recovered, None)
            .map_err(|err| store_error(&interrupted.meeting_id, err.to_string()))?;
        self.store
            .update_recording_session_status(
                &interrupted.recording_id,
                RecordingStatus::Recovered,
                None,
                Some("recovered interrupted fake recording"),
            )
            .map_err(|err| store_error(&interrupted.meeting_id, err.to_string()))?;
        self.active = Some(ActiveRecording {
            meeting_id: interrupted.meeting_id.clone(),
            recording_id: interrupted.recording_id.clone(),
            setup: interrupted.setup,
            status: RecordingStatus::Recovered,
        });

        Ok(dto(
            meeting_id,
            Some(recording_id.to_string()),
            CommandRecordingState::Recovering,
            AppPermissionState::Ready,
            RawAudioRetentionPolicy::DeleteAfterTranscription,
            true,
            "Review recovered chunks before transcription",
        ))
    }

    pub fn write_fake_audio_chunk(&mut self) -> AppResult<CommandRecordingDto> {
        let active = self.active.as_ref().ok_or_else(|| {
            simple_error(
                RecordingErrorKind::NoActiveRecording,
                "unknown",
                CommandRecordingState::Interrupted,
                "Start a recording before writing audio",
            )
        })?;
        let frames = match self.capture.capture_frames() {
            Ok(frames) => frames,
            Err(err) => {
                let active = self.active.take().expect("checked active recording");
                self.mark_persisted_recording_failed(&active, "capture permission failed")?;
                return Err(active_permission_error(active, err));
            }
        };
        if let Err(err) = self.sink.write_frames(&active.setup, &frames) {
            let interrupted = self.active.take().expect("checked active recording");
            let meeting_id = interrupted.meeting_id.clone();
            let recording_id = interrupted.recording_id.clone();
            let recoverable = self.sink.has_recovery_evidence(&interrupted.setup);
            if recoverable {
                self.store
                    .update_meeting_status(&meeting_id, MeetingStatus::Interrupted, None)
                    .map_err(|err| store_error(&meeting_id, err.to_string()))?;
                self.store
                    .update_recording_session_status(
                        &recording_id,
                        RecordingStatus::Interrupted,
                        None,
                        Some("fake audio write failed after recoverable evidence"),
                    )
                    .map_err(|err| store_error(&meeting_id, err.to_string()))?;
                self.interrupted = Some(InterruptedRecording {
                    meeting_id: interrupted.meeting_id,
                    recording_id: interrupted.recording_id,
                    setup: interrupted.setup,
                });
            } else {
                self.mark_persisted_recording_failed(&interrupted, "fake audio write failed before recoverable evidence")?;
            }
            return Err(storage_error_with_recording(
                &meeting_id,
                recoverable.then_some(recording_id),
                CommandRecordingState::Interrupted,
                recoverable,
                err,
            ));
        }
        Ok(dto(
            &active.meeting_id,
            Some(active.recording_id.clone()),
            CommandRecordingState::Recording,
            AppPermissionState::Ready,
            RawAudioRetentionPolicy::Retain,
            false,
            "Audio chunk written to private app storage",
        ))
    }

    fn mark_persisted_recording_failed(
        &self,
        active: &ActiveRecording,
        note: &str,
    ) -> AppResult<()> {
        self.store
            .update_meeting_status(&active.meeting_id, MeetingStatus::Failed, None)
            .map_err(|err| store_error(&active.meeting_id, err.to_string()))?;
        self.store
            .update_recording_session_status(
                &active.recording_id,
                RecordingStatus::Failed,
                None,
                Some(note),
            )
            .map_err(|err| store_error(&active.meeting_id, err.to_string()))?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct ActiveRecording {
    meeting_id: String,
    recording_id: String,
    setup: StorageSetup,
    status: RecordingStatus,
}

#[derive(Clone, Debug)]
struct InterruptedRecording {
    meeting_id: String,
    recording_id: String,
    setup: StorageSetup,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpeechSource {
    Microphone,
    System,
    Mixed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpeechSegment {
    pub source: SpeechSource,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

impl SpeechSegment {
    pub fn new(source: SpeechSource, start_ms: u64, end_ms: u64, text: impl Into<String>) -> Self {
        Self {
            source,
            start_ms,
            end_ms,
            text: text.into(),
        }
    }
}

pub fn dedupe_selected_segments(
    selected_source: SpeechSource,
    segments: &[SpeechSegment],
) -> Vec<SpeechSegment> {
    let mut selected: Vec<SpeechSegment> = segments
        .iter()
        .filter(|segment| selected_source == SpeechSource::Mixed || segment.source == selected_source)
        .cloned()
        .collect();
    selected.sort_by_key(|segment| (segment.start_ms, segment.end_ms));

    if selected_source != SpeechSource::Mixed {
        return selected;
    }

    let mut deduped: Vec<SpeechSegment> = Vec::new();
    for segment in selected {
        let duplicate = deduped.iter().any(|existing| {
            normalized_text(&existing.text) == normalized_text(&segment.text)
                && ranges_overlap(existing.start_ms, existing.end_ms, segment.start_ms, segment.end_ms)
        });
        if !duplicate {
            deduped.push(segment);
        }
    }
    deduped
}

fn dto(
    meeting_id: &str,
    recording_id: Option<String>,
    state: CommandRecordingState,
    permission_state: AppPermissionState,
    raw_audio_retention: RawAudioRetentionPolicy,
    recoverable: bool,
    recovery_action: &str,
) -> CommandRecordingDto {
    CommandRecordingDto {
        meeting_id: meeting_id.to_string(),
        recording_id,
        state,
        permission_state,
        storage_location: StorageLocationDto {
            app_private_path: storage_location(meeting_id),
        },
        raw_audio_retention,
        recoverable,
        recovery_action: recovery_action.to_string(),
    }
}

fn storage_location(meeting_id: &str) -> String {
    format!("meetings/{meeting_id}/audio")
}

fn permission_error(
    meeting_id: &str,
    storage_location: &str,
    err: CapturePermissionError,
) -> RecordingError {
    let permission_state = match err.permission {
        CapturePermission::Microphone => AppPermissionState::MicrophoneDenied,
        CapturePermission::SystemAudioScreenRecording => AppPermissionState::SystemAudioDenied,
    };
    let guidance = err.recovery_guidance();
    RecordingError {
        kind: RecordingErrorKind::PermissionDenied,
        message: err.message,
        trust_state: CommandRecordingDto {
            meeting_id: meeting_id.to_string(),
            recording_id: None,
            state: CommandRecordingState::Interrupted,
            permission_state,
            storage_location: StorageLocationDto {
                app_private_path: storage_location.to_string(),
            },
            raw_audio_retention: RawAudioRetentionPolicy::Retain,
            recoverable: false,
            recovery_action: guidance.title,
        },
    }
}

fn active_permission_error(active: ActiveRecording, err: CapturePermissionError) -> RecordingError {
    let permission_state = match err.permission {
        CapturePermission::Microphone => AppPermissionState::MicrophoneDenied,
        CapturePermission::SystemAudioScreenRecording => AppPermissionState::SystemAudioDenied,
    };
    let guidance = err.recovery_guidance();
    RecordingError {
        kind: RecordingErrorKind::PermissionDenied,
        message: err.message,
        trust_state: CommandRecordingDto {
            meeting_id: active.meeting_id,
            recording_id: None,
            state: CommandRecordingState::Interrupted,
            permission_state,
            storage_location: StorageLocationDto {
                app_private_path: active.setup.relative_audio_dir,
            },
            raw_audio_retention: RawAudioRetentionPolicy::Retain,
            recoverable: false,
            recovery_action: guidance.title,
        },
    }
}

fn storage_error(
    meeting_id: &str,
    state: CommandRecordingState,
    recoverable: bool,
    err: StorageSetupError,
) -> RecordingError {
    storage_error_with_recording(meeting_id, None, state, recoverable, err)
}

fn storage_error_with_recording(
    meeting_id: &str,
    recording_id: Option<String>,
    state: CommandRecordingState,
    recoverable: bool,
    err: StorageSetupError,
) -> RecordingError {
    let recovery_action = match err.kind {
        RecordingErrorKind::DiskFull => "Free disk space, then recover or restart recording",
        _ => "Check private app storage access, then restart recording",
    };
    RecordingError {
        kind: err.kind,
        message: err.message,
        trust_state: dto(
            meeting_id,
            recording_id,
            state,
            AppPermissionState::Ready,
            RawAudioRetentionPolicy::Retain,
            recoverable,
            recovery_action,
        ),
    }
}

fn store_error(meeting_id: &str, message: String) -> RecordingError {
    RecordingError {
        kind: RecordingErrorKind::StorageUnavailable,
        message,
        trust_state: dto(
            meeting_id,
            None,
            CommandRecordingState::Interrupted,
            AppPermissionState::Ready,
            RawAudioRetentionPolicy::Retain,
            false,
            "Check private app storage access, then restart recording",
        ),
    }
}

fn simple_error(
    kind: RecordingErrorKind,
    meeting_id: &str,
    state: CommandRecordingState,
    action: &str,
) -> RecordingError {
    RecordingError {
        kind,
        message: action.to_string(),
        trust_state: dto(
            meeting_id,
            None,
            state,
            AppPermissionState::Ready,
            RawAudioRetentionPolicy::Retain,
            false,
            action,
        ),
    }
}

fn no_recoverable_recording_error(meeting_id: &str, recording_id: &str) -> RecordingError {
    RecordingError {
        kind: RecordingErrorKind::NoRecoverableRecording,
        message: format!("no recoverable recording found for {recording_id}"),
        trust_state: dto(
            meeting_id,
            Some(recording_id.to_string()),
            CommandRecordingState::Interrupted,
            AppPermissionState::Ready,
            RawAudioRetentionPolicy::Retain,
            false,
            "Start a new recording or choose the interrupted recording that needs recovery",
        ),
    }
}

fn relative_to_meetings_root(meetings_root: &std::path::Path, artifact_path: &str) -> PathBuf {
    let prefix = "meetings/";
    if let Some(path) = artifact_path.strip_prefix(prefix) {
        meetings_root.join(path)
    } else {
        meetings_root.join(artifact_path)
    }
}

fn normalized_text(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn ranges_overlap(left_start: u64, left_end: u64, right_start: u64, right_end: u64) -> bool {
    left_start < right_end && right_start < left_end
}
