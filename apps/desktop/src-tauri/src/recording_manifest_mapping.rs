use std::path::{Component, Path};

use curiosity_audio::{ArtifactManifest, StreamKind};
use curiosity_store::CompletedAudioArtifact;

use crate::recording_artifact_paths::{artifact_id_for_stream, artifact_relative_path_for_stream};
use crate::recording_streams::stream_label;

#[derive(Debug, Eq, PartialEq)]
pub(super) struct CompletedAudioManifestMapping {
    pub(super) completed_artifacts: Vec<CompletedAudioArtifact>,
    pub(super) completed_streams: Vec<StreamKind>,
}

pub(super) fn completed_audio_artifacts_from_manifest(
    app_root: &Path,
    meeting_id: &str,
    recording_id: &str,
    streams: &[StreamKind],
    manifest: &ArtifactManifest,
) -> Result<CompletedAudioManifestMapping, String> {
    let mut completed_artifacts = Vec::new();
    let mut completed_streams = Vec::new();
    for artifact in &manifest.artifacts {
        if !streams.contains(&artifact.stream) {
            return Err(format!(
                "{} artifact was not part of the active recording",
                stream_label(artifact.stream)
            ));
        }
        let relative_path =
            relative_private_artifact_path(app_root, &artifact.path, artifact.stream)?;
        let expected_path =
            artifact_relative_path_for_stream(meeting_id, recording_id, artifact.stream);
        if relative_path != expected_path {
            return Err(format!(
                "{} artifact path mismatch: expected {expected_path}, got {relative_path}",
                stream_label(artifact.stream)
            ));
        }
        completed_streams.push(artifact.stream);
        completed_artifacts.push(CompletedAudioArtifact {
            artifact_id: artifact_id_for_stream(recording_id, artifact.stream),
            sha256: artifact.sha256.clone(),
        });
    }
    Ok(CompletedAudioManifestMapping {
        completed_artifacts,
        completed_streams,
    })
}

fn relative_private_artifact_path(
    app_root: &Path,
    path: &Path,
    stream: StreamKind,
) -> Result<String, String> {
    let relative_path = path.strip_prefix(app_root).map_err(|_| {
        format!(
            "{} artifact was written outside private app storage",
            stream_label(stream)
        )
    })?;
    if relative_path
        .components()
        .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err(format!(
            "{} artifact was written outside private app storage",
            stream_label(stream)
        ));
    }
    Ok(relative_path.to_string_lossy().to_string())
}
