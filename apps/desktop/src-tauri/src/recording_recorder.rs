use std::path::Path;

use curiosity_app::AppPermissionState;
use curiosity_audio::{
    ArtifactManifest, CaptureCapability, CaptureError, CapturePermission, MacosDesktopWavRecording,
    MacosMicrophoneWavRecording, StreamKind,
};

pub(super) struct StartedMicrophoneRecording {
    pub(super) sample_rate_hz: u32,
    pub(super) streams: Vec<StreamKind>,
    pub(super) recorder: Box<dyn ActiveMicrophoneRecording>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct MicrophoneStartFailure {
    pub(super) permission_state: AppPermissionState,
    pub(super) message: String,
    pub(super) recovery_action: String,
}

impl MicrophoneStartFailure {
    pub(super) fn persistence(message: impl Into<String>) -> Self {
        Self {
            permission_state: AppPermissionState::MicrophoneUnavailable,
            message: message.into(),
            recovery_action: "Check local storage permissions and retry microphone recording."
                .to_string(),
        }
    }

    #[cfg(test)]
    pub(super) fn permission_denied(message: impl Into<String>) -> Self {
        Self {
            permission_state: AppPermissionState::MicrophoneDenied,
            message: message.into(),
            recovery_action:
                "Open System Settings; go to Privacy & Security, then Microphone; allow Curiosity Transcripts and retry recording."
                    .to_string(),
        }
    }

    fn from_capture_error(error: CaptureError) -> Self {
        match error {
            CaptureError::PermissionDenied(error) => {
                let permission_state = match error.permission {
                    CapturePermission::Microphone => AppPermissionState::MicrophoneDenied,
                    CapturePermission::SystemAudioScreenRecording => {
                        AppPermissionState::SystemAudioDenied
                    }
                };
                let guidance = error.recovery_guidance();
                Self {
                    permission_state,
                    message: error.to_string(),
                    recovery_action: guidance.steps.join("; "),
                }
            }
            CaptureError::Unavailable(error) => {
                let permission_state = match error.capability {
                    CaptureCapability::Microphone => AppPermissionState::MicrophoneUnavailable,
                    CaptureCapability::SystemAudio => AppPermissionState::SystemAudioUnavailable,
                };
                let guidance = error.recovery_guidance();
                Self {
                    permission_state,
                    message: error.to_string(),
                    recovery_action: guidance.steps.join("; "),
                }
            }
            CaptureError::Configuration(error) => Self {
                permission_state: AppPermissionState::MicrophoneUnavailable,
                message: error.to_string(),
                recovery_action: "Check the microphone capture configuration and retry recording."
                    .to_string(),
            },
            CaptureError::Recording(error) => Self {
                permission_state: AppPermissionState::MicrophoneUnavailable,
                message: error.to_string(),
                recovery_action:
                    "Check local storage and microphone availability, then retry recording."
                        .to_string(),
            },
        }
    }
}

pub(super) trait MicrophoneRecorderFactory {
    fn start(
        &self,
        audio_root: &Path,
        recording_id: &str,
        started_at_ms: u64,
    ) -> Result<StartedMicrophoneRecording, MicrophoneStartFailure>;
}

pub(super) trait ActiveMicrophoneRecording: Send {
    fn stop(self: Box<Self>, ended_at_ms: u64) -> Result<ArtifactManifest, String>;
}

pub(super) struct RealMicrophoneRecorderFactory;

impl MicrophoneRecorderFactory for RealMicrophoneRecorderFactory {
    fn start(
        &self,
        audio_root: &Path,
        recording_id: &str,
        started_at_ms: u64,
    ) -> Result<StartedMicrophoneRecording, MicrophoneStartFailure> {
        match MacosDesktopWavRecording::start(audio_root, recording_id, started_at_ms) {
            Ok(recorder) => {
                let sample_rate_hz = recorder.sample_rate_hz();
                Ok(StartedMicrophoneRecording {
                    sample_rate_hz,
                    streams: vec![StreamKind::Microphone, StreamKind::SystemAudio],
                    recorder: Box::new(recorder),
                })
            }
            Err(error) if can_fallback_to_microphone_recording(&error) => {
                let recorder =
                    MacosMicrophoneWavRecording::start(audio_root, recording_id, started_at_ms)
                        .map_err(MicrophoneStartFailure::from_capture_error)?;
                let sample_rate_hz = recorder.sample_rate_hz();
                Ok(StartedMicrophoneRecording {
                    sample_rate_hz,
                    streams: vec![StreamKind::Microphone],
                    recorder: Box::new(recorder),
                })
            }
            Err(error) => Err(MicrophoneStartFailure::from_capture_error(error)),
        }
    }
}

pub(super) fn can_fallback_to_microphone_recording(error: &CaptureError) -> bool {
    matches!(
        error,
        CaptureError::Unavailable(unavailable)
            if unavailable.capability == CaptureCapability::SystemAudio
    ) || matches!(
        error,
        CaptureError::PermissionDenied(permission)
            if permission.permission == CapturePermission::SystemAudioScreenRecording
    )
}

impl ActiveMicrophoneRecording for MacosDesktopWavRecording {
    fn stop(self: Box<Self>, ended_at_ms: u64) -> Result<ArtifactManifest, String> {
        (*self).stop(ended_at_ms).map_err(|error| error.to_string())
    }
}

impl ActiveMicrophoneRecording for MacosMicrophoneWavRecording {
    fn stop(self: Box<Self>, ended_at_ms: u64) -> Result<ArtifactManifest, String> {
        (*self).stop(ended_at_ms).map_err(|error| error.to_string())
    }
}
