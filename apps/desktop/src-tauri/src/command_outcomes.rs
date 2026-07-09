use std::path::{Path, PathBuf};

use curiosity_app::{DeletedMeetingDto, ExportFormat, ExportedMeetingDto};
use curiosity_store::{AppSettings, PendingDeleteFinalizationReport};
use serde::Serialize;

pub(crate) fn export_root_for_settings(app_root: &Path, settings: &AppSettings) -> PathBuf {
    settings
        .export_directory
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| app_root.join("exports"))
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExportCommandState {
    state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) meeting_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<ExportFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

impl Default for ExportCommandState {
    fn default() -> Self {
        Self {
            state: "idle".to_string(),
            meeting_id: None,
            format: None,
            path: None,
            message: None,
        }
    }
}

impl ExportCommandState {
    pub(crate) fn exported(exported: ExportedMeetingDto) -> Self {
        Self {
            state: "exported".to_string(),
            meeting_id: Some(exported.meeting_id),
            format: Some(exported.format),
            path: Some(exported.path),
            message: None,
        }
    }

    pub(crate) fn failed(meeting_id: &str, format: ExportFormat, message: String) -> Self {
        Self {
            state: "failed".to_string(),
            meeting_id: Some(meeting_id.to_string()),
            format: Some(format),
            path: None,
            message: Some(message),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeleteCommandState {
    state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) meeting_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    deleted_private_artifacts: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    skipped_private_artifacts: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    remaining_exports: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

impl Default for DeleteCommandState {
    fn default() -> Self {
        Self {
            state: "idle".to_string(),
            meeting_id: None,
            deleted_private_artifacts: Vec::new(),
            skipped_private_artifacts: Vec::new(),
            remaining_exports: Vec::new(),
            message: None,
        }
    }
}

impl DeleteCommandState {
    pub(crate) fn deleted(deleted: DeletedMeetingDto) -> Self {
        Self {
            state: "deleted".to_string(),
            meeting_id: Some(deleted.meeting_id),
            deleted_private_artifacts: deleted.deleted_private_artifacts,
            skipped_private_artifacts: deleted.skipped_private_artifacts,
            remaining_exports: deleted.remaining_exports,
            message: None,
        }
    }

    pub(crate) fn failed(meeting_id: &str, message: String) -> Self {
        Self {
            state: "failed".to_string(),
            meeting_id: Some(meeting_id.to_string()),
            deleted_private_artifacts: Vec::new(),
            skipped_private_artifacts: Vec::new(),
            remaining_exports: Vec::new(),
            message: Some(message),
        }
    }
}

pub(crate) fn delete_command_state_from_pending_finalization(
    report: PendingDeleteFinalizationReport,
) -> DeleteCommandState {
    DeleteCommandState::deleted(DeletedMeetingDto {
        meeting_id: report.meeting_id,
        deleted_private_artifacts: paths_to_strings(report.deleted_private_artifacts),
        skipped_private_artifacts: paths_to_strings(report.skipped_private_artifacts),
        remaining_exports: paths_to_strings(report.exported_files_outside_app_control),
    })
}

fn paths_to_strings(paths: Vec<PathBuf>) -> Vec<String> {
    paths
        .into_iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect()
}
