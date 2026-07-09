use std::path::Path;
use std::time::Duration;

use crate::{
    record_macos_microphone_to_wav, record_macos_system_audio_to_wav, ArtifactManifest,
    CaptureError,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManualSmokeStatus {
    NotRun,
    Skipped,
    Unavailable,
    PermissionDenied,
    Passed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManualSmokeResult {
    pub status: ManualSmokeStatus,
    pub message: String,
}

pub struct ManualSmokeCheck;

impl ManualSmokeCheck {
    pub fn macos_placeholder() -> Self {
        Self
    }

    pub fn run_without_hardware(&self) -> ManualSmokeResult {
        ManualSmokeResult {
            status: ManualSmokeStatus::Skipped,
            message: "macOS audio smoke skipped; rerun audio-smoke with --attempt-mic to request microphone hardware capture"
                .to_string(),
        }
    }

    pub fn run_macos_microphone_capture(
        &self,
        root: &Path,
        duration: Duration,
    ) -> ManualSmokeResult {
        match record_macos_microphone_to_wav(root, duration) {
            Ok(manifest) => ManualSmokeResult::from_artifact_manifest(&manifest),
            Err(error) => ManualSmokeResult::from_capture_error(error),
        }
    }

    pub fn run_macos_system_audio_capture(
        &self,
        root: &Path,
        duration: Duration,
    ) -> ManualSmokeResult {
        match record_macos_system_audio_to_wav(root, duration) {
            Ok(manifest) => ManualSmokeResult::from_artifact_manifest(&manifest),
            Err(error) => ManualSmokeResult::from_capture_error(error),
        }
    }
}

impl ManualSmokeResult {
    pub fn from_capture_error(error: CaptureError) -> Self {
        match error {
            CaptureError::PermissionDenied(error) => {
                let guidance = error.recovery_guidance();
                Self {
                    status: ManualSmokeStatus::PermissionDenied,
                    message: format!("{}: {}", guidance.title, guidance.steps.join("; ")),
                }
            }
            CaptureError::Unavailable(error) => {
                let guidance = error.recovery_guidance();
                Self {
                    status: ManualSmokeStatus::Unavailable,
                    message: format!("{}: {}", error, guidance.steps.join("; ")),
                }
            }
            CaptureError::Configuration(error) => Self {
                status: ManualSmokeStatus::Unavailable,
                message: error.to_string(),
            },
            CaptureError::Recording(error) => Self {
                status: ManualSmokeStatus::Unavailable,
                message: error.to_string(),
            },
        }
    }

    pub fn from_artifact_manifest(manifest: &ArtifactManifest) -> Self {
        let Some(artifact) = manifest.artifacts.first() else {
            return Self {
                status: ManualSmokeStatus::Unavailable,
                message: "microphone capture completed without an audio artifact".to_string(),
            };
        };
        Self {
            status: ManualSmokeStatus::Passed,
            message: format!(
                "wrote {}: sample_rate_hz={}, channels={}, device={}, duration_ms={}, sha256={}",
                artifact.path.display(),
                artifact.sample_rate_hz,
                artifact.channel_count,
                artifact.identity.display_name,
                artifact.duration_ms,
                artifact.sha256
            ),
        }
    }
}
