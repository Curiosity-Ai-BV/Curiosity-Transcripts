use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::StoreResult;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteStatus {
    Writing,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepairStatus {
    NotNeeded,
    Recoverable,
    Recovered,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArtifactManifest {
    pub meeting_id: String,
    #[serde(default)]
    pub meeting_title: Option<String>,
    pub session_id: String,
    pub artifact_id: String,
    pub path: String,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<RecoverableArtifact>,
    pub write_status: WriteStatus,
    pub recovery_status: RepairStatus,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RecoverableArtifact {
    pub artifact_id: String,
    pub path: String,
    pub sha256: String,
}

impl ArtifactManifest {
    pub fn new(
        meeting_id: impl ToString,
        session_id: impl ToString,
        artifact_id: impl ToString,
        path: impl ToString,
        sha256: impl ToString,
    ) -> Self {
        Self {
            meeting_id: meeting_id.to_string(),
            meeting_title: None,
            session_id: session_id.to_string(),
            artifact_id: artifact_id.to_string(),
            path: path.to_string(),
            sha256: sha256.to_string(),
            artifacts: Vec::new(),
            write_status: WriteStatus::Writing,
            recovery_status: RepairStatus::NotNeeded,
        }
    }

    pub fn mark_interrupted_recoverable(mut self) -> Self {
        self.recovery_status = RepairStatus::Recoverable;
        self
    }

    pub fn write(&self, path: impl AsRef<Path>) -> StoreResult<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temp_path = path.with_extension("json.tmp");
        fs::write(&temp_path, serde_json::to_vec_pretty(self)?)?;
        fs::rename(temp_path, path)?;
        Ok(())
    }

    pub fn read(path: impl AsRef<Path>) -> StoreResult<Self> {
        Ok(serde_json::from_slice(&fs::read(path)?)?)
    }
}

pub(crate) fn recoverable_artifact_entries(manifest: &ArtifactManifest) -> Vec<ArtifactManifest> {
    if manifest.artifacts.is_empty() {
        return vec![manifest.clone()];
    }
    manifest
        .artifacts
        .iter()
        .map(|artifact| {
            let mut entry = manifest.clone();
            entry.artifact_id = artifact.artifact_id.clone();
            entry.path = artifact.path.clone();
            entry.sha256 = artifact.sha256.clone();
            entry.artifacts = Vec::new();
            entry
        })
        .collect()
}

pub(crate) fn manifest_paths(root: &Path) -> StoreResult<Vec<PathBuf>> {
    let meetings = root.join("meetings");
    match fs::symlink_metadata(&meetings) {
        Ok(metadata) if metadata.file_type().is_dir() => {}
        Ok(_) => return Ok(Vec::new()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err.into()),
    }
    let mut paths = Vec::new();
    for entry in fs::read_dir(meetings)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path().join("manifest.json");
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_file() => paths.push(path),
            Ok(_) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }
    }
    Ok(paths)
}

pub(crate) fn manifest_update_temp_path(path: &Path) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    path.with_extension(format!("json.rename-tmp-{}-{nonce}", std::process::id(),))
}
