use std::fs;
use std::path::{Component, Path, PathBuf};

use curiosity_domain::{
    AudioArtifact, JobStatus, Meeting, MeetingStatus, ProcessingJob, RecordingSession,
    RecordingStatus,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

pub type StoreResult<T> = Result<T, Box<dyn std::error::Error>>;

pub struct Store {
    conn: Connection,
    app_root: PathBuf,
}

impl Store {
    pub fn open(db_path: impl AsRef<Path>, app_root: impl Into<PathBuf>) -> StoreResult<Self> {
        let db_path = db_path.as_ref();
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(Self {
            conn: Connection::open(db_path)?,
            app_root: app_root.into(),
        })
    }

    pub fn migrate(&self) -> StoreResult<()> {
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

            CREATE TABLE IF NOT EXISTS processing_jobs (
                id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                status TEXT NOT NULL,
                attempts INTEGER NOT NULL,
                last_error TEXT
            );

            CREATE TABLE IF NOT EXISTS exported_files (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                meeting_id TEXT NOT NULL,
                path TEXT NOT NULL
            );
            ",
        )?;
        Ok(())
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

    pub fn insert_audio_artifact(&self, artifact: &AudioArtifact) -> StoreResult<()> {
        self.conn.execute(
            "
            INSERT INTO audio_artifacts (
                id, recording_session_id, kind, path, sha256, retained
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ",
            params![
                artifact.id,
                artifact.recording_session_id,
                enum_name(artifact.kind),
                artifact.path,
                artifact.sha256,
                artifact.retained
            ],
        )?;
        Ok(())
    }

    pub fn insert_processing_job(&self, job: &ProcessingJob) -> StoreResult<()> {
        self.conn.execute(
            "
            INSERT INTO processing_jobs (
                id, meeting_id, kind, status, attempts, last_error
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ",
            params![
                job.id,
                job.meeting_id,
                enum_name(job.kind),
                enum_name(job.status),
                job.attempts,
                job.last_error
            ],
        )?;
        Ok(())
    }

    pub fn count(&self, table: &str) -> StoreResult<u64> {
        let allowed = [
            "meetings",
            "recording_sessions",
            "audio_artifacts",
            "processing_jobs",
            "exported_files",
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

    pub fn recording_session_ended_at_ms(
        &self,
        recording_id: &str,
    ) -> StoreResult<Option<u64>> {
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
            let Some(db_artifact) = self.db_artifact_for_repair(&manifest.artifact_id)? else {
                report.conflicts.push(RepairConflict::MissingArtifact {
                    artifact_id: manifest.artifact_id,
                });
                continue;
            };
            if let Some(conflict) = repair_conflict(&manifest, &db_artifact) {
                report.conflicts.push(conflict);
                continue;
            }
            let Some(artifact_path) = self.private_app_path(&manifest.path) else {
                report.conflicts.push(RepairConflict::UnsafePath {
                    artifact_id: manifest.artifact_id,
                    path: manifest.path,
                });
                continue;
            };
            if !artifact_path.exists() {
                continue;
            }
            self.conn.execute(
                "
                UPDATE audio_artifacts
                SET write_status = 'Complete', recovery_status = 'Recovered'
                WHERE id = ?1
                ",
                params![manifest.artifact_id],
            )?;
            if self.conn.changes() > 0 {
                report.recovered_artifacts.push(manifest.artifact_id.clone());
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
            for job_id in job_ids {
                self.conn.execute(
                    "UPDATE processing_jobs SET status = 'Recovery' WHERE id = ?1",
                    params![job_id],
                )?;
                report.recovered_jobs.push(job_id);
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

    pub fn record_exported_file(&self, meeting_id: &str, path: &Path) -> StoreResult<()> {
        self.conn.execute(
            "INSERT INTO exported_files (meeting_id, path) VALUES (?1, ?2)",
            params![meeting_id, path.to_string_lossy()],
        )?;
        Ok(())
    }

    pub fn delete_meeting(&self, meeting_id: &str) -> StoreResult<DeleteReport> {
        let mut report = DeleteReport::default();
        let mut stmt = self.conn.prepare(
            "
            SELECT audio_artifacts.id, audio_artifacts.path
            FROM audio_artifacts
            JOIN recording_sessions
              ON recording_sessions.id = audio_artifacts.recording_session_id
            WHERE recording_sessions.meeting_id = ?1
              AND audio_artifacts.tombstoned = 0
            ",
        )?;
        let artifacts = stmt
            .query_map(params![meeting_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (artifact_id, artifact_path) in artifacts {
            let Some(path) = self.private_app_path(&artifact_path) else {
                report.skipped_private_artifacts.push(self.reported_path(&artifact_path));
                continue;
            };
            if path.exists() {
                fs::remove_file(&path)?;
                report.deleted_private_artifacts.push(path);
            }
            self.conn.execute(
                "UPDATE audio_artifacts SET retained = 0, tombstoned = 1 WHERE id = ?1",
                params![artifact_id],
            )?;
        }

        let mut exports = self
            .conn
            .prepare("SELECT path FROM exported_files WHERE meeting_id = ?1 ORDER BY path")?;
        report.exported_files_outside_app_control = exports
            .query_map(params![meeting_id], |row| {
                Ok(PathBuf::from(row.get::<_, String>(0)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        self.conn.execute(
            "UPDATE meetings SET status = 'Deleted', deleted_at_ms = COALESCE(deleted_at_ms, 0) WHERE id = ?1",
            params![meeting_id],
        )?;
        report.deleted_private_artifacts.sort();
        report.skipped_private_artifacts.sort();
        Ok(report)
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
        let (retained, tombstoned): (u8, u8) = self.conn.query_row(
            "SELECT retained, tombstoned FROM audio_artifacts WHERE id = ?1",
            params![artifact_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        Ok(retained == 0 && tombstoned == 1)
    }

    fn db_artifact_for_repair(&self, artifact_id: &str) -> StoreResult<Option<DbArtifactForRepair>> {
        self.conn
            .query_row(
                "
                SELECT
                    audio_artifacts.recording_session_id,
                    recording_sessions.meeting_id,
                    audio_artifacts.path,
                    audio_artifacts.sha256,
                    audio_artifacts.write_status,
                    audio_artifacts.recovery_status
                FROM audio_artifacts
                JOIN recording_sessions
                  ON recording_sessions.id = audio_artifacts.recording_session_id
                WHERE audio_artifacts.id = ?1
                ",
                params![artifact_id],
                |row| {
                    Ok(DbArtifactForRepair {
                        session_id: row.get(0)?,
                        meeting_id: row.get(1)?,
                        path: row.get(2)?,
                        sha256: row.get(3)?,
                        write_status: row.get(4)?,
                        recovery_status: row.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    fn private_app_path(&self, path: &str) -> Option<PathBuf> {
        let path = PathBuf::from(path);
        if path.is_absolute() || path.components().any(|component| component == Component::ParentDir)
        {
            return None;
        }
        let candidate = self.app_root.join(path);
        let app_root = self.app_root.canonicalize().ok()?;
        let canonical_check_path = nearest_existing_path(&candidate)?;
        let canonical_check_path = canonical_check_path.canonicalize().ok()?;
        if canonical_check_path.starts_with(app_root) {
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WriteStatus {
    Writing,
    Complete,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum RepairStatus {
    NotNeeded,
    Recoverable,
    Recovered,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArtifactManifest {
    pub meeting_id: String,
    pub session_id: String,
    pub artifact_id: String,
    pub path: String,
    pub sha256: String,
    pub write_status: WriteStatus,
    pub recovery_status: RepairStatus,
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
            session_id: session_id.to_string(),
            artifact_id: artifact_id.to_string(),
            path: path.to_string(),
            sha256: sha256.to_string(),
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
        fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    pub fn read(path: impl AsRef<Path>) -> StoreResult<Self> {
        Ok(serde_json::from_slice(&fs::read(path)?)?)
    }
}

fn manifest_paths(root: &Path) -> StoreResult<Vec<PathBuf>> {
    let meetings = root.join("meetings");
    if !meetings.exists() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in fs::read_dir(meetings)? {
        let entry = entry?;
        let path = entry.path().join("manifest.json");
        if path.exists() {
            paths.push(path);
        }
    }
    Ok(paths)
}

fn enum_name<T: std::fmt::Debug>(value: T) -> String {
    format!("{value:?}")
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
    path: String,
    sha256: String,
    write_status: String,
    recovery_status: String,
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
            manifest_status: manifest_write_status,
            db_status: db_artifact.write_status.clone(),
        });
    }
    let manifest_recovery_status = enum_name(manifest.recovery_status);
    if db_artifact.recovery_status != "NotNeeded" && manifest_recovery_status != db_artifact.recovery_status {
        return Some(RepairConflict::MismatchedRecoveryStatus {
            artifact_id: manifest.artifact_id.clone(),
            manifest_status: manifest_recovery_status,
            db_status: db_artifact.recovery_status.clone(),
        });
    }
    None
}

fn parse_repair_status(status: &str) -> StoreResult<RepairStatus> {
    match status {
        "NotNeeded" => Ok(RepairStatus::NotNeeded),
        "Recoverable" => Ok(RepairStatus::Recoverable),
        "Recovered" => Ok(RepairStatus::Recovered),
        other => Err(format!("unknown repair status: {other}").into()),
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
