//! SQLite-backed persistence for local transcript data.
//!
//! This crate owns migrations, search, export/delete, startup repair, settings,
//! and persisted analysis records. It should not own desktop DTOs, capture
//! devices, transcription engines, or provider calls.

use std::error::Error;
use std::fmt;
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use curiosity_domain::{
    ArtifactKind, AudioArtifact, JobKind, JobStatus, Meeting, MeetingAnalysis, MeetingStatus,
    ModelRun, ProcessingJob, RecordingSession, RecordingSource, RecordingStatus, SourceChannel,
    TranscriptSegment, TranscriptState, TranscriptVersion,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Result type for operations at the persistence boundary.
pub type StoreResult<T> = Result<T, StoreError>;

pub const DEFAULT_OLLAMA_BASE_URL: &str = "http://127.0.0.1:11434";
pub const DEFAULT_OLLAMA_MODEL: &str = "qwen3.6:27b";

const SCHEMA_VERSION: u32 = 3;
const PENDING_SHA_PREFIX: &str = "sha256:pending";
const SETTING_WHISPER_MODEL_PATH: &str = "whisper_model_path";
const SETTING_OLLAMA_BASE_URL: &str = "ollama_base_url";
const SETTING_OLLAMA_MODEL: &str = "ollama_model";
const SETTING_EXPORT_DIRECTORY: &str = "export_directory";

/// Typed store failure for storage, path safety, recovery, and invariant errors.
#[derive(Debug)]
pub enum StoreError {
    Io(io::Error),
    Sqlite(rusqlite::Error),
    Serde(serde_json::Error),
    ReplayConflict(String),
    UnsafePath(String),
    NotFound(String),
    RepairConflict(String),
    InvariantViolation(String),
    Message(String),
}

impl StoreError {
    fn from_message(message: String) -> Self {
        if message.contains("replay conflict") {
            Self::ReplayConflict(message)
        } else if message.contains("repair conflict") {
            Self::RepairConflict(message)
        } else if message.contains("not safe")
            || message.contains("unsafe")
            || message.contains("outside app")
            || message.contains("escape app")
        {
            Self::UnsafePath(message)
        } else if message.contains("not found") {
            Self::NotFound(message)
        } else if message.contains("requires")
            || message.contains("unexpected")
            || message.contains("unknown ")
            || message.contains("unsupported")
        {
            Self::InvariantViolation(message)
        } else {
            Self::Message(message)
        }
    }
}

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(formatter, "{err}"),
            Self::Sqlite(err) => write!(formatter, "{err}"),
            Self::Serde(err) => write!(formatter, "{err}"),
            Self::ReplayConflict(message)
            | Self::UnsafePath(message)
            | Self::NotFound(message)
            | Self::RepairConflict(message)
            | Self::InvariantViolation(message)
            | Self::Message(message) => formatter.write_str(message),
        }
    }
}

impl Error for StoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Sqlite(err) => Some(err),
            Self::Serde(err) => Some(err),
            Self::ReplayConflict(_)
            | Self::UnsafePath(_)
            | Self::NotFound(_)
            | Self::RepairConflict(_)
            | Self::InvariantViolation(_)
            | Self::Message(_) => None,
        }
    }
}

impl From<io::Error> for StoreError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(err: rusqlite::Error) -> Self {
        Self::Sqlite(err)
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serde(err)
    }
}

impl From<String> for StoreError {
    fn from(message: String) -> Self {
        Self::from_message(message)
    }
}

impl From<&str> for StoreError {
    fn from(message: &str) -> Self {
        Self::from_message(message.to_string())
    }
}

