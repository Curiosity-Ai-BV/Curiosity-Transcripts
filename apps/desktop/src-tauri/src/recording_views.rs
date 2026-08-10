use std::path::Path;

use curiosity_app::{
    AppPermissionState, CommandRecordingDto, CommandRecordingState, RawAudioRetentionPolicy,
    StorageLocationDto,
};
#[cfg(not(test))]
use curiosity_audio::{ScreenCaptureKitSystemAudioAdapter, SystemAudioAdapterStatus};
use serde::Serialize;

use crate::recording_artifact_paths::microphone_storage_path;
use crate::recording_recorder::MicrophoneStartFailure;

use super::{DesktopCommandSnapshotState, MeetingView};

pub(super) fn recording_snapshot(
    app_root: &Path,
    command_state: &DesktopCommandSnapshotState,
) -> CommandRecordingDto {
    if let Some(active) = &command_state.active_recording {
        return recording_dto_with_retention(
            &active.meeting_id,
            Some(active.recording_id.clone()),
            CommandRecordingState::Recording,
            AppPermissionState::Ready,
            microphone_storage_path(&active.meeting_id),
            active.raw_audio_retention_policy,
            "Recording locally to private app storage",
        );
    }
    if let Some(recording) = &command_state.last_recording {
        return recording.clone();
    }
    recording_dto(
        "",
        None,
        CommandRecordingState::Idle,
        AppPermissionState::Ready,
        app_root.display().to_string(),
        "Start a desktop recording to create private microphone and system audio WAV artifacts.",
    )
}

pub(super) fn microphone_capture_state(
    command_state: &DesktopCommandSnapshotState,
) -> DesktopPermissionState {
    if command_state.active_recording.is_some() {
        return DesktopPermissionState::Ready;
    }
    if let Some(recording) = &command_state.last_recording {
        return match recording.permission_state {
            AppPermissionState::Ready => DesktopPermissionState::Ready,
            AppPermissionState::MicrophoneDenied => DesktopPermissionState::MicrophoneDenied,
            AppPermissionState::MicrophoneUnavailable => {
                DesktopPermissionState::MicrophoneUnavailable
            }
            AppPermissionState::SystemAudioDenied | AppPermissionState::SystemAudioUnavailable => {
                DesktopPermissionState::Ready
            }
        };
    }
    DesktopPermissionState::Ready
}

pub(super) fn meetings_have_system_audio_transcript(meetings: &[MeetingView]) -> bool {
    meetings.iter().any(|meeting| {
        meeting
            .segments
            .iter()
            .any(|segment| segment.source_channel == "System")
    })
}

pub(super) fn system_audio_capture_state(
    command_state: &DesktopCommandSnapshotState,
    has_system_audio_transcript: bool,
) -> DesktopPermissionState {
    if command_state
        .active_recording
        .as_ref()
        .map(|recording| recording.captures_system_audio)
        .unwrap_or(false)
    {
        return DesktopPermissionState::Ready;
    }
    if let Some(recording) = &command_state.last_recording {
        match recording.permission_state {
            AppPermissionState::SystemAudioDenied => {
                return DesktopPermissionState::SystemAudioDenied
            }
            AppPermissionState::SystemAudioUnavailable => {
                return DesktopPermissionState::SystemAudioUnavailable;
            }
            AppPermissionState::Ready => return DesktopPermissionState::Ready,
            AppPermissionState::MicrophoneDenied | AppPermissionState::MicrophoneUnavailable => {}
        }
    }
    if has_system_audio_transcript {
        return DesktopPermissionState::Ready;
    }
    #[cfg(test)]
    {
        DesktopPermissionState::SystemAudioUnavailable
    }
    #[cfg(not(test))]
    match ScreenCaptureKitSystemAudioAdapter::status() {
        SystemAudioAdapterStatus::Available => DesktopPermissionState::Ready,
        SystemAudioAdapterStatus::PermissionDenied(_) => DesktopPermissionState::SystemAudioDenied,
        SystemAudioAdapterStatus::Unavailable(_) => DesktopPermissionState::SystemAudioUnavailable,
    }
}

pub(super) fn start_failure_recording_dto(
    app_root: &Path,
    error: &MicrophoneStartFailure,
) -> CommandRecordingDto {
    recording_dto(
        "",
        None,
        CommandRecordingState::Interrupted,
        error.permission_state,
        app_root.display().to_string(),
        &format!(
            "Desktop recording could not start: {} {}",
            error.message, error.recovery_action
        ),
    )
}

fn recording_dto(
    meeting_id: &str,
    recording_id: Option<String>,
    state: CommandRecordingState,
    permission_state: AppPermissionState,
    storage_path: String,
    recovery_action: &str,
) -> CommandRecordingDto {
    recording_dto_with_retention(
        meeting_id,
        recording_id,
        state,
        permission_state,
        storage_path,
        RawAudioRetentionPolicy::Retain,
        recovery_action,
    )
}

pub(super) fn recording_dto_with_retention(
    meeting_id: &str,
    recording_id: Option<String>,
    state: CommandRecordingState,
    permission_state: AppPermissionState,
    storage_path: String,
    raw_audio_retention: RawAudioRetentionPolicy,
    recovery_action: &str,
) -> CommandRecordingDto {
    CommandRecordingDto {
        meeting_id: meeting_id.to_string(),
        recording_id,
        state,
        permission_state,
        storage_location: StorageLocationDto {
            app_private_path: storage_path,
        },
        raw_audio_retention,
        recoverable: false,
        recovery_action: recovery_action.to_string(),
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CaptureStatus {
    pub(super) microphone: DesktopPermissionState,
    pub(super) system_audio: DesktopPermissionState,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub(super) enum DesktopPermissionState {
    Ready,
    MicrophoneDenied,
    MicrophoneUnavailable,
    SystemAudioDenied,
    SystemAudioUnavailable,
}
