use std::path::{Path, PathBuf};

use crate::file_hashing::sha256_for_readable_file;
use curiosity_store::{
    AppSettings, WhisperPathTestEvidence, WhisperTranscriptionCompatibilityEvidence,
};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ModelStatus {
    kind: String,
    configured_path: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WhisperSetupGuidanceView {
    state: String,
    configured_path: String,
    message: String,
    setup_guidance: String,
    compatibility_note: String,
    last_path_test: Option<WhisperPathTestEvidence>,
    last_successful_transcription: Option<WhisperTranscriptionCompatibilityEvidence>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WhisperModelPathTestView {
    pub(super) state: String,
    pub(super) message: String,
    pub(super) setup_guidance: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) file_size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) sha256: Option<String>,
}

impl WhisperModelPathTestView {
    fn invalid(message: impl Into<String>, setup_guidance: impl Into<String>) -> Self {
        Self {
            state: "Invalid".to_string(),
            message: message.into(),
            setup_guidance: setup_guidance.into(),
            file_size_bytes: None,
            sha256: None,
        }
    }
}

pub(super) fn model_status_from_settings(settings: &AppSettings) -> ModelStatus {
    let configured_path = resolved_whisper_model_path(settings);
    let path = PathBuf::from(configured_path.trim());
    let kind = if configured_path.trim().is_empty() || !path.is_file() {
        "missing"
    } else if !is_supported_whisper_model_file_path(&path) {
        "unsupported"
    } else if whisper_path_test_evidence_proves_current_readiness(
        &configured_path,
        &settings.whisper_path_test_evidence,
    ) {
        "ready"
    } else {
        "untested"
    };
    ModelStatus {
        kind: kind.to_string(),
        configured_path,
    }
}

pub(super) fn whisper_setup_guidance_from_settings(
    settings: &AppSettings,
) -> WhisperSetupGuidanceView {
    let configured_path = resolved_whisper_model_path(settings);
    let last_path_test = matching_whisper_path_test_evidence(settings, &configured_path);
    let last_successful_transcription =
        matching_whisper_transcription_compatibility_evidence(settings, &configured_path);
    if configured_path.trim().is_empty() {
        return WhisperSetupGuidanceView {
            state: "MissingPath".to_string(),
            configured_path,
            message: "No Whisper model path is configured.".to_string(),
            setup_guidance:
                "Enter a local Whisper model path in Settings, save it, then use Test path."
                    .to_string(),
            compatibility_note: "Readability does not prove model compatibility.".to_string(),
            last_path_test,
            last_successful_transcription,
        };
    }

    let path = PathBuf::from(configured_path.trim());
    let unreadable = |message: String, setup_guidance: &str| WhisperSetupGuidanceView {
        state: "UnreadablePath".to_string(),
        configured_path: configured_path.clone(),
        message,
        setup_guidance: setup_guidance.to_string(),
        compatibility_note: "Readability does not prove model compatibility.".to_string(),
        last_path_test: last_path_test.clone(),
        last_successful_transcription: last_successful_transcription.clone(),
    };

    let metadata = match std::fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) => {
            return unreadable(
                format!("Whisper model path does not exist or cannot be inspected: {error}"),
                "Check the saved path, choose a readable local Whisper model file, then use Test path.",
            );
        }
    };
    if !metadata.is_file() {
        return unreadable(
            "Whisper model path must point to a file.".to_string(),
            "Choose a readable local Whisper model file, not a directory, then use Test path.",
        );
    }
    if !is_supported_whisper_model_file_path(&path) {
        return WhisperSetupGuidanceView {
            state: "UnsupportedFile".to_string(),
            configured_path,
            message: "Whisper model path must use a supported .bin or .gguf file.".to_string(),
            setup_guidance:
                "Choose an existing whisper.cpp-compatible .bin or .gguf Whisper model file."
                    .to_string(),
            compatibility_note: "Test path only accepts .bin and .gguf model files.".to_string(),
            last_path_test,
            last_successful_transcription,
        };
    }
    if let Err(error) = std::fs::File::open(&path) {
        return unreadable(
            format!("Whisper model path is not readable: {error}"),
            "Check file permissions, choose a readable local Whisper model file, then use Test path.",
        );
    }

    let (message, compatibility_note) = if last_successful_transcription.is_some() {
        (
            "Whisper model path is readable and has completed transcription before.".to_string(),
            "Last successful transcription is historical evidence for this local path, not a background compatibility check."
                .to_string(),
        )
    } else {
        (
            "Whisper model path is readable; compatibility is not verified.".to_string(),
            "Readability does not prove model compatibility.".to_string(),
        )
    };

    WhisperSetupGuidanceView {
        state: "ReadablePath".to_string(),
        configured_path,
        message,
        setup_guidance:
            "Use Test path for file evidence, then transcribe a sample to verify compatibility."
                .to_string(),
        compatibility_note,
        last_path_test,
        last_successful_transcription,
    }
}

fn matching_whisper_path_test_evidence(
    settings: &AppSettings,
    configured_path: &str,
) -> Option<WhisperPathTestEvidence> {
    let configured_path = configured_path.trim();
    settings
        .whisper_path_test_evidence
        .as_ref()
        .filter(|evidence| evidence.tested_path == configured_path)
        .filter(|evidence| {
            evidence.state != "Valid"
                || whisper_path_test_evidence_matches_current_file(configured_path, evidence)
        })
        .cloned()
}