/// SQLite-backed local store.
///
/// `rusqlite::Connection` is single-connection state, so callers that use a
/// store from multiple threads should wrap a `Store` in a mutex or open one
/// `Store` per worker/task.
pub struct Store {
    conn: Connection,
    app_root: PathBuf,
    canonical_app_root: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppSettings {
    pub whisper_model_path: String,
    pub ollama_base_url: String,
    pub ollama_model: String,
    pub export_directory: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MeetingSummary {
    pub meeting_id: String,
    pub title: String,
    pub started_at_ms: u64,
    pub ended_at_ms: Option<u64>,
    pub status: String,
    pub transcript_state: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MeetingSearchResult {
    pub meeting_id: String,
    pub title: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TranscriptionAudioArtifact {
    pub artifact_id: String,
    pub recording_session_id: String,
    pub kind: String,
    pub path: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletedAudioArtifact {
    pub artifact_id: String,
    pub sha256: String,
}

impl Store {
    pub fn open(db_path: impl AsRef<Path>, app_root: impl Into<PathBuf>) -> StoreResult<Self> {
        let db_path = db_path.as_ref();
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let app_root = app_root.into();
        fs::create_dir_all(&app_root)?;
        let canonical_app_root = app_root.canonicalize()?;
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA busy_timeout = 5000;
            ",
        )?;
        Ok(Self {
            conn,
            app_root,
            canonical_app_root,
        })
    }

    pub fn migrate(&self) -> StoreResult<()> {
        let _existing_version = self.schema_version()?;
        // v0-v2 migrations below are idempotent. Branch on this value when
        // adding non-idempotent v3+ migrations.
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS meetings (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                started_at_ms INTEGER NOT NULL,
                ended_at_ms INTEGER,
                deleted_at_ms INTEGER,
                status TEXT NOT NULL,
                transcript_state TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS recording_sessions (
                id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                source TEXT NOT NULL,
                started_at_ms INTEGER NOT NULL,
                ended_at_ms INTEGER,
                sample_rate_hz INTEGER NOT NULL,
                status TEXT NOT NULL,
                recovery_note TEXT
            );

            CREATE TABLE IF NOT EXISTS audio_artifacts (
                id TEXT PRIMARY KEY,
                recording_session_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                path TEXT NOT NULL,
                sha256 TEXT NOT NULL,
                retained INTEGER NOT NULL,
                write_status TEXT NOT NULL DEFAULT 'Writing',
                recovery_status TEXT NOT NULL DEFAULT 'NotNeeded',
                tombstoned INTEGER NOT NULL DEFAULT 0
            );

            CREATE UNIQUE INDEX IF NOT EXISTS audio_artifacts_import_identity_idx
            ON audio_artifacts(recording_session_id, kind, sha256);

            CREATE TABLE IF NOT EXISTS processing_jobs (
                id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                status TEXT NOT NULL,
                attempts INTEGER NOT NULL,
                last_error TEXT,
                started_at_ms INTEGER,
                finished_at_ms INTEGER,
                cancel_requested INTEGER NOT NULL DEFAULT 0,
                idempotency_key TEXT
            );

            CREATE TABLE IF NOT EXISTS exported_files (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                meeting_id TEXT NOT NULL,
                path TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS model_runs (
                id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                source_artifact_sha256 TEXT NOT NULL,
                provider TEXT NOT NULL,
                model_name TEXT NOT NULL,
                network_used INTEGER NOT NULL,
                created_at_ms INTEGER NOT NULL,
                UNIQUE(meeting_id, source_artifact_sha256, provider, model_name)
            );

            CREATE TABLE IF NOT EXISTS transcript_versions (
                id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                model_run_id TEXT NOT NULL,
                version INTEGER NOT NULL,
                created_at_ms INTEGER NOT NULL,
                edited_at_ms INTEGER,
                UNIQUE(meeting_id, model_run_id, version)
            );

            CREATE TABLE IF NOT EXISTS transcript_segments (
                id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                transcript_version_id TEXT NOT NULL,
                model_run_id TEXT NOT NULL,
                ordinal INTEGER NOT NULL,
                start_ms INTEGER NOT NULL,
                end_ms INTEGER NOT NULL,
                text TEXT NOT NULL,
                original_text TEXT,
                source_channel TEXT NOT NULL,
                UNIQUE(transcript_version_id, ordinal)
            );

            CREATE TABLE IF NOT EXISTS transcript_segment_edits (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                segment_id TEXT NOT NULL,
                transcript_version_id TEXT NOT NULL,
                edited_at_ms INTEGER NOT NULL,
                previous_text TEXT NOT NULL,
                corrected_text TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS analysis_results (
                id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                provider TEXT NOT NULL,
                model_name TEXT NOT NULL,
                network_used INTEGER NOT NULL,
                created_at_ms INTEGER NOT NULL,
                prompt_template_version TEXT NOT NULL,
                result_json TEXT NOT NULL,
                UNIQUE(meeting_id, provider, model_name, prompt_template_version)
            );

            CREATE TABLE IF NOT EXISTS app_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS meeting_search
            USING fts5(meeting_id UNINDEXED, title, transcript_text);
            ",
        )?;
        self.ensure_column(
            "audio_artifacts",
            "write_status",
            "TEXT NOT NULL DEFAULT 'Writing'",
        )?;
        self.ensure_column(
            "audio_artifacts",
            "recovery_status",
            "TEXT NOT NULL DEFAULT 'NotNeeded'",
        )?;
        self.ensure_column(
            "audio_artifacts",
            "tombstoned",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        self.ensure_column("processing_jobs", "started_at_ms", "INTEGER")?;
        self.ensure_column("processing_jobs", "finished_at_ms", "INTEGER")?;
        self.ensure_column(
            "processing_jobs",
            "cancel_requested",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        self.ensure_column("processing_jobs", "idempotency_key", "TEXT")?;
        self.conn
            .execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))?;
        Ok(())
    }

    pub fn schema_version(&self) -> StoreResult<u32> {
        Ok(self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?)
    }

    fn ensure_column(&self, table: &str, column: &str, definition: &str) -> StoreResult<()> {
        if self.table_has_column(table, column)? {
            return Ok(());
        }
        let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {definition}");
        self.conn.execute_batch(&sql)?;
        Ok(())
    }

    fn table_has_column(&self, table: &str, column: &str) -> StoreResult<bool> {
        let sql = format!("PRAGMA table_info({table})");
        let mut stmt = self.conn.prepare(&sql)?;
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(columns.iter().any(|name| name == column))
    }

    pub fn insert_meeting(&self, meeting: &Meeting) -> StoreResult<()> {
        self.conn.execute(
            "
            INSERT INTO meetings (
                id, title, started_at_ms, ended_at_ms, deleted_at_ms, status, transcript_state
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ",
            params![
                meeting.id,
                meeting.title,
                meeting.started_at_ms,
                meeting.ended_at_ms,
                meeting.deleted_at_ms,
                enum_name(meeting.status),
                enum_name(meeting.transcript_state)
            ],
        )?;
        self.refresh_search_index_for_meeting(&meeting.id)?;
        Ok(())
    }

    pub fn insert_recording_session(&self, session: &RecordingSession) -> StoreResult<()> {
        self.conn.execute(
            "
            INSERT INTO recording_sessions (
                id, meeting_id, source, started_at_ms, ended_at_ms, sample_rate_hz, status, recovery_note
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ",
            params![
                session.id,
                session.meeting_id,
                enum_name(session.source),
                session.started_at_ms,
                session.ended_at_ms,
                session.sample_rate_hz,
                enum_name(session.status),
                session.recovery_note
            ],
        )?;
        Ok(())
    }

    pub fn insert_recording_start(
        &self,
        meeting: &Meeting,
        session: &RecordingSession,
        artifact: &AudioArtifact,
    ) -> StoreResult<()> {
        self.insert_recording_start_with_artifacts(meeting, session, std::slice::from_ref(artifact))
    }

    pub fn insert_recording_start_with_artifacts(
        &self,
        meeting: &Meeting,
        session: &RecordingSession,
        artifacts: &[AudioArtifact],
    ) -> StoreResult<()> {
        if artifacts.is_empty() {
            return Err("recording start requires at least one audio artifact".into());
        }
        self.conn.execute_batch("BEGIN IMMEDIATE TRANSACTION")?;
        let result = (|| {
            self.insert_meeting(meeting)?;
            self.insert_recording_session(session)?;
            for artifact in artifacts {
                let inserted_artifact_id = self.insert_audio_artifact(artifact)?;
                if inserted_artifact_id != artifact.id {
                    return Err(format!(
                        "recording start reused unexpected audio artifact: {inserted_artifact_id}"
                    )
                    .into());
                }
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                if let Err(err) = self.conn.execute_batch("COMMIT") {
                    let _ = self.conn.execute_batch("ROLLBACK");
                    return Err(err.into());
                }
                Ok(())
            }
            Err(err) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(err)
            }
        }
    }

    pub fn insert_audio_artifact(&self, artifact: &AudioArtifact) -> StoreResult<String> {
        let kind = enum_name(artifact.kind);
        if !is_pending_sha256(&artifact.sha256) {
            if let Some(existing_id) = self.audio_artifact_id_by_import_identity(
                &artifact.recording_session_id,
                kind,
                &artifact.sha256,
            )? {
                return Ok(existing_id);
            }
        }
        self.conn.execute(
            "
            INSERT INTO audio_artifacts (
                id, recording_session_id, kind, path, sha256, retained
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ",
            params![
                artifact.id,
                artifact.recording_session_id,
                kind,
                artifact.path,
                artifact.sha256,
                artifact.retained
            ],
        )?;
        Ok(artifact.id.clone())
    }

    pub fn insert_processing_job(&self, job: &ProcessingJob) -> StoreResult<()> {
        self.conn.execute(
            "
            INSERT INTO processing_jobs (
                id, meeting_id, kind, status, attempts, last_error,
                started_at_ms, finished_at_ms, cancel_requested, idempotency_key
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ",
            params![
                job.id,
                job.meeting_id,
                enum_name(job.kind),
                enum_name(job.status),
                job.attempts,
                job.last_error,
                job.started_at_ms,
                job.finished_at_ms,
                job.cancel_requested,
                job.idempotency_key
            ],
        )?;
        Ok(())
    }

    pub fn request_processing_job_cancel(&self, job_id: &str) -> StoreResult<()> {
        self.conn.execute(
            "
            UPDATE processing_jobs
            SET cancel_requested = 1
            WHERE id = ?1
              AND status = 'Running'
            ",
            params![job_id],
        )?;
        if self.conn.changes() == 0 {
            let status = self
                .conn
                .query_row(
                    "SELECT status FROM processing_jobs WHERE id = ?1",
                    params![job_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            let Some(status) = status else {
                return Err(format!("processing job not found: {job_id}").into());
            };
            let status = parse_job_status(&status)?;
            if status == JobStatus::Running {
                return Ok(());
            }
            return Err(format!("processing job is not running: {job_id}").into());
        }
        Ok(())
    }

    pub fn complete_processing_job(&self, job_id: &str, finished_at_ms: u64) -> StoreResult<()> {
        self.finish_processing_job(job_id, JobStatus::Succeeded, finished_at_ms, None)
    }

    pub fn cancel_processing_job(&self, job_id: &str, finished_at_ms: u64) -> StoreResult<()> {
        self.finish_processing_job(job_id, JobStatus::Canceled, finished_at_ms, None)
    }

    pub fn fail_processing_job(
        &self,
        job_id: &str,
        finished_at_ms: u64,
        last_error: &str,
    ) -> StoreResult<()> {
        self.finish_processing_job(job_id, JobStatus::Failed, finished_at_ms, Some(last_error))
    }

    pub fn recover_processing_job(
        &self,
        job_id: &str,
        finished_at_ms: u64,
        last_error: &str,
    ) -> StoreResult<()> {
        self.finish_processing_job(
            job_id,
            JobStatus::Recovery,
            finished_at_ms,
            Some(last_error),
        )
    }

    fn finish_processing_job(
        &self,
        job_id: &str,
        status: JobStatus,
        finished_at_ms: u64,
        last_error: Option<&str>,
    ) -> StoreResult<()> {
        self.conn.execute(
            "
            UPDATE processing_jobs
            SET status = ?2,
                finished_at_ms = ?3,
                last_error = ?4,
                cancel_requested = 0
            WHERE id = ?1
              AND status = 'Running'
            ",
            params![job_id, enum_name(status), finished_at_ms, last_error],
        )?;
        if self.conn.changes() == 0 {
            let current_status = self
                .conn
                .query_row(
                    "SELECT status FROM processing_jobs WHERE id = ?1",
                    params![job_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            let Some(current_status) = current_status else {
                return Err(format!("processing job not found: {job_id}").into());
            };
            let current_status = parse_job_status(&current_status)?;
            if current_status == JobStatus::Running {
                return Err(format!("processing job was not updated: {job_id}").into());
            }
            return Err(format!("processing job is not running: {job_id}").into());
        }
        Ok(())
    }

    pub fn active_transcription_job_for_meeting(
        &self,
        meeting_id: &str,
    ) -> StoreResult<Option<ProcessingJob>> {
        self.active_processing_job_for_meeting(meeting_id, JobKind::Transcribe)
    }

    pub fn active_summary_job_for_meeting(
        &self,
        meeting_id: &str,
    ) -> StoreResult<Option<ProcessingJob>> {
        self.active_processing_job_for_meeting(meeting_id, JobKind::Summarize)
    }

    fn active_processing_job_for_meeting(
        &self,
        meeting_id: &str,
        kind: JobKind,
    ) -> StoreResult<Option<ProcessingJob>> {
        self.processing_job_for_query(
            "
            SELECT
                id,
                meeting_id,
                kind,
                status,
                attempts,
                last_error,
                started_at_ms,
                finished_at_ms,
                cancel_requested,
                idempotency_key
            FROM processing_jobs
            WHERE meeting_id = ?1
              AND kind = ?2
              AND status = 'Running'
            ORDER BY started_at_ms DESC, id DESC
            LIMIT 1
            ",
            params![meeting_id, enum_name(kind)],
        )
    }

    pub fn recover_active_transcription_jobs(
        &self,
        finished_at_ms: u64,
        last_error: &str,
    ) -> StoreResult<Vec<ProcessingJob>> {
        self.recover_active_processing_jobs(JobKind::Transcribe, finished_at_ms, last_error)
    }

    pub fn recover_active_summary_jobs(
        &self,
        finished_at_ms: u64,
        last_error: &str,
    ) -> StoreResult<Vec<ProcessingJob>> {
        self.recover_active_processing_jobs(JobKind::Summarize, finished_at_ms, last_error)
    }

    fn recover_active_processing_jobs(
        &self,
        kind: JobKind,
        finished_at_ms: u64,
        last_error: &str,
    ) -> StoreResult<Vec<ProcessingJob>> {
        self.conn.execute_batch("BEGIN IMMEDIATE TRANSACTION")?;
        let result =
            self.recover_active_processing_jobs_in_transaction(kind, finished_at_ms, last_error);
        match result {
            Ok(jobs) => {
                if let Err(err) = self.conn.execute_batch("COMMIT") {
                    let _ = self.conn.execute_batch("ROLLBACK");
                    return Err(err.into());
                }
                Ok(jobs)
            }
            Err(err) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(err)
            }
        }
    }

    fn recover_active_processing_jobs_in_transaction(
        &self,
        kind: JobKind,
        finished_at_ms: u64,
        last_error: &str,
    ) -> StoreResult<Vec<ProcessingJob>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT processing_jobs.id
            FROM processing_jobs
            JOIN meetings
              ON meetings.id = processing_jobs.meeting_id
            WHERE processing_jobs.kind = ?1
              AND processing_jobs.status = 'Running'
              AND meetings.status != 'Deleted'
              AND meetings.deleted_at_ms IS NULL
            ORDER BY processing_jobs.started_at_ms DESC, processing_jobs.id DESC
            ",
        )?;
        let job_ids = stmt
            .query_map(params![enum_name(kind)], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        let mut jobs = Vec::with_capacity(job_ids.len());
        for job_id in job_ids {
            self.recover_processing_job(&job_id, finished_at_ms, last_error)?;
            jobs.push(self.processing_job(&job_id)?);
        }
        Ok(jobs)
    }

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
        })
    }

    pub fn save_whisper_model_path(&self, whisper_model_path: &str) -> StoreResult<AppSettings> {
        self.upsert_setting(SETTING_WHISPER_MODEL_PATH, whisper_model_path.trim())?;
        self.app_settings()
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

    pub fn count(&self, table: &str) -> StoreResult<u64> {
        let allowed = [
            "meetings",
            "recording_sessions",
            "audio_artifacts",
            "processing_jobs",
            "exported_files",
            "model_runs",
            "transcript_versions",
            "transcript_segments",
            "transcript_segment_edits",
            "analysis_results",
            "app_settings",
        ];
        if !allowed.contains(&table) {
            return Err(format!("unsupported count table: {table}").into());
        }
        let sql = format!("SELECT COUNT(*) FROM {table}");
        Ok(self.conn.query_row(&sql, [], |row| row.get(0))?)
    }

    pub fn update_meeting_status(
        &self,
        meeting_id: &str,
        status: MeetingStatus,
        ended_at_ms: Option<u64>,
    ) -> StoreResult<()> {
        self.conn.execute(
            "
            UPDATE meetings
            SET status = ?2,
                ended_at_ms = COALESCE(?3, ended_at_ms)
            WHERE id = ?1
            ",
            params![meeting_id, enum_name(status), ended_at_ms],
        )?;
        Ok(())
    }

    pub fn update_recording_session_status(
        &self,
        recording_id: &str,
        status: RecordingStatus,
        ended_at_ms: Option<u64>,
        recovery_note: Option<&str>,
    ) -> StoreResult<()> {
        self.conn.execute(
            "
            UPDATE recording_sessions
            SET status = ?2,
                ended_at_ms = COALESCE(?3, ended_at_ms),
                recovery_note = COALESCE(?4, recovery_note)
            WHERE id = ?1
            ",
            params![recording_id, enum_name(status), ended_at_ms, recovery_note],
        )?;
        Ok(())
    }

    pub fn complete_audio_artifact(&self, artifact_id: &str, sha256: &str) -> StoreResult<()> {
        if is_pending_sha256(sha256) {
            return Err("completed audio artifacts require a final sha256".into());
        }
        self.conn.execute(
            "
            UPDATE audio_artifacts
            SET sha256 = ?2,
                write_status = 'Complete',
                recovery_status = 'NotNeeded'
            WHERE id = ?1
            ",
            params![artifact_id, sha256],
        )?;
        if self.conn.changes() == 0 {
            return Err(format!("audio artifact not found: {artifact_id}").into());
        }
        Ok(())
    }

    pub fn complete_recording_session_with_artifacts(
        &self,
        meeting_id: &str,
        recording_id: &str,
        ended_at_ms: u64,
        recording_source: RecordingSource,
        artifacts: &[CompletedAudioArtifact],
    ) -> StoreResult<()> {
        if artifacts.is_empty() {
            return Err("completed recording requires at least one audio artifact".into());
        }
        for artifact in artifacts {
            if is_pending_sha256(&artifact.sha256) {
                return Err("completed audio artifacts require a final sha256".into());
            }
        }

        self.conn.execute_batch("BEGIN IMMEDIATE TRANSACTION")?;
        let result = self.complete_recording_session_with_artifacts_in_transaction(
            meeting_id,
            recording_id,
            ended_at_ms,
            recording_source,
            artifacts,
        );
        match result {
            Ok(()) => {
                if let Err(err) = self.conn.execute_batch("COMMIT") {
                    let _ = self.conn.execute_batch("ROLLBACK");
                    return Err(err.into());
                }
            }
            Err(err) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                return Err(err);
            }
        }
        Ok(())
    }

    fn complete_recording_session_with_artifacts_in_transaction(
        &self,
        meeting_id: &str,
        recording_id: &str,
        ended_at_ms: u64,
        recording_source: RecordingSource,
        artifacts: &[CompletedAudioArtifact],
    ) -> StoreResult<()> {
        for artifact in artifacts {
            self.conn.execute(
                "
                UPDATE audio_artifacts
                SET sha256 = ?3,
                    write_status = 'Complete',
                    recovery_status = 'NotNeeded'
                WHERE id = ?1 AND recording_session_id = ?2
                ",
                params![artifact.artifact_id, recording_id, artifact.sha256],
            )?;
            if self.conn.changes() == 0 {
                return Err(format!(
                    "audio artifact not found for recording {recording_id}: {}",
                    artifact.artifact_id
                )
                .into());
            }
        }

        self.conn.execute(
            "
            UPDATE recording_sessions
            SET status = 'Complete',
                ended_at_ms = ?3,
                source = ?4,
                recovery_note = NULL
            WHERE id = ?1 AND meeting_id = ?2
            ",
            params![
                recording_id,
                meeting_id,
                ended_at_ms,
                enum_name(recording_source)
            ],
        )?;
        if self.conn.changes() == 0 {
            return Err(format!("recording session not found: {recording_id}").into());
        }

        self.conn.execute(
            "
            UPDATE meetings
            SET status = 'Complete',
                ended_at_ms = ?2
            WHERE id = ?1
            ",
            params![meeting_id, ended_at_ms],
        )?;
        if self.conn.changes() == 0 {
            return Err(format!("meeting not found: {meeting_id}").into());
        }
        Ok(())
    }

    pub fn tombstone_audio_artifact(&self, artifact_id: &str) -> StoreResult<()> {
        self.conn.execute(
            "
            UPDATE audio_artifacts
            SET retained = 0,
                tombstoned = 1
            WHERE id = ?1
            ",
            params![artifact_id],
        )?;
        if self.conn.changes() == 0 {
            return Err(format!("audio artifact not found: {artifact_id}").into());
        }
        Ok(())
    }

    pub fn completed_wav_artifact_for_transcription(
        &self,
        meeting_id: &str,
    ) -> StoreResult<Option<TranscriptionAudioArtifact>> {
        Ok(self
            .completed_wav_artifacts_for_transcription(meeting_id)?
            .into_iter()
            .next())
    }

    pub fn completed_wav_artifacts_for_transcription(
        &self,
        meeting_id: &str,
    ) -> StoreResult<Vec<TranscriptionAudioArtifact>> {
        let meeting_path_prefix = format!("meetings/{meeting_id}/");
        let mut stmt = self.conn.prepare(
            "
            SELECT
                audio_artifacts.id,
                audio_artifacts.recording_session_id,
                audio_artifacts.kind,
                audio_artifacts.path,
                audio_artifacts.sha256,
                recording_sessions.source
            FROM audio_artifacts
            JOIN recording_sessions
              ON recording_sessions.id = audio_artifacts.recording_session_id
            WHERE recording_sessions.meeting_id = ?1
              AND audio_artifacts.retained = 1
              AND audio_artifacts.write_status = 'Complete'
              AND audio_artifacts.tombstoned = 0
              AND recording_sessions.status IN ('Complete', 'Recovered')
              AND lower(audio_artifacts.path) LIKE '%.wav'
            ORDER BY recording_sessions.started_at_ms DESC,
                     audio_artifacts.id ASC
            ",
        )?;
        let artifacts = stmt
            .query_map(params![meeting_id], |row| {
                Ok((
                    TranscriptionAudioArtifact {
                        artifact_id: row.get(0)?,
                        recording_session_id: row.get(1)?,
                        kind: row.get(2)?,
                        path: row.get(3)?,
                        sha256: row.get(4)?,
                    },
                    row.get::<_, String>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut artifacts = artifacts
            .into_iter()
            .filter(|(artifact, _source)| {
                artifact.path.starts_with(&meeting_path_prefix)
                    && self.private_app_path(&artifact.path).is_some()
            })
            .collect::<Vec<_>>();
        let Some((first_artifact, recording_source)) = artifacts.first() else {
            return Ok(Vec::new());
        };
        let recording_session_id = first_artifact.recording_session_id.clone();
        let recording_source = recording_source.clone();
        artifacts
            .retain(|(artifact, _source)| artifact.recording_session_id == recording_session_id);
        let mut artifacts = artifacts
            .into_iter()
            .map(|(artifact, _source)| artifact)
            .collect::<Vec<_>>();
        if !transcription_artifacts_satisfy_recording_source(&recording_source, &artifacts) {
            return Ok(Vec::new());
        }
        artifacts.sort_by_key(|artifact| {
            (
                transcription_artifact_kind_rank(&artifact.kind),
                artifact.artifact_id.clone(),
            )
        });
        Ok(artifacts)
    }

    pub fn meeting_status(&self, meeting_id: &str) -> StoreResult<String> {
        Ok(self.conn.query_row(
            "SELECT status FROM meetings WHERE id = ?1",
            params![meeting_id],
            |row| row.get(0),
        )?)
    }

    pub fn recording_session_status(&self, recording_id: &str) -> StoreResult<String> {
        Ok(self.conn.query_row(
            "SELECT status FROM recording_sessions WHERE id = ?1",
            params![recording_id],
            |row| row.get(0),
        )?)
    }

    pub fn recording_session_ended_at_ms(&self, recording_id: &str) -> StoreResult<Option<u64>> {
        Ok(self.conn.query_row(
            "SELECT ended_at_ms FROM recording_sessions WHERE id = ?1",
            params![recording_id],
            |row| row.get(0),
        )?)
    }

    pub fn repair_startup(&self) -> StoreResult<RepairReport> {
        let mut report = RepairReport::default();
        for manifest_path in manifest_paths(&self.app_root)? {
            let manifest = ArtifactManifest::read(&manifest_path)?;
            if manifest.recovery_status != RepairStatus::Recoverable {
                continue;
            }
            let mut manifest_had_conflict = false;
            let entries = recoverable_artifact_entries(&manifest);
            let mut recovery_plan = Vec::new();
            for entry in &entries {
                let Some(db_artifact) = self.db_artifact_for_repair(&entry.artifact_id)? else {
                    report.conflicts.push(RepairConflict::MissingArtifact {
                        artifact_id: entry.artifact_id.clone(),
                    });
                    manifest_had_conflict = true;
                    continue;
                };
                if let Some(conflict) = repair_conflict(entry, &db_artifact) {
                    report.conflicts.push(conflict);
                    manifest_had_conflict = true;
                    continue;
                }
                let Some(artifact_path) = self.private_app_path(&entry.path) else {
                    report.conflicts.push(RepairConflict::UnsafePath {
                        artifact_id: entry.artifact_id.clone(),
                        path: entry.path.clone(),
                    });
                    manifest_had_conflict = true;
                    continue;
                };
                if !artifact_path.exists() {
                    if artifact_kind_required_for_recording_source(
                        &db_artifact.recording_source,
                        &db_artifact.kind,
                    ) {
                        report.conflicts.push(RepairConflict::MissingFile {
                            artifact_id: entry.artifact_id.clone(),
                            path: entry.path.clone(),
                        });
                        manifest_had_conflict = true;
                    }
                    continue;
                }
                let recovered_sha256 = if is_pending_sha256(&entry.sha256) {
                    sha256_file(&artifact_path)?
                } else {
                    entry.sha256.clone()
                };
                recovery_plan.push(RepairArtifactRecovery {
                    artifact_id: entry.artifact_id.clone(),
                    sha256: recovered_sha256,
                    kind: db_artifact.kind,
                    recording_source: db_artifact.recording_source,
                });
            }
            if manifest_had_conflict || recovery_plan.is_empty() {
                continue;
            }
            let recording_source = recovery_plan
                .first()
                .map(|artifact| artifact.recording_source.clone())
                .unwrap_or_default();
            let artifact_kinds = recovery_plan
                .iter()
                .map(|artifact| artifact.kind.as_str())
                .collect::<Vec<_>>();
            if !artifact_kinds_satisfy_recording_source(&recording_source, &artifact_kinds) {
                report
                    .conflicts
                    .push(RepairConflict::IncompleteArtifactSet {
                        session_id: manifest.session_id.clone(),
                        recording_source,
                    });
                continue;
            }

            self.conn.execute_batch("BEGIN IMMEDIATE TRANSACTION")?;
            let recovery_result = (|| -> StoreResult<(Vec<String>, Vec<String>)> {
                let mut recovered_artifacts = Vec::new();
                for artifact in &recovery_plan {
                    self.conn.execute(
                        "
                        UPDATE audio_artifacts
                        SET sha256 = ?2,
                            write_status = 'Complete',
                            recovery_status = 'Recovered'
                        WHERE id = ?1
                        ",
                        params![artifact.artifact_id, artifact.sha256],
                    )?;
                    if self.conn.changes() > 0 {
                        recovered_artifacts.push(artifact.artifact_id.clone());
                    }
                }
                self.conn.execute(
                    "
                    UPDATE recording_sessions
                    SET status = 'Recovered',
                        recovery_note = COALESCE(
                            recovery_note,
                            'recovered completed audio artifacts during startup repair'
                        )
                    WHERE id = ?1
                    ",
                    params![manifest.session_id],
                )?;
                if self.conn.changes() == 0 {
                    return Err(
                        format!("recording session not found: {}", manifest.session_id).into(),
                    );
                }
                self.conn.execute(
                    "
                    UPDATE meetings
                    SET status = CASE
                            WHEN status IN ('Complete', 'Deleted') THEN status
                            ELSE 'Recovered'
                        END
                    WHERE id = ?1
                    ",
                    params![manifest.meeting_id],
                )?;
                if self.conn.changes() == 0 {
                    return Err(format!("meeting not found: {}", manifest.meeting_id).into());
                }

                let mut stmt = self.conn.prepare(
                    "
                    SELECT id FROM processing_jobs
                    WHERE meeting_id = ?1 AND status = 'Running'
                    ORDER BY id
                    ",
                )?;
                let job_ids = stmt
                    .query_map(params![manifest.meeting_id], |row| row.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                for job_id in &job_ids {
                    self.conn.execute(
                        "UPDATE processing_jobs SET status = 'Recovery' WHERE id = ?1",
                        params![job_id],
                    )?;
                }
                Ok((recovered_artifacts, job_ids))
            })();
            match recovery_result {
                Ok((recovered_artifacts, recovered_jobs)) => {
                    if let Err(err) = self.conn.execute_batch("COMMIT") {
                        let _ = self.conn.execute_batch("ROLLBACK");
                        return Err(err.into());
                    }
                    report.recovered_artifacts.extend(recovered_artifacts);
                    report.recovered_jobs.extend(recovered_jobs);
                }
                Err(err) => {
                    let _ = self.conn.execute_batch("ROLLBACK");
                    return Err(err);
                }
            }
        }
        report.recovered_artifacts.sort();
        report.recovered_jobs.sort();
        report.conflicts.sort();
        Ok(report)
    }

    pub fn artifact_recovery_status(&self, artifact_id: &str) -> StoreResult<RepairStatus> {
        let status: String = self.conn.query_row(
            "SELECT recovery_status FROM audio_artifacts WHERE id = ?1",
            params![artifact_id],
            |row| row.get(0),
        )?;
        parse_repair_status(&status)
    }

    pub fn job_status(&self, job_id: &str) -> StoreResult<JobStatus> {
        let status: String = self.conn.query_row(
            "SELECT status FROM processing_jobs WHERE id = ?1",
            params![job_id],
            |row| row.get(0),
        )?;
        parse_job_status(&status)
    }

    pub fn processing_job(&self, job_id: &str) -> StoreResult<ProcessingJob> {
        self.processing_job_for_query(
            "
            SELECT
                id,
                meeting_id,
                kind,
                status,
                attempts,
                last_error,
                started_at_ms,
                finished_at_ms,
                cancel_requested,
                idempotency_key
            FROM processing_jobs
            WHERE id = ?1
            ",
            params![job_id],
        )?
        .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    fn processing_job_for_query<P>(
        &self,
        sql: &str,
        params: P,
    ) -> StoreResult<Option<ProcessingJob>>
    where
        P: rusqlite::Params,
    {
        let (
            id,
            meeting_id,
            kind,
            status,
            attempts,
            last_error,
            started_at_ms,
            finished_at_ms,
            cancel_requested,
            idempotency_key,
        ): (
            String,
            String,
            String,
            String,
            u32,
            Option<String>,
            Option<u64>,
            Option<u64>,
            bool,
            Option<String>,
        ) = match self
            .conn
            .query_row(sql, params, |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                ))
            })
            .optional()?
        {
            Some(job) => job,
            None => return Ok(None),
        };
        Ok(Some(ProcessingJob {
            id,
            meeting_id,
            kind: parse_job_kind(&kind)?,
            status: parse_job_status(&status)?,
            attempts,
            last_error,
            started_at_ms,
            finished_at_ms,
            cancel_requested,
            idempotency_key,
        }))
    }

    pub fn record_exported_file(&self, meeting_id: &str, path: &Path) -> StoreResult<()> {
        self.conn.execute(
            "INSERT INTO exported_files (meeting_id, path) VALUES (?1, ?2)",
            params![meeting_id, path.to_string_lossy()],
        )?;
        Ok(())
    }

    pub fn exported_files(&self, meeting_id: &str) -> StoreResult<Vec<PathBuf>> {
        let mut stmt = self
            .conn
            .prepare("SELECT path FROM exported_files WHERE meeting_id = ?1 ORDER BY path")?;
        let files = stmt
            .query_map(params![meeting_id], |row| {
                Ok(PathBuf::from(row.get::<_, String>(0)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(files)
    }

    pub fn write_recoverable_artifact_manifest(
        &self,
        meeting_id: &str,
        session_id: &str,
        artifact_id: &str,
        artifact_path: &str,
        sha256: &str,
    ) -> StoreResult<()> {
        self.write_recoverable_artifact_manifests(
            meeting_id,
            session_id,
            &[RecoverableArtifact {
                artifact_id: artifact_id.to_string(),
                path: artifact_path.to_string(),
                sha256: sha256.to_string(),
            }],
        )
    }

    pub fn write_recoverable_artifact_manifests(
        &self,
        meeting_id: &str,
        session_id: &str,
        artifacts: &[RecoverableArtifact],
    ) -> StoreResult<()> {
        if !is_safe_meeting_id(meeting_id) {
            return Err(format!("meeting id is not safe for private storage: {meeting_id}").into());
        }
        let Some(first_artifact) = artifacts.first() else {
            return Err("recoverable manifest requires at least one audio artifact".into());
        };
        for artifact in artifacts {
            if self.private_app_path(&artifact.path).is_none() {
                return Err(format!(
                    "artifact path is not safe for private storage: {}",
                    artifact.path
                )
                .into());
            }
        }
        let manifest_path = self
            .app_root
            .join("meetings")
            .join(meeting_id)
            .join("manifest.json");
        let mut manifest = ArtifactManifest::new(
            meeting_id,
            session_id,
            &first_artifact.artifact_id,
            &first_artifact.path,
            &first_artifact.sha256,
        )
        .mark_interrupted_recoverable();
        if artifacts.len() > 1 {
            manifest.artifacts = artifacts.to_vec();
        }
        manifest.write(manifest_path)
    }

    pub fn list_meetings(&self) -> StoreResult<Vec<MeetingSummary>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, title, started_at_ms, ended_at_ms, status, transcript_state
            FROM meetings
            WHERE status != 'Deleted'
            ORDER BY started_at_ms DESC, id DESC
            ",
        )?;
        let meetings = stmt
            .query_map([], |row| {
                Ok(MeetingSummary {
                    meeting_id: row.get(0)?,
                    title: row.get(1)?,
                    started_at_ms: row.get(2)?,
                    ended_at_ms: row.get(3)?,
                    status: row.get(4)?,
                    transcript_state: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(meetings)
    }

    pub fn rename_meeting(&self, meeting_id: &str, title: &str) -> StoreResult<MeetingSummary> {
        self.ensure_active_meeting_exists(meeting_id)?;
        self.update_manifest_title(meeting_id, title)?;
        self.conn.execute_batch("BEGIN IMMEDIATE TRANSACTION")?;
        let result = self.rename_meeting_in_transaction(meeting_id, title);
        match result {
            Ok(()) => {
                if let Err(err) = self.conn.execute_batch("COMMIT") {
                    let _ = self.conn.execute_batch("ROLLBACK");
                    return Err(err.into());
                }
            }
            Err(err) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                return Err(err);
            }
        }
        self.meeting_summary(meeting_id)
    }

    fn rename_meeting_in_transaction(&self, meeting_id: &str, title: &str) -> StoreResult<()> {
        self.conn.execute(
            "UPDATE meetings SET title = ?2 WHERE id = ?1 AND status != 'Deleted'",
            params![meeting_id, title],
        )?;
        if self.conn.changes() == 0 {
            return Err(format!("meeting not found or deleted: {meeting_id}").into());
        }
        self.refresh_search_index_for_meeting(meeting_id)?;
        Ok(())
    }

    pub fn rebuild_search_index(&self) -> StoreResult<()> {
        self.conn.execute("DELETE FROM meeting_search", [])?;
        let mut stmt = self.conn.prepare(
            "
            SELECT id, title
            FROM meetings
            WHERE status != 'Deleted'
            ORDER BY id
            ",
        )?;
        let meetings = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (meeting_id, title) in meetings {
            let transcript_text = self.searchable_transcript_text(&meeting_id)?;
            self.conn.execute(
                "
                INSERT INTO meeting_search (meeting_id, title, transcript_text)
                VALUES (?1, ?2, ?3)
                ",
                params![meeting_id, title, transcript_text],
            )?;
        }
        Ok(())
    }

    fn refresh_search_index_for_meeting(&self, meeting_id: &str) -> StoreResult<()> {
        self.conn.execute(
            "DELETE FROM meeting_search WHERE meeting_id = ?1",
            params![meeting_id],
        )?;
        let meeting = self
            .conn
            .query_row(
                "
                SELECT id, title
                FROM meetings
                WHERE id = ?1 AND status != 'Deleted'
                ",
                params![meeting_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        if let Some((meeting_id, title)) = meeting {
            let transcript_text = self.searchable_transcript_text(&meeting_id)?;
            self.conn.execute(
                "
                INSERT INTO meeting_search (meeting_id, title, transcript_text)
                VALUES (?1, ?2, ?3)
                ",
                params![meeting_id, title, transcript_text],
            )?;
        }
        Ok(())
    }

    pub fn search_meetings(&self, query: &str) -> StoreResult<Vec<MeetingSearchResult>> {
        let query = fts_query(query)?;
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = self.conn.prepare(
            "
            SELECT meeting_search.meeting_id, meetings.title, bm25(meeting_search) AS rank
            FROM meeting_search
            JOIN meetings ON meetings.id = meeting_search.meeting_id
            WHERE meeting_search MATCH ?1
              AND meetings.status != 'Deleted'
            ORDER BY rank, meetings.started_at_ms DESC, meeting_search.meeting_id
            ",
        )?;
        let results = stmt
            .query_map(params![query], |row| {
                Ok(MeetingSearchResult {
                    meeting_id: row.get(0)?,
                    title: row.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(results)
    }

    pub fn export_meeting_json(
        &self,
        meeting_id: &str,
        export_root: &Path,
    ) -> StoreResult<PathBuf> {
        let export = self.meeting_export(meeting_id)?;
        fs::create_dir_all(export_root)?;
        let path = export_root.join(safe_export_filename(meeting_id)?);
        fs::write(&path, serde_json::to_vec_pretty(&export)?)?;
        self.record_exported_file(meeting_id, &path)?;
        Ok(path)
    }

    pub fn read_meeting_export_json(path: impl AsRef<Path>) -> StoreResult<MeetingExport> {
        Ok(serde_json::from_slice(&fs::read(path)?)?)
    }

    pub fn delete_meeting(&self, meeting_id: &str) -> StoreResult<DeleteReport> {
        let mut report = self.delete_report_for_meeting(meeting_id)?;

        self.conn.execute_batch("BEGIN IMMEDIATE TRANSACTION")?;
        let result = self.mark_meeting_delete_intent_in_transaction(meeting_id);
        match result {
            Ok(()) => {
                if let Err(err) = self.conn.execute_batch("COMMIT") {
                    let _ = self.conn.execute_batch("ROLLBACK");
                    return Err(err.into());
                }
            }
            Err(err) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                return Err(err);
            }
        }

        let private_artifacts = self.private_artifacts_for_delete(meeting_id)?;
        self.finalize_deleted_meeting_cleanup(meeting_id, private_artifacts, &mut report)?;
        Ok(report)
    }

    pub fn finalize_pending_delete_intents(
        &self,
    ) -> StoreResult<Vec<PendingDeleteFinalizationReport>> {
        let meeting_ids = self.deleted_meeting_ids()?;
        let mut reports = Vec::new();
        for meeting_id in meeting_ids {
            let private_artifacts = self.private_artifacts_for_delete(&meeting_id)?;
            let has_private_rows = self.private_rows_remain_for_delete(&meeting_id)?;
            let has_private_manifest = self.private_manifest_exists(&meeting_id)?;
            if private_artifacts.is_empty() && !has_private_rows && !has_private_manifest {
                continue;
            }

            let mut report = self.delete_report_for_meeting(&meeting_id)?;
            self.finalize_deleted_meeting_cleanup(&meeting_id, private_artifacts, &mut report)?;
            reports.push(PendingDeleteFinalizationReport::from_delete_report(
                meeting_id, report,
            ));
        }
        Ok(reports)
    }

    fn finalize_deleted_meeting_cleanup(
        &self,
        meeting_id: &str,
        private_artifacts: Vec<String>,
        report: &mut DeleteReport,
    ) -> StoreResult<()> {
        for artifact_path in private_artifacts {
            let Some(path) = self.private_app_path(&artifact_path) else {
                report
                    .skipped_private_artifacts
                    .push(self.reported_path(&artifact_path));
                continue;
            };
            if path.exists() {
                fs::remove_file(&path)?;
                report.deleted_private_artifacts.push(path);
            }
        }

        self.conn.execute_batch("BEGIN IMMEDIATE TRANSACTION")?;
        let result = self.delete_meeting_db_rows_in_transaction(meeting_id);
        match result {
            Ok(()) => {
                if let Err(err) = self.conn.execute_batch("COMMIT") {
                    let _ = self.conn.execute_batch("ROLLBACK");
                    return Err(err.into());
                }
            }
            Err(err) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                return Err(err);
            }
        }
        self.delete_private_manifests(meeting_id)?;
        report.deleted_private_artifacts.sort();
        report.skipped_private_artifacts.sort();
        Ok(())
    }

    pub fn meeting_deleted(&self, meeting_id: &str) -> StoreResult<bool> {
        let status = self
            .conn
            .query_row(
                "SELECT status FROM meetings WHERE id = ?1",
                params![meeting_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(status.as_deref() == Some("Deleted"))
    }

    pub fn artifact_tombstoned(&self, artifact_id: &str) -> StoreResult<bool> {
        let row = self
            .conn
            .query_row(
                "SELECT retained, tombstoned FROM audio_artifacts WHERE id = ?1",
                params![artifact_id],
                |row| Ok((row.get::<_, u8>(0)?, row.get::<_, u8>(1)?)),
            )
            .optional()?;
        Ok(match row {
            Some((retained, tombstoned)) => retained == 0 && tombstoned == 1,
            None => true,
        })
    }

    fn audio_artifact_id_by_import_identity(
        &self,
        recording_session_id: &str,
        kind: &str,
        sha256: &str,
    ) -> StoreResult<Option<String>> {
        self.conn
            .query_row(
                "
                SELECT id FROM audio_artifacts
                WHERE recording_session_id = ?1
                  AND kind = ?2
                  AND sha256 = ?3
                ",
                params![recording_session_id, kind, sha256],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn persist_transcript(
        &self,
        model_run: &ModelRun,
        version: &TranscriptVersion,
        segments: &[TranscriptSegment],
    ) -> StoreResult<()> {
        self.conn.execute_batch("BEGIN IMMEDIATE TRANSACTION")?;
        let result = self.persist_transcript_in_transaction(model_run, version, segments);
        match result {
            Ok(()) => {
                if let Err(err) = self.conn.execute_batch("COMMIT") {
                    let _ = self.conn.execute_batch("ROLLBACK");
                    return Err(err.into());
                }
                self.refresh_search_index_for_meeting(&model_run.meeting_id)?;
                Ok(())
            }
            Err(err) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(err)
            }
        }
    }

    fn persist_transcript_in_transaction(
        &self,
        model_run: &ModelRun,
        version: &TranscriptVersion,
        segments: &[TranscriptSegment],
    ) -> StoreResult<()> {
        let model_run_id = match self.model_run_by_transcript_identity(model_run)? {
            Some((existing_id, network_used)) => {
                if network_used != model_run.network_used {
                    return Err("transcript replay conflict: model metadata changed".into());
                }
                existing_id
            }
            None => {
                self.conn.execute(
                    "
                    INSERT INTO model_runs (
                        id, meeting_id, source_artifact_sha256, provider, model_name, network_used, created_at_ms
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                    ",
                    params![
                        model_run.id,
                        model_run.meeting_id,
                        model_run.source_artifact_sha256,
                        model_run.provider,
                        model_run.model_name,
                        model_run.network_used,
                        model_run.created_at_ms,
                    ],
                )?;
                model_run.id.clone()
            }
        };
        let transcript_version_id = match self.transcript_version_by_identity(
            &version.meeting_id,
            &model_run_id,
            version.version,
        )? {
            Some(existing_id) => {
                self.ensure_transcript_replay_matches(&existing_id, segments)?;
                existing_id
            }
            None => {
                self.conn.execute(
                    "
                        INSERT INTO transcript_versions (
                            id, meeting_id, model_run_id, version, created_at_ms, edited_at_ms
                        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                        ",
                    params![
                        version.id,
                        version.meeting_id,
                        model_run_id,
                        version.version,
                        version.created_at_ms,
                        version.edited_at_ms,
                    ],
                )?;
                version.id.clone()
            }
        };
        if self.transcript_segment_count_for_version(&transcript_version_id)? > 0 {
            self.conn.execute(
                "UPDATE meetings SET transcript_state = 'Complete' WHERE id = ?1",
                params![model_run.meeting_id],
            )?;
            return Ok(());
        }
        for (ordinal, segment) in segments.iter().enumerate() {
            self.conn.execute(
                "
                INSERT INTO transcript_segments (
                    id, meeting_id, transcript_version_id, model_run_id, ordinal,
                    start_ms, end_ms, text, original_text, source_channel
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                ",
                params![
                    segment.id,
                    segment.meeting_id,
                    transcript_version_id,
                    model_run_id,
                    ordinal as u32,
                    segment.start_ms,
                    segment.end_ms,
                    segment.text,
                    segment.original_text,
                    enum_name(segment.source_channel),
                ],
            )?;
        }
        self.conn.execute(
            "UPDATE meetings SET transcript_state = 'Complete' WHERE id = ?1",
            params![model_run.meeting_id],
        )?;
        Ok(())
    }

    pub fn transcript_segments(&self, meeting_id: &str) -> StoreResult<Vec<TranscriptSegment>> {
        let Some(version_id) = self.current_transcript_version_id(meeting_id)? else {
            return Ok(Vec::new());
        };
        self.transcript_segments_for_version(&version_id)
    }

    fn transcript_segments_for_version(
        &self,
        version_id: &str,
    ) -> StoreResult<Vec<TranscriptSegment>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT
                id, meeting_id, start_ms, end_ms, text, source_channel,
                model_run_id, transcript_version_id, original_text
            FROM transcript_segments
            WHERE transcript_version_id = ?1
            ORDER BY start_ms, end_ms, ordinal
            ",
        )?;
        let segments = stmt
            .query_map(params![version_id], |row| {
                let source_channel: String = row.get(5)?;
                Ok(TranscriptSegment {
                    id: row.get(0)?,
                    meeting_id: row.get(1)?,
                    start_ms: row.get(2)?,
                    end_ms: row.get(3)?,
                    text: row.get(4)?,
                    source_channel: parse_source_channel(&source_channel).map_err(|err| {
                        rusqlite::Error::FromSqlConversionFailure(
                            5,
                            rusqlite::types::Type::Text,
                            err.into(),
                        )
                    })?,
                    model_run_id: row.get(6)?,
                    transcript_version_id: row.get(7)?,
                    original_text: row.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(segments)
    }

    pub fn transcript_segment_edits(
        &self,
        segment_id: &str,
    ) -> StoreResult<Vec<TranscriptSegmentEdit>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT segment_id, transcript_version_id, edited_at_ms, previous_text, corrected_text
            FROM transcript_segment_edits
            WHERE segment_id = ?1
            ORDER BY edited_at_ms, id
            ",
        )?;
        let edits = stmt
            .query_map(params![segment_id], |row| {
                Ok(TranscriptSegmentEdit {
                    segment_id: row.get(0)?,
                    transcript_version_id: row.get(1)?,
                    edited_at_ms: row.get(2)?,
                    previous_text: row.get(3)?,
                    corrected_text: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(edits)
    }

    pub fn persist_analysis_result(&self, analysis: &MeetingAnalysis) -> StoreResult<()> {
        self.conn.execute_batch("BEGIN IMMEDIATE TRANSACTION")?;
        let result = self.persist_analysis_result_in_transaction(analysis);
        match result {
            Ok(()) => {
                if let Err(err) = self.conn.execute_batch("COMMIT") {
                    let _ = self.conn.execute_batch("ROLLBACK");
                    return Err(err.into());
                }
                Ok(())
            }
            Err(err) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(err)
            }
        }
    }

    fn persist_analysis_result_in_transaction(
        &self,
        analysis: &MeetingAnalysis,
    ) -> StoreResult<()> {
        self.ensure_active_meeting_exists(&analysis.meeting_id)?;
        if let Some(existing) = self.analysis_result_by_identity(analysis)? {
            if existing == *analysis {
                return Ok(());
            }
            return Err("analysis replay conflict: result changed".into());
        }
        let result_json = serde_json::to_string(analysis)?;
        self.conn.execute(
            "
            INSERT INTO analysis_results (
                id, meeting_id, provider, model_name, network_used,
                created_at_ms, prompt_template_version, result_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ",
            params![
                analysis.id,
                analysis.meeting_id,
                analysis.provider,
                analysis.model_name,
                analysis.network_used,
                analysis.created_at_ms,
                analysis.prompt_template_version,
                result_json,
            ],
        )?;
        Ok(())
    }

    fn analysis_result_by_identity(
        &self,
        analysis: &MeetingAnalysis,
    ) -> StoreResult<Option<MeetingAnalysis>> {
        let result_json = self
            .conn
            .query_row(
                "
                SELECT result_json
                FROM analysis_results
                WHERE meeting_id = ?1
                  AND provider = ?2
                  AND model_name = ?3
                  AND prompt_template_version = ?4
                ",
                params![
                    analysis.meeting_id,
                    analysis.provider,
                    analysis.model_name,
                    analysis.prompt_template_version,
                ],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        result_json
            .map(|json| serde_json::from_str(&json).map_err(Into::into))
            .transpose()
    }

    pub fn current_analysis_result(
        &self,
        meeting_id: &str,
    ) -> StoreResult<Option<MeetingAnalysis>> {
        let result_json = self
            .conn
            .query_row(
                "
                SELECT result_json
                FROM analysis_results
                WHERE meeting_id = ?1
                ORDER BY created_at_ms DESC, id DESC
                LIMIT 1
                ",
                params![meeting_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        result_json
            .map(|json| serde_json::from_str(&json).map_err(Into::into))
            .transpose()
    }

    pub fn correct_transcript_segment(
        &self,
        segment_id: &str,
        corrected_text: &str,
        edited_at_ms: u64,
    ) -> StoreResult<()> {
        self.conn.execute_batch("BEGIN IMMEDIATE TRANSACTION")?;
        let result = self.correct_transcript_segment_in_transaction(
            segment_id,
            corrected_text,
            edited_at_ms,
        );
        match result {
            Ok(()) => {
                if let Err(err) = self.conn.execute_batch("COMMIT") {
                    let _ = self.conn.execute_batch("ROLLBACK");
                    return Err(err.into());
                }
                Ok(())
            }
            Err(err) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(err)
            }
        }
    }

    fn correct_transcript_segment_in_transaction(
        &self,
        segment_id: &str,
        corrected_text: &str,
        edited_at_ms: u64,
    ) -> StoreResult<()> {
        let (version_id, previous_text): (String, String) = self.conn.query_row(
            "SELECT transcript_version_id, text FROM transcript_segments WHERE id = ?1",
            params![segment_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        self.conn.execute(
            "
            INSERT INTO transcript_segment_edits (
                segment_id, transcript_version_id, edited_at_ms, previous_text, corrected_text
            ) VALUES (?1, ?2, ?3, ?4, ?5)
            ",
            params![
                segment_id,
                version_id,
                edited_at_ms,
                previous_text,
                corrected_text
            ],
        )?;
        self.conn.execute(
            "
            UPDATE transcript_segments
            SET original_text = COALESCE(original_text, text),
                text = ?2
            WHERE id = ?1
            ",
            params![segment_id, corrected_text],
        )?;
        self.conn.execute(
            "UPDATE transcript_versions SET edited_at_ms = ?2 WHERE id = ?1",
            params![version_id, edited_at_ms],
        )?;
        let meeting_id: String = self.conn.query_row(
            "SELECT meeting_id FROM transcript_segments WHERE id = ?1",
            params![segment_id],
            |row| row.get(0),
        )?;
        self.refresh_search_index_for_meeting(&meeting_id)?;
        Ok(())
    }

    fn model_run_by_transcript_identity(
        &self,
        model_run: &ModelRun,
    ) -> StoreResult<Option<(String, bool)>> {
        self.conn
            .query_row(
                "
                SELECT id, network_used FROM model_runs
                WHERE meeting_id = ?1
                  AND source_artifact_sha256 = ?2
                  AND provider = ?3
                  AND model_name = ?4
                ",
                params![
                    model_run.meeting_id,
                    model_run.source_artifact_sha256,
                    model_run.provider,
                    model_run.model_name,
                ],
                |row| {
                    let network_used: u8 = row.get(1)?;
                    Ok((row.get(0)?, network_used != 0))
                },
            )
            .optional()
            .map_err(Into::into)
    }

    fn transcript_version_by_identity(
        &self,
        meeting_id: &str,
        model_run_id: &str,
        version: u32,
    ) -> StoreResult<Option<String>> {
        self.conn
            .query_row(
                "
                SELECT id FROM transcript_versions
                WHERE meeting_id = ?1
                  AND model_run_id = ?2
                  AND version = ?3
                ",
                params![meeting_id, model_run_id, version],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    fn current_transcript_version_id(&self, meeting_id: &str) -> StoreResult<Option<String>> {
        self.conn
            .query_row(
                "
                SELECT id FROM transcript_versions
                WHERE meeting_id = ?1
                ORDER BY created_at_ms DESC, id DESC
                LIMIT 1
                ",
                params![meeting_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    fn transcript_segment_count_for_version(&self, version_id: &str) -> StoreResult<u64> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM transcript_segments WHERE transcript_version_id = ?1",
            params![version_id],
            |row| row.get(0),
        )?)
    }

    fn ensure_transcript_replay_matches(
        &self,
        version_id: &str,
        segments: &[TranscriptSegment],
    ) -> StoreResult<()> {
        let existing = self.transcript_segments_for_version(version_id)?;
        if existing.len() != segments.len() {
            return Err("transcript replay conflict: segment count changed".into());
        }
        for (existing, incoming) in existing.iter().zip(segments) {
            if existing.start_ms != incoming.start_ms
                || existing.end_ms != incoming.end_ms
                || existing.text != incoming.text
                || existing.source_channel != incoming.source_channel
            {
                return Err("transcript replay conflict: segment content changed".into());
            }
        }
        Ok(())
    }

    pub fn meeting_title(&self, meeting_id: &str) -> StoreResult<String> {
        Ok(self.conn.query_row(
            "SELECT title FROM meetings WHERE id = ?1",
            params![meeting_id],
            |row| row.get(0),
        )?)
    }

    fn meeting_summary(&self, meeting_id: &str) -> StoreResult<MeetingSummary> {
        Ok(self.conn.query_row(
            "
            SELECT id, title, started_at_ms, ended_at_ms, status, transcript_state
            FROM meetings
            WHERE id = ?1
            ",
            params![meeting_id],
            |row| {
                Ok(MeetingSummary {
                    meeting_id: row.get(0)?,
                    title: row.get(1)?,
                    started_at_ms: row.get(2)?,
                    ended_at_ms: row.get(3)?,
                    status: row.get(4)?,
                    transcript_state: row.get(5)?,
                })
            },
        )?)
    }

    fn ensure_active_meeting_exists(&self, meeting_id: &str) -> StoreResult<()> {
        let exists = self
            .conn
            .query_row(
                "SELECT 1 FROM meetings WHERE id = ?1 AND status != 'Deleted'",
                params![meeting_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if exists {
            Ok(())
        } else {
            Err(format!("meeting not found or deleted: {meeting_id}").into())
        }
    }

    fn searchable_transcript_text(&self, meeting_id: &str) -> StoreResult<String> {
        let mut parts = Vec::new();
        let Some(version_id) = self.current_transcript_version_id(meeting_id)? else {
            return Ok(String::new());
        };
        let mut stmt = self.conn.prepare(
            "
            SELECT text, original_text
            FROM transcript_segments
            WHERE transcript_version_id = ?1
            ORDER BY start_ms, end_ms, ordinal
            ",
        )?;
        let segments = stmt
            .query_map(params![version_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (text, original_text) in segments {
            parts.push(text);
            if let Some(original_text) = original_text {
                parts.push(original_text);
            }
        }
        let mut edits = self.conn.prepare(
            "
            SELECT corrected_text
            FROM transcript_segment_edits
            WHERE transcript_version_id = ?1
            ORDER BY edited_at_ms, id
            ",
        )?;
        for corrected_text in edits.query_map(params![version_id], |row| row.get::<_, String>(0))? {
            parts.push(corrected_text?);
        }
        Ok(parts.join("\n"))
    }

    fn update_manifest_title(&self, meeting_id: &str, title: &str) -> StoreResult<()> {
        let mut staged: Vec<(PathBuf, PathBuf, Vec<u8>)> = Vec::new();
        for manifest_path in manifest_paths(&self.app_root)? {
            let original_bytes = fs::read(&manifest_path).map_err(|err| {
                format!(
                    "manifest read failed for {}: {err}",
                    manifest_path.display()
                )
            })?;
            let mut manifest: ArtifactManifest =
                serde_json::from_slice(&original_bytes).map_err(|err| {
                    format!(
                        "manifest read failed for {}: {err}",
                        manifest_path.display()
                    )
                })?;
            if manifest.meeting_id != meeting_id {
                continue;
            }
            manifest.meeting_title = Some(title.to_string());
            let temp_path = manifest_update_temp_path(&manifest_path);
            let stage_result = fs::write(&temp_path, serde_json::to_vec_pretty(&manifest)?);
            if let Err(err) = stage_result {
                for (_, temp_path, _) in staged {
                    let _ = fs::remove_file(temp_path);
                }
                return Err(format!(
                    "manifest title update failed for {}: {err}",
                    manifest_path.display()
                )
                .into());
            }
            staged.push((manifest_path, temp_path, original_bytes));
        }

        let mut replaced: Vec<(PathBuf, Vec<u8>)> = Vec::new();
        for (manifest_path, temp_path, original_bytes) in staged {
            if let Err(err) = fs::rename(&temp_path, &manifest_path) {
                let _ = fs::remove_file(&temp_path);
                for (replaced_path, original_bytes) in replaced {
                    let _ = fs::write(replaced_path, original_bytes);
                }
                return Err(format!(
                    "manifest title update failed for {}: {err}",
                    manifest_path.display()
                )
                .into());
            }
            replaced.push((manifest_path, original_bytes));
        }
        Ok(())
    }

    fn meeting_export(&self, meeting_id: &str) -> StoreResult<MeetingExport> {
        let summary = self.meeting_summary(meeting_id)?;
        let mut segments = Vec::new();
        for segment in self.transcript_segments(meeting_id)? {
            let edits = self
                .transcript_segment_edits(&segment.id)?
                .into_iter()
                .map(|edit| TranscriptSegmentEditExport {
                    edited_at_ms: edit.edited_at_ms,
                    previous_text: edit.previous_text,
                    corrected_text: edit.corrected_text,
                })
                .collect();
            segments.push(TranscriptSegmentExport {
                id: segment.id,
                start_ms: segment.start_ms,
                end_ms: segment.end_ms,
                text: segment.text,
                original_text: segment.original_text,
                source_channel: enum_name(segment.source_channel).to_string(),
                model_run_id: segment.model_run_id,
                transcript_version_id: segment.transcript_version_id,
                edits,
            });
        }
        Ok(MeetingExport {
            meeting_id: summary.meeting_id,
            title: summary.title,
            started_at_ms: summary.started_at_ms,
            ended_at_ms: summary.ended_at_ms,
            segments,
        })
    }

    fn delete_meeting_db_rows_in_transaction(&self, meeting_id: &str) -> StoreResult<()> {
        self.delete_private_meeting_rows(meeting_id)?;
        Ok(())
    }

    fn delete_report_for_meeting(&self, meeting_id: &str) -> StoreResult<DeleteReport> {
        let mut report = DeleteReport::default();
        let mut exports = self
            .conn
            .prepare("SELECT path FROM exported_files WHERE meeting_id = ?1 ORDER BY path")?;
        report.exported_files_outside_app_control = exports
            .query_map(params![meeting_id], |row| {
                Ok(PathBuf::from(row.get::<_, String>(0)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(report)
    }

    fn deleted_meeting_ids(&self) -> StoreResult<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id FROM meetings
            WHERE status = 'Deleted' OR deleted_at_ms IS NOT NULL
            ORDER BY id
            ",
        )?;
        let meeting_ids = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(meeting_ids)
    }

    fn mark_meeting_delete_intent_in_transaction(&self, meeting_id: &str) -> StoreResult<()> {
        self.conn.execute(
            "UPDATE meetings SET status = 'Deleted', deleted_at_ms = COALESCE(deleted_at_ms, 0) WHERE id = ?1",
            params![meeting_id],
        )?;
        self.conn.execute(
            "
            UPDATE audio_artifacts
            SET retained = 0,
                tombstoned = 1
            WHERE recording_session_id IN (
                SELECT id FROM recording_sessions WHERE meeting_id = ?1
            )
            ",
            params![meeting_id],
        )?;
        self.conn.execute(
            "DELETE FROM meeting_search WHERE meeting_id = ?1",
            params![meeting_id],
        )?;
        Ok(())
    }

    fn private_artifacts_for_delete(&self, meeting_id: &str) -> StoreResult<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT audio_artifacts.path
            FROM audio_artifacts
            JOIN recording_sessions
              ON recording_sessions.id = audio_artifacts.recording_session_id
            WHERE recording_sessions.meeting_id = ?1
            ORDER BY audio_artifacts.path
            ",
        )?;
        let artifacts = stmt
            .query_map(params![meeting_id], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(artifacts)
    }

    fn private_rows_remain_for_delete(&self, meeting_id: &str) -> StoreResult<bool> {
        let remains = self.conn.query_row(
            "
            SELECT CASE WHEN
                EXISTS(SELECT 1 FROM recording_sessions WHERE meeting_id = ?1)
                OR EXISTS(SELECT 1 FROM analysis_results WHERE meeting_id = ?1)
                OR EXISTS(SELECT 1 FROM transcript_segments WHERE meeting_id = ?1)
                OR EXISTS(SELECT 1 FROM transcript_versions WHERE meeting_id = ?1)
                OR EXISTS(SELECT 1 FROM model_runs WHERE meeting_id = ?1)
                OR EXISTS(SELECT 1 FROM processing_jobs WHERE meeting_id = ?1)
                OR EXISTS(SELECT 1 FROM meeting_search WHERE meeting_id = ?1)
                OR EXISTS(
                    SELECT 1 FROM transcript_segment_edits
                    WHERE transcript_version_id IN (
                        SELECT id FROM transcript_versions WHERE meeting_id = ?1
                    )
                )
            THEN 1 ELSE 0 END
            ",
            params![meeting_id],
            |row| row.get::<_, u8>(0),
        )?;
        Ok(remains != 0)
    }

    fn delete_private_meeting_rows(&self, meeting_id: &str) -> StoreResult<()> {
        self.conn.execute(
            "
            DELETE FROM transcript_segment_edits
            WHERE transcript_version_id IN (
                SELECT id FROM transcript_versions WHERE meeting_id = ?1
            )
            ",
            params![meeting_id],
        )?;
        self.conn.execute(
            "DELETE FROM analysis_results WHERE meeting_id = ?1",
            params![meeting_id],
        )?;
        self.conn.execute(
            "DELETE FROM transcript_segments WHERE meeting_id = ?1",
            params![meeting_id],
        )?;
        self.conn.execute(
            "DELETE FROM transcript_versions WHERE meeting_id = ?1",
            params![meeting_id],
        )?;
        self.conn.execute(
            "DELETE FROM model_runs WHERE meeting_id = ?1",
            params![meeting_id],
        )?;
        self.conn.execute(
            "DELETE FROM processing_jobs WHERE meeting_id = ?1",
            params![meeting_id],
        )?;
        self.conn.execute(
            "DELETE FROM meeting_search WHERE meeting_id = ?1",
            params![meeting_id],
        )?;
        self.conn.execute(
            "
            DELETE FROM audio_artifacts
            WHERE recording_session_id IN (
                SELECT id FROM recording_sessions WHERE meeting_id = ?1
            )
            ",
            params![meeting_id],
        )?;
        self.conn.execute(
            "DELETE FROM recording_sessions WHERE meeting_id = ?1",
            params![meeting_id],
        )?;
        Ok(())
    }

    fn private_manifest_exists(&self, meeting_id: &str) -> StoreResult<bool> {
        for manifest_path in manifest_paths(&self.app_root)? {
            let manifest = ArtifactManifest::read(&manifest_path)?;
            if manifest.meeting_id != meeting_id {
                continue;
            }
            let Some(relative_path) = manifest_path.strip_prefix(&self.app_root).ok() else {
                continue;
            };
            if self
                .private_app_path(&relative_path.to_string_lossy())
                .is_some()
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn delete_private_manifests(&self, meeting_id: &str) -> StoreResult<()> {
        for manifest_path in manifest_paths(&self.app_root)? {
            let manifest = ArtifactManifest::read(&manifest_path)?;
            if manifest.meeting_id == meeting_id {
                let Some(relative_path) = manifest_path.strip_prefix(&self.app_root).ok() else {
                    continue;
                };
                let Some(safe_path) = self.private_app_path(&relative_path.to_string_lossy())
                else {
                    continue;
                };
                if safe_path.exists() {
                    fs::remove_file(safe_path)?;
                }
            }
        }
        Ok(())
    }

    fn db_artifact_for_repair(
        &self,
        artifact_id: &str,
    ) -> StoreResult<Option<DbArtifactForRepair>> {
        self.conn
            .query_row(
                "
                SELECT
                    audio_artifacts.recording_session_id,
                    recording_sessions.meeting_id,
                    audio_artifacts.kind,
                    recording_sessions.source,
                    audio_artifacts.path,
                    audio_artifacts.sha256,
                    audio_artifacts.write_status,
                    audio_artifacts.recovery_status,
                    recording_sessions.status,
                    meetings.status,
                    meetings.deleted_at_ms,
                    audio_artifacts.retained,
                    audio_artifacts.tombstoned
                FROM audio_artifacts
                JOIN recording_sessions
                  ON recording_sessions.id = audio_artifacts.recording_session_id
                JOIN meetings
                  ON meetings.id = recording_sessions.meeting_id
                WHERE audio_artifacts.id = ?1
                ",
                params![artifact_id],
                |row| {
                    Ok(DbArtifactForRepair {
                        session_id: row.get(0)?,
                        meeting_id: row.get(1)?,
                        kind: row.get(2)?,
                        recording_source: row.get(3)?,
                        path: row.get(4)?,
                        sha256: row.get(5)?,
                        write_status: row.get(6)?,
                        recovery_status: row.get(7)?,
                        session_status: row.get(8)?,
                        meeting_status: row.get(9)?,
                        meeting_deleted_at_ms: row.get(10)?,
                        retained: row.get(11)?,
                        tombstoned: row.get(12)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
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

    fn private_app_path(&self, path: &str) -> Option<PathBuf> {
        let path = PathBuf::from(path);
        if path.is_absolute()
            || path
                .components()
                .any(|component| component == Component::ParentDir)
        {
            return None;
        }
        let candidate = self.app_root.join(path);
        let canonical_check_path = nearest_existing_path(&candidate)?;
        let canonical_check_path = canonical_check_path.canonicalize().ok()?;
        if canonical_check_path.starts_with(&self.canonical_app_root) {
            Some(candidate)
        } else {
            None
        }
    }

    fn reported_path(&self, path: &str) -> PathBuf {
        let path = PathBuf::from(path);
        if path.is_absolute() {
            path
        } else {
            self.app_root.join(path)
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RepairReport {
    pub recovered_artifacts: Vec<String>,
    pub recovered_jobs: Vec<String>,
    pub conflicts: Vec<RepairConflict>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RepairConflict {
    MissingArtifact {
        artifact_id: String,
    },
    MissingFile {
        artifact_id: String,
        path: String,
    },
    IncompleteArtifactSet {
        session_id: String,
        recording_source: String,
    },
    MismatchedMeeting {
        artifact_id: String,
        manifest_meeting_id: String,
        db_meeting_id: String,
    },
    MismatchedSession {
        artifact_id: String,
        manifest_session_id: String,
        db_session_id: String,
    },
    MismatchedPath {
        artifact_id: String,
        manifest_path: String,
        db_path: String,
    },
    MismatchedHash {
        artifact_id: String,
        manifest_sha256: String,
        db_sha256: String,
    },
    MismatchedWriteStatus {
        artifact_id: String,
        manifest_status: String,
        db_status: String,
    },
    MismatchedRecoveryStatus {
        artifact_id: String,
        manifest_status: String,
        db_status: String,
    },
    DeletedOrTombstonedArtifact {
        artifact_id: String,
    },
    InactiveRecordingArtifact {
        artifact_id: String,
        meeting_status: String,
        session_status: String,
    },
    UnsafePath {
        artifact_id: String,
        path: String,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeleteReport {
    pub deleted_private_artifacts: Vec<PathBuf>,
    pub skipped_private_artifacts: Vec<PathBuf>,
    pub exported_files_outside_app_control: Vec<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingDeleteFinalizationReport {
    pub meeting_id: String,
    pub deleted_private_artifacts: Vec<PathBuf>,
    pub skipped_private_artifacts: Vec<PathBuf>,
    pub exported_files_outside_app_control: Vec<PathBuf>,
}

impl PendingDeleteFinalizationReport {
    fn from_delete_report(meeting_id: String, report: DeleteReport) -> Self {
        Self {
            meeting_id,
            deleted_private_artifacts: report.deleted_private_artifacts,
            skipped_private_artifacts: report.skipped_private_artifacts,
            exported_files_outside_app_control: report.exported_files_outside_app_control,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TranscriptSegmentEdit {
    pub segment_id: String,
    pub transcript_version_id: String,
    pub edited_at_ms: u64,
    pub previous_text: String,
    pub corrected_text: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MeetingExport {
    pub meeting_id: String,
    pub title: String,
    pub started_at_ms: u64,
    pub ended_at_ms: Option<u64>,
    pub segments: Vec<TranscriptSegmentExport>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TranscriptSegmentExport {
    pub id: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub original_text: Option<String>,
    pub source_channel: String,
    pub model_run_id: String,
    pub transcript_version_id: String,
    pub edits: Vec<TranscriptSegmentEditExport>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TranscriptSegmentEditExport {
    pub edited_at_ms: u64,
    pub previous_text: String,
    pub corrected_text: String,
}

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

fn recoverable_artifact_entries(manifest: &ArtifactManifest) -> Vec<ArtifactManifest> {
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

fn manifest_paths(root: &Path) -> StoreResult<Vec<PathBuf>> {
    let meetings = root.join("meetings");
    if !meetings.exists() {
        return Ok(Vec::new());
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

trait StoreEnum {
    fn as_store_str(&self) -> &'static str;
}

impl StoreEnum for MeetingStatus {
    fn as_store_str(&self) -> &'static str {
        match self {
            MeetingStatus::Created => "Created",
            MeetingStatus::Recording => "Recording",
            MeetingStatus::Paused => "Paused",
            MeetingStatus::Stopping => "Stopping",
            MeetingStatus::Interrupted => "Interrupted",
            MeetingStatus::Recovered => "Recovered",
            MeetingStatus::Transcribing => "Transcribing",
            MeetingStatus::Complete => "Complete",
            MeetingStatus::Failed => "Failed",
            MeetingStatus::Deleted => "Deleted",
        }
    }
}

impl StoreEnum for TranscriptState {
    fn as_store_str(&self) -> &'static str {
        match self {
            TranscriptState::NotStarted => "NotStarted",
            TranscriptState::Transcribing => "Transcribing",
            TranscriptState::Complete => "Complete",
        }
    }
}

impl StoreEnum for RecordingSource {
    fn as_store_str(&self) -> &'static str {
        match self {
            RecordingSource::Microphone => "Microphone",
            RecordingSource::System => "System",
            RecordingSource::Mixed => "Mixed",
            RecordingSource::Imported => "Imported",
        }
    }
}

impl StoreEnum for RecordingStatus {
    fn as_store_str(&self) -> &'static str {
        match self {
            RecordingStatus::Recording => "Recording",
            RecordingStatus::Paused => "Paused",
            RecordingStatus::Stopping => "Stopping",
            RecordingStatus::Interrupted => "Interrupted",
            RecordingStatus::Recovered => "Recovered",
            RecordingStatus::Complete => "Complete",
            RecordingStatus::Failed => "Failed",
        }
    }
}

impl StoreEnum for ArtifactKind {
    fn as_store_str(&self) -> &'static str {
        match self {
            ArtifactKind::RawMic => "RawMic",
            ArtifactKind::RawSystem => "RawSystem",
            ArtifactKind::Mixed => "Mixed",
            ArtifactKind::Imported => "Imported",
        }
    }
}

impl StoreEnum for SourceChannel {
    fn as_store_str(&self) -> &'static str {
        match self {
            SourceChannel::Microphone => "Microphone",
            SourceChannel::System => "System",
            SourceChannel::Mixed => "Mixed",
            SourceChannel::Imported => "Imported",
        }
    }
}

impl StoreEnum for curiosity_domain::JobKind {
    fn as_store_str(&self) -> &'static str {
        match self {
            curiosity_domain::JobKind::Transcribe => "Transcribe",
            curiosity_domain::JobKind::Summarize => "Summarize",
            curiosity_domain::JobKind::Export => "Export",
            curiosity_domain::JobKind::Index => "Index",
        }
    }
}

impl StoreEnum for JobStatus {
    fn as_store_str(&self) -> &'static str {
        match self {
            JobStatus::Queued => "Queued",
            JobStatus::Running => "Running",
            JobStatus::Succeeded => "Succeeded",
            JobStatus::Failed => "Failed",
            JobStatus::Canceled => "Canceled",
            JobStatus::Retry => "Retry",
            JobStatus::Recovery => "Recovery",
        }
    }
}

impl StoreEnum for WriteStatus {
    fn as_store_str(&self) -> &'static str {
        match self {
            WriteStatus::Writing => "Writing",
            WriteStatus::Complete => "Complete",
        }
    }
}

impl StoreEnum for RepairStatus {
    fn as_store_str(&self) -> &'static str {
        match self {
            RepairStatus::NotNeeded => "NotNeeded",
            RepairStatus::Recoverable => "Recoverable",
            RepairStatus::Recovered => "Recovered",
        }
    }
}

fn enum_name<T: StoreEnum>(value: T) -> &'static str {
    value.as_store_str()
}

impl Serialize for WriteStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(enum_name(*self))
    }
}

impl<'de> Deserialize<'de> for WriteStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        parse_write_status_value(&value).map_err(serde::de::Error::custom)
    }
}

impl Serialize for RepairStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(enum_name(*self))
    }
}

impl<'de> Deserialize<'de> for RepairStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        parse_repair_status_value(&value).map_err(serde::de::Error::custom)
    }
}

fn safe_export_filename(meeting_id: &str) -> StoreResult<String> {
    if !is_safe_meeting_id(meeting_id) {
        return Err(format!("meeting id is not a safe export filename: {meeting_id}").into());
    }
    Ok(format!("{meeting_id}.json"))
}

fn is_pending_sha256(sha256: &str) -> bool {
    // Bare `sha256:pending` is tolerated for manifests/DB rows written before
    // pending hashes became artifact-scoped. New app writes use the prefixed
    // `sha256:pending:<artifact-id>` form.
    sha256 == PENDING_SHA_PREFIX || sha256.starts_with("sha256:pending:")
}

fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8 * 1024];
    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn is_safe_meeting_id(meeting_id: &str) -> bool {
    let mut components = Path::new(meeting_id).components();
    !meeting_id.is_empty()
        && !meeting_id.contains('/')
        && !meeting_id.contains('\\')
        && !meeting_id.contains("..")
        && !Path::new(meeting_id).is_absolute()
        && matches!(components.next(), Some(Component::Normal(_)))
        && components.next().is_none()
}

fn manifest_update_temp_path(path: &Path) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    path.with_extension(format!("json.rename-tmp-{}-{nonce}", std::process::id(),))
}

fn fts_query(query: &str) -> StoreResult<String> {
    let tokens = query
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| format!("\"{}\"", token.replace('"', "\"\"")))
        .collect::<Vec<_>>();
    Ok(tokens.join(" "))
}

fn default_if_blank<'a>(value: &'a str, default: &'static str) -> &'a str {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        default
    } else {
        trimmed
    }
}

fn nearest_existing_path(path: &Path) -> Option<PathBuf> {
    let mut current = path.to_path_buf();
    loop {
        if current.exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

#[derive(Clone, Debug)]
struct DbArtifactForRepair {
    session_id: String,
    meeting_id: String,
    kind: String,
    recording_source: String,
    path: String,
    sha256: String,
    write_status: String,
    recovery_status: String,
    session_status: String,
    meeting_status: String,
    meeting_deleted_at_ms: Option<u64>,
    retained: u8,
    tombstoned: u8,
}

#[derive(Clone, Debug)]
struct RepairArtifactRecovery {
    artifact_id: String,
    sha256: String,
    kind: String,
    recording_source: String,
}

fn repair_conflict(
    manifest: &ArtifactManifest,
    db_artifact: &DbArtifactForRepair,
) -> Option<RepairConflict> {
    if manifest.meeting_id != db_artifact.meeting_id {
        return Some(RepairConflict::MismatchedMeeting {
            artifact_id: manifest.artifact_id.clone(),
            manifest_meeting_id: manifest.meeting_id.clone(),
            db_meeting_id: db_artifact.meeting_id.clone(),
        });
    }
    if manifest.session_id != db_artifact.session_id {
        return Some(RepairConflict::MismatchedSession {
            artifact_id: manifest.artifact_id.clone(),
            manifest_session_id: manifest.session_id.clone(),
            db_session_id: db_artifact.session_id.clone(),
        });
    }
    if manifest.path != db_artifact.path {
        return Some(RepairConflict::MismatchedPath {
            artifact_id: manifest.artifact_id.clone(),
            manifest_path: manifest.path.clone(),
            db_path: db_artifact.path.clone(),
        });
    }
    if manifest.sha256 != db_artifact.sha256 {
        return Some(RepairConflict::MismatchedHash {
            artifact_id: manifest.artifact_id.clone(),
            manifest_sha256: manifest.sha256.clone(),
            db_sha256: db_artifact.sha256.clone(),
        });
    }
    let manifest_write_status = enum_name(manifest.write_status);
    if manifest_write_status != db_artifact.write_status {
        return Some(RepairConflict::MismatchedWriteStatus {
            artifact_id: manifest.artifact_id.clone(),
            manifest_status: manifest_write_status.to_string(),
            db_status: db_artifact.write_status.clone(),
        });
    }
    let manifest_recovery_status = enum_name(manifest.recovery_status);
    if db_artifact.recovery_status != "NotNeeded"
        && manifest_recovery_status != db_artifact.recovery_status
    {
        return Some(RepairConflict::MismatchedRecoveryStatus {
            artifact_id: manifest.artifact_id.clone(),
            manifest_status: manifest_recovery_status.to_string(),
            db_status: db_artifact.recovery_status.clone(),
        });
    }
    if db_artifact.meeting_status == "Deleted"
        || db_artifact.meeting_deleted_at_ms.is_some()
        || db_artifact.retained == 0
        || db_artifact.tombstoned != 0
    {
        return Some(RepairConflict::DeletedOrTombstonedArtifact {
            artifact_id: manifest.artifact_id.clone(),
        });
    }
    if db_artifact.meeting_status == "Failed"
        || matches!(
            db_artifact.session_status.as_str(),
            "Complete" | "Failed" | "Recovered"
        )
    {
        return Some(RepairConflict::InactiveRecordingArtifact {
            artifact_id: manifest.artifact_id.clone(),
            meeting_status: db_artifact.meeting_status.clone(),
            session_status: db_artifact.session_status.clone(),
        });
    }
    None
}

fn parse_repair_status(status: &str) -> StoreResult<RepairStatus> {
    parse_repair_status_value(status).map_err(Into::into)
}

fn parse_write_status_value(status: &str) -> Result<WriteStatus, String> {
    match status {
        "Writing" => Ok(WriteStatus::Writing),
        "Complete" => Ok(WriteStatus::Complete),
        other => Err(format!("unknown write status: {other}")),
    }
}

fn parse_repair_status_value(status: &str) -> Result<RepairStatus, String> {
    match status {
        "NotNeeded" => Ok(RepairStatus::NotNeeded),
        "Recoverable" => Ok(RepairStatus::Recoverable),
        "Recovered" => Ok(RepairStatus::Recovered),
        other => Err(format!("unknown repair status: {other}")),
    }
}

fn parse_job_kind(kind: &str) -> StoreResult<JobKind> {
    match kind {
        "Transcribe" => Ok(JobKind::Transcribe),
        "Summarize" => Ok(JobKind::Summarize),
        "Export" => Ok(JobKind::Export),
        "Index" => Ok(JobKind::Index),
        other => Err(format!("unknown job kind: {other}").into()),
    }
}

fn parse_job_status(status: &str) -> StoreResult<JobStatus> {
    match status {
        "Queued" => Ok(JobStatus::Queued),
        "Running" => Ok(JobStatus::Running),
        "Succeeded" => Ok(JobStatus::Succeeded),
        "Failed" => Ok(JobStatus::Failed),
        "Canceled" => Ok(JobStatus::Canceled),
        "Retry" => Ok(JobStatus::Retry),
        "Recovery" => Ok(JobStatus::Recovery),
        other => Err(format!("unknown job status: {other}").into()),
    }
}

fn parse_source_channel(channel: &str) -> Result<SourceChannel, String> {
    match channel {
        "Microphone" => Ok(SourceChannel::Microphone),
        "System" => Ok(SourceChannel::System),
        "Mixed" => Ok(SourceChannel::Mixed),
        "Imported" => Ok(SourceChannel::Imported),
        other => Err(format!("unknown source channel: {other}")),
    }
}

fn transcription_artifact_kind_rank(kind: &str) -> u8 {
    match kind {
        "RawMic" => 0,
        "RawSystem" => 1,
        "Mixed" => 2,
        "Imported" => 3,
        _ => 4,
    }
}

fn transcription_artifacts_satisfy_recording_source(
    recording_source: &str,
    artifacts: &[TranscriptionAudioArtifact],
) -> bool {
    let kinds = artifacts
        .iter()
        .map(|artifact| artifact.kind.as_str())
        .collect::<Vec<_>>();
    artifact_kinds_satisfy_recording_source(recording_source, &kinds)
}

fn artifact_kinds_satisfy_recording_source(recording_source: &str, kinds: &[&str]) -> bool {
    let has_kind = |kind: &str| kinds.contains(&kind);
    match recording_source {
        "Mixed" => has_kind("Mixed") || (has_kind("RawMic") && has_kind("RawSystem")),
        "Microphone" => has_kind("RawMic"),
        "System" => has_kind("RawSystem"),
        "Imported" => has_kind("Imported"),
        _ => false,
    }
}

fn artifact_kind_required_for_recording_source(recording_source: &str, kind: &str) -> bool {
    match recording_source {
        "Mixed" => matches!(kind, "Mixed" | "RawMic" | "RawSystem"),
        "Microphone" => kind == "RawMic",
        "System" => kind == "RawSystem",
        "Imported" => kind == "Imported",
        _ => true,
    }
}
