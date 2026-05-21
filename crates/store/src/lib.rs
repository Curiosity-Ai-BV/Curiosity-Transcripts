use std::fs;
use std::path::{Component, Path, PathBuf};

use curiosity_domain::{
    AudioArtifact, JobStatus, Meeting, MeetingStatus, ModelRun, ProcessingJob, RecordingSession,
    RecordingStatus, SourceChannel, TranscriptSegment, TranscriptVersion,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

pub type StoreResult<T> = Result<T, Box<dyn std::error::Error>>;

pub struct Store {
    conn: Connection,
    app_root: PathBuf,
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

            CREATE UNIQUE INDEX IF NOT EXISTS audio_artifacts_import_identity_idx
            ON audio_artifacts(recording_session_id, kind, sha256);

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

            CREATE VIRTUAL TABLE IF NOT EXISTS meeting_search
            USING fts5(meeting_id UNINDEXED, title, transcript_text);
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

    pub fn insert_audio_artifact(&self, artifact: &AudioArtifact) -> StoreResult<String> {
        let kind = enum_name(artifact.kind);
        if let Some(existing_id) = self.audio_artifact_id_by_import_identity(
            &artifact.recording_session_id,
            &kind,
            &artifact.sha256,
        )? {
            return Ok(existing_id);
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
            "model_runs",
            "transcript_versions",
            "transcript_segments",
            "transcript_segment_edits",
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
        if !is_safe_meeting_id(meeting_id) {
            return Err(
                format!("meeting id is not safe for private storage: {meeting_id}").into(),
            );
        }
        if self.private_app_path(artifact_path).is_none() {
            return Err(
                format!("artifact path is not safe for private storage: {artifact_path}").into(),
            );
        }
        let manifest_path = self
            .app_root
            .join("meetings")
            .join(meeting_id)
            .join("manifest.json");
        ArtifactManifest::new(meeting_id, session_id, artifact_id, artifact_path, sha256)
            .mark_interrupted_recoverable()
            .write(manifest_path)
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
        self.rebuild_search_index()?;
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
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
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

    pub fn export_meeting_json(&self, meeting_id: &str, export_root: &Path) -> StoreResult<PathBuf> {
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
        let mut report = DeleteReport::default();

        let mut exports = self
            .conn
            .prepare("SELECT path FROM exported_files WHERE meeting_id = ?1 ORDER BY path")?;
        report.exported_files_outside_app_control = exports
            .query_map(params![meeting_id], |row| {
                Ok(PathBuf::from(row.get::<_, String>(0)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(exports);

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

        for artifact_path in self.private_artifacts_for_delete(meeting_id)? {
            let Some(path) = self.private_app_path(&artifact_path) else {
                report.skipped_private_artifacts.push(self.reported_path(&artifact_path));
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
        let transcript_version_id =
            match self.transcript_version_by_identity(&version.meeting_id, &model_run_id, version.version)? {
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

    fn transcript_segments_for_version(&self, version_id: &str) -> StoreResult<Vec<TranscriptSegment>> {
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

    pub fn transcript_segment_edits(&self, segment_id: &str) -> StoreResult<Vec<TranscriptSegmentEdit>> {
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

    pub fn correct_transcript_segment(
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
            params![segment_id, version_id, edited_at_ms, previous_text, corrected_text],
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
        for corrected_text in edits.query_map(params![version_id], |row| row.get::<_, String>(0))?
        {
            parts.push(corrected_text?);
        }
        Ok(parts.join("\n"))
    }

    fn update_manifest_title(&self, meeting_id: &str, title: &str) -> StoreResult<()> {
        let mut staged = Vec::new();
        let mut originals = Vec::new();
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
                for (_, temp_path) in staged {
                    let _ = fs::remove_file(temp_path);
                }
                return Err(format!(
                    "manifest title update failed for {}: {err}",
                    manifest_path.display()
                )
                .into());
            }
            originals.push((manifest_path.clone(), original_bytes));
            staged.push((manifest_path, temp_path));
        }

        let mut replaced = Vec::new();
        for (manifest_path, temp_path) in staged {
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
            if let Some((_, original_bytes)) = originals
                .iter()
                .find(|(original_path, _)| original_path == &manifest_path)
            {
                replaced.push((manifest_path, original_bytes.clone()));
            }
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
                source_channel: enum_name(segment.source_channel),
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

    fn delete_private_manifests(&self, meeting_id: &str) -> StoreResult<()> {
        for manifest_path in manifest_paths(&self.app_root)? {
            let manifest = ArtifactManifest::read(&manifest_path)?;
            if manifest.meeting_id == meeting_id {
                let Some(relative_path) = manifest_path.strip_prefix(&self.app_root).ok() else {
                    continue;
                };
                let Some(safe_path) = self.private_app_path(&relative_path.to_string_lossy()) else {
                    continue;
                };
                if safe_path.exists() {
                    fs::remove_file(safe_path)?;
                }
            }
        }
        Ok(())
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
    #[serde(default)]
    pub meeting_title: Option<String>,
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
            meeting_title: None,
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
        let temp_path = path.with_extension("json.tmp");
        fs::write(&temp_path, serde_json::to_vec_pretty(self)?)?;
        fs::rename(temp_path, path)?;
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

fn safe_export_filename(meeting_id: &str) -> StoreResult<String> {
    if !is_safe_meeting_id(meeting_id) {
        return Err(format!("meeting id is not a safe export filename: {meeting_id}").into());
    }
    Ok(format!("{meeting_id}.json"))
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
    path.with_extension(format!(
        "json.rename-tmp-{}",
        std::process::id()
    ))
}

fn fts_query(query: &str) -> StoreResult<String> {
    let tokens = query
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| format!("\"{}\"", token.replace('"', "\"\"")))
        .collect::<Vec<_>>();
    Ok(tokens.join(" "))
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

fn parse_source_channel(channel: &str) -> Result<SourceChannel, String> {
    match channel {
        "Microphone" => Ok(SourceChannel::Microphone),
        "System" => Ok(SourceChannel::System),
        "Mixed" => Ok(SourceChannel::Mixed),
        "Imported" => Ok(SourceChannel::Imported),
        other => Err(format!("unknown source channel: {other}")),
    }
}