fn matching_whisper_transcription_compatibility_evidence(
    settings: &AppSettings,
    configured_path: &str,
) -> Option<WhisperTranscriptionCompatibilityEvidence> {
    let configured_path = configured_path.trim();
    if !is_supported_whisper_model_file_path(Path::new(configured_path)) {
        return None;
    }
    settings
        .whisper_transcription_compatibility_evidence
        .as_ref()
        .filter(|evidence| evidence.model_path == configured_path)
        .filter(|evidence| {
            whisper_transcription_compatibility_evidence_matches_current_file(
                configured_path,
                evidence,
            )
        })
        .cloned()
}

pub(super) fn whisper_path_test_evidence_proves_current_readiness(
    configured_path: &str,
    evidence: &Option<WhisperPathTestEvidence>,
) -> bool {
    let configured_path = configured_path.trim();
    evidence
        .as_ref()
        .filter(|evidence| evidence.tested_path == configured_path)
        .filter(|evidence| evidence.state == "Valid")
        .map(|evidence| whisper_path_test_evidence_matches_current_file(configured_path, evidence))
        .unwrap_or(false)
}

fn whisper_transcription_compatibility_evidence_matches_current_file(
    configured_path: &str,
    evidence: &WhisperTranscriptionCompatibilityEvidence,
) -> bool {
    std::fs::metadata(configured_path.trim())
        .map(|metadata| {
            metadata.is_file()
                && metadata.len() == evidence.file_size_bytes
                && file_modified_at_ms(&metadata) == Some(evidence.modified_at_ms)
        })
        .unwrap_or(false)
}

fn whisper_path_test_evidence_matches_current_file(
    configured_path: &str,
    evidence: &WhisperPathTestEvidence,
) -> bool {
    let path = PathBuf::from(configured_path.trim());
    if !is_supported_whisper_model_file_path(&path) {
        return false;
    }
    let Some(expected_size) = evidence.file_size_bytes else {
        return false;
    };
    std::fs::metadata(&path)
        .map(|metadata| metadata.is_file() && metadata.len() == expected_size)
        .unwrap_or(false)
}

pub(super) fn resolved_whisper_model_path(settings: &AppSettings) -> String {
    let saved_path = settings.whisper_model_path.trim();
    if saved_path.is_empty() {
        std::env::var("CURIOSITY_WHISPER_MODEL").unwrap_or_default()
    } else {
        saved_path.to_string()
    }
}

pub(super) fn model_name_for_path(model_path: &str) -> String {
    PathBuf::from(model_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("local-whisper")
        .to_string()
}

pub(super) fn test_whisper_model_path_value(path: &str) -> WhisperModelPathTestView {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return WhisperModelPathTestView::invalid(
            "No Whisper model path is configured.",
            "Enter a local Whisper model path, or set CURIOSITY_WHISPER_MODEL before launching the app.",
        );
    }
    let path = PathBuf::from(trimmed);
    let metadata = match std::fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) => {
            return WhisperModelPathTestView::invalid(
                format!("Whisper model path does not exist or cannot be inspected: {error}"),
                "Check the path and choose a readable local Whisper model file.",
            );
        }
    };
    if !metadata.is_file() {
        return WhisperModelPathTestView::invalid(
            "Whisper model path must point to a file.",
            "Choose a readable local Whisper model file, not a directory.",
        );
    }
    if !is_supported_whisper_model_file_path(&path) {
        return WhisperModelPathTestView::invalid(
            "Whisper model path must use a supported .bin or .gguf file.",
            "Choose an existing whisper.cpp-compatible .bin or .gguf model file, then run Test path.",
        );
    }
    if metadata.len() == 0 {
        return WhisperModelPathTestView::invalid(
            "Whisper model path points to an empty file.",
            "Choose a non-empty whisper.cpp-compatible .bin or .gguf model file, then run Test path.",
        );
    }
    match sha256_for_readable_file(&path) {
        Ok(sha256) => WhisperModelPathTestView {
            state: "Valid".to_string(),
            message: "Whisper model path is readable; compatibility is not verified by this test."
                .to_string(),
            setup_guidance:
                "Record this file size and SHA-256, then run the real Whisper smoke or transcribe a sample to verify compatibility."
                    .to_string(),
            file_size_bytes: Some(metadata.len()),
            sha256: Some(sha256),
        },
        Err(error) => WhisperModelPathTestView::invalid(
            format!("Whisper model path is not readable: {error}"),
            "Check file permissions and choose a readable local Whisper model file.",
        ),
    }
}

pub(super) const SUPPORTED_WHISPER_MODEL_EXTENSIONS: [&str; 2] = ["bin", "gguf"];

pub(super) fn is_supported_whisper_model_file_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            SUPPORTED_WHISPER_MODEL_EXTENSIONS
                .iter()
                .any(|supported_extension| extension.eq_ignore_ascii_case(supported_extension))
        })
        .unwrap_or(false)
}

pub(super) fn file_modified_at_ms(metadata: &std::fs::Metadata) -> Option<u64> {
    u64::try_from(
        metadata
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_millis(),
    )
    .ok()
}
