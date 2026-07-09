use curiosity_domain::RawAudioRetentionPolicy;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::{
    default_if_blank, enum_name, is_lower_hex_sha256, parse_raw_audio_retention_policy, Store,
    StoreResult,
};

pub const DEFAULT_OLLAMA_BASE_URL: &str = "http://127.0.0.1:11434";
pub const DEFAULT_OLLAMA_MODEL: &str = "qwen3.6:27b";

const SETTING_WHISPER_MODEL_PATH: &str = "whisper_model_path";
const SETTING_OLLAMA_BASE_URL: &str = "ollama_base_url";
const SETTING_OLLAMA_MODEL: &str = "ollama_model";
const SETTING_EXPORT_DIRECTORY: &str = "export_directory";
const SETTING_RAW_AUDIO_RETENTION_POLICY: &str = "raw_audio_retention_policy";
const SETTING_WHISPER_PATH_TEST_EVIDENCE: &str = "whisper_path_test_evidence";
const SETTING_WHISPER_TRANSCRIPTION_COMPATIBILITY_EVIDENCE: &str =
    "whisper_transcription_compatibility_evidence";
const SETTING_OLLAMA_CONNECTION_TEST_EVIDENCE: &str = "ollama_connection_test_evidence";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppSettings {
    pub whisper_model_path: String,
    pub ollama_base_url: String,
    pub ollama_model: String,
    pub export_directory: Option<String>,
    pub raw_audio_retention_policy: RawAudioRetentionPolicy,
    pub whisper_path_test_evidence: Option<WhisperPathTestEvidence>,
    pub whisper_transcription_compatibility_evidence:
        Option<WhisperTranscriptionCompatibilityEvidence>,
    pub ollama_connection_test_evidence: Option<OllamaConnectionTestEvidence>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhisperPathTestEvidence {
    pub tested_path: String,
    pub tested_at_ms: u64,
    pub state: String,
    pub file_size_bytes: Option<u64>,
    pub sha256: Option<String>,
    pub failure_detail: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhisperTranscriptionCompatibilityEvidence {
    pub model_path: String,
    pub used_at_ms: u64,
    pub provider: String,
    pub model_name: String,
    pub meeting_id: String,
    pub model_run_id: String,
    pub transcript_version_id: String,
    pub segment_count: u64,
    pub file_size_bytes: u64,
    pub modified_at_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OllamaConnectionTestEvidence {
    pub base_url: String,
    pub requested_model: String,
    pub tested_at_ms: u64,
    pub state: String,
    pub selected_local_model_tag: Option<String>,
    pub installed_local_models: Option<Vec<String>>,
    pub pull_command: Option<String>,
    pub failure_detail: Option<String>,
}

impl WhisperPathTestEvidence {
    fn is_valid_snapshot_evidence(&self) -> bool {
        match self.state.as_str() {
            "Valid" => {
                matches!(self.file_size_bytes, Some(size) if size > 0)
                    && self.sha256.as_deref().map(is_lower_hex_sha256) == Some(true)
            }
            "Invalid" => self
                .sha256
                .as_deref()
                .map(is_lower_hex_sha256)
                .unwrap_or(true),
            _ => false,
        }
    }
}

impl WhisperTranscriptionCompatibilityEvidence {
    fn is_valid_snapshot_evidence(&self) -> bool {
        !self.model_path.trim().is_empty()
            && !self.provider.trim().is_empty()
            && !self.model_name.trim().is_empty()
            && !self.meeting_id.trim().is_empty()
            && !self.model_run_id.trim().is_empty()
            && !self.transcript_version_id.trim().is_empty()
            && self.segment_count > 0
            && self.file_size_bytes > 0
    }
}

impl OllamaConnectionTestEvidence {
    fn is_valid_snapshot_evidence(&self) -> bool {
        matches!(self.state.as_str(), "Available" | "Unavailable")
    }
}

impl Store {
    pub fn app_settings(&self) -> StoreResult<AppSettings> {
        let export_directory = self
            .setting_value(SETTING_EXPORT_DIRECTORY)?
            .filter(|value| !value.trim().is_empty());
        Ok(AppSettings {
            whisper_model_path: self
                .setting_value(SETTING_WHISPER_MODEL_PATH)?
                .unwrap_or_default(),
            ollama_base_url: self
                .setting_value(SETTING_OLLAMA_BASE_URL)?
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_OLLAMA_BASE_URL.to_string()),
            ollama_model: self
                .setting_value(SETTING_OLLAMA_MODEL)?
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_OLLAMA_MODEL.to_string()),
            export_directory,
            raw_audio_retention_policy: parse_raw_audio_retention_policy(
                self.setting_value(SETTING_RAW_AUDIO_RETENTION_POLICY)?
                    .filter(|value| !value.trim().is_empty())
                    .as_deref()
                    .unwrap_or("Retain"),
            )?,
            whisper_path_test_evidence: self.optional_setting_json(
                SETTING_WHISPER_PATH_TEST_EVIDENCE,
                WhisperPathTestEvidence::is_valid_snapshot_evidence,
            )?,
            whisper_transcription_compatibility_evidence: self.optional_setting_json(
                SETTING_WHISPER_TRANSCRIPTION_COMPATIBILITY_EVIDENCE,
                WhisperTranscriptionCompatibilityEvidence::is_valid_snapshot_evidence,
            )?,
            ollama_connection_test_evidence: self.optional_setting_json(
                SETTING_OLLAMA_CONNECTION_TEST_EVIDENCE,
                OllamaConnectionTestEvidence::is_valid_snapshot_evidence,
            )?,
        })
    }

    pub fn save_whisper_model_path(&self, whisper_model_path: &str) -> StoreResult<AppSettings> {
        let whisper_model_path = whisper_model_path.trim();
        self.conn.execute_batch("BEGIN IMMEDIATE TRANSACTION")?;
        let result = (|| {
            self.upsert_setting(SETTING_WHISPER_MODEL_PATH, whisper_model_path)?;
            self.clear_setting_when_json::<WhisperPathTestEvidence, _>(
                SETTING_WHISPER_PATH_TEST_EVIDENCE,
                WhisperPathTestEvidence::is_valid_snapshot_evidence,
                |evidence| evidence.tested_path != whisper_model_path,
            )?;
            self.clear_setting_when_json::<WhisperTranscriptionCompatibilityEvidence, _>(
                SETTING_WHISPER_TRANSCRIPTION_COMPATIBILITY_EVIDENCE,
                WhisperTranscriptionCompatibilityEvidence::is_valid_snapshot_evidence,
                |evidence| evidence.model_path != whisper_model_path,
            )?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                if let Err(err) = self.conn.execute_batch("COMMIT") {
                    let _ = self.conn.execute_batch("ROLLBACK");
                    return Err(err.into());
                }
                self.app_settings()
            }
            Err(err) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(err)
            }
        }
    }

    pub fn save_analysis_settings(
        &self,
        ollama_base_url: &str,
        ollama_model: &str,
    ) -> StoreResult<AppSettings> {
        let ollama_base_url = default_if_blank(ollama_base_url, DEFAULT_OLLAMA_BASE_URL);
        let ollama_model = default_if_blank(ollama_model, DEFAULT_OLLAMA_MODEL);
        self.conn.execute_batch("BEGIN IMMEDIATE TRANSACTION")?;
        let result = (|| {
            self.upsert_setting(SETTING_OLLAMA_BASE_URL, ollama_base_url)?;
            self.upsert_setting(SETTING_OLLAMA_MODEL, ollama_model)?;
            self.clear_setting_when_json::<OllamaConnectionTestEvidence, _>(
                SETTING_OLLAMA_CONNECTION_TEST_EVIDENCE,
                OllamaConnectionTestEvidence::is_valid_snapshot_evidence,
                |evidence| {
                    evidence.base_url != ollama_base_url || evidence.requested_model != ollama_model
                },
            )?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                if let Err(err) = self.conn.execute_batch("COMMIT") {
                    let _ = self.conn.execute_batch("ROLLBACK");
                    return Err(err.into());
                }
                self.app_settings()
            }
            Err(err) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(err)
            }
        }
    }

    pub fn save_whisper_path_test_evidence(
        &self,
        evidence: &WhisperPathTestEvidence,
    ) -> StoreResult<AppSettings> {
        if !evidence.is_valid_snapshot_evidence() {
            return Err("invalid Whisper path test evidence".into());
        }
        let value = serde_json::to_string(evidence)?;
        self.upsert_setting(SETTING_WHISPER_PATH_TEST_EVIDENCE, &value)?;
        self.app_settings()
    }

    pub fn save_whisper_transcription_compatibility_evidence(
        &self,
        evidence: &WhisperTranscriptionCompatibilityEvidence,
    ) -> StoreResult<AppSettings> {
        if !evidence.is_valid_snapshot_evidence() {
            return Err("invalid Whisper transcription compatibility evidence".into());
        }
        let value = serde_json::to_string(evidence)?;
        self.upsert_setting(SETTING_WHISPER_TRANSCRIPTION_COMPATIBILITY_EVIDENCE, &value)?;
        self.app_settings()
    }

    pub fn save_ollama_connection_test_evidence(
        &self,
        evidence: &OllamaConnectionTestEvidence,
    ) -> StoreResult<AppSettings> {
        if !evidence.is_valid_snapshot_evidence() {
            return Err("invalid Ollama connection test evidence".into());
        }
        let value = serde_json::to_string(evidence)?;
        self.upsert_setting(SETTING_OLLAMA_CONNECTION_TEST_EVIDENCE, &value)?;
        self.app_settings()
    }

    pub fn save_raw_audio_retention_policy(&self, policy: &str) -> StoreResult<AppSettings> {
        let policy = parse_raw_audio_retention_policy(policy.trim())?;
        self.upsert_setting(SETTING_RAW_AUDIO_RETENTION_POLICY, enum_name(policy))?;
        self.app_settings()
    }

    pub(super) fn clear_whisper_transcription_compatibility_evidence_for_meeting(
        &self,
        meeting_id: &str,
    ) -> StoreResult<()> {
        self.clear_setting_when_json::<WhisperTranscriptionCompatibilityEvidence, _>(
            SETTING_WHISPER_TRANSCRIPTION_COMPATIBILITY_EVIDENCE,
            WhisperTranscriptionCompatibilityEvidence::is_valid_snapshot_evidence,
            |evidence| evidence.meeting_id == meeting_id,
        )
    }

    fn setting_value(&self, key: &str) -> StoreResult<Option<String>> {
        self.conn
            .query_row(
                "SELECT value FROM app_settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    fn optional_setting_json<T, F>(&self, key: &str, is_valid: F) -> StoreResult<Option<T>>
    where
        T: for<'de> Deserialize<'de>,
        F: Fn(&T) -> bool,
    {
        let Some(value) = self.setting_value(key)? else {
            return Ok(None);
        };
        match serde_json::from_str(&value) {
            Ok(value) if is_valid(&value) => Ok(Some(value)),
            Ok(_) => Ok(None),
            Err(_) => Ok(None),
        }
    }

    fn clear_setting_when_json<T, F>(
        &self,
        key: &str,
        is_valid: F,
        should_clear: impl FnOnce(&T) -> bool,
    ) -> StoreResult<()>
    where
        T: for<'de> Deserialize<'de>,
        F: Fn(&T) -> bool,
    {
        let Some(value) = self.setting_value(key)? else {
            return Ok(());
        };
        match serde_json::from_str::<T>(&value) {
            Ok(value) if !is_valid(&value) => self.delete_setting(key)?,
            Ok(value) if should_clear(&value) => self.delete_setting(key)?,
            Ok(_) => {}
            Err(_) => self.delete_setting(key)?,
        }
        Ok(())
    }

    fn upsert_setting(&self, key: &str, value: &str) -> StoreResult<()> {
        self.conn.execute(
            "
            INSERT INTO app_settings (key, value)
            VALUES (?1, ?2)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value
            ",
            params![key, value],
        )?;
        Ok(())
    }

    fn delete_setting(&self, key: &str) -> StoreResult<()> {
        self.conn
            .execute("DELETE FROM app_settings WHERE key = ?1", params![key])?;
        Ok(())
    }
}
