use curiosity_domain::{JobKind, JobStatus, ProcessingJob};
use rusqlite::{params, OptionalExtension};

use super::{enum_name, parse_job_kind, parse_job_status, Store, StoreResult};

type ProcessingJobRow = (
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
);

impl Store {
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
        ): ProcessingJobRow = match self
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
}
