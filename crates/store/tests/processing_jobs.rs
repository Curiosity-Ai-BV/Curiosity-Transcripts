use std::fs;
use std::path::PathBuf;

use curiosity_domain::{JobKind, JobStatus, Meeting, ProcessingJob};
use curiosity_store::{Store, StoreResult};
use rusqlite::Connection;

fn test_root(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "curiosity-store-processing-jobs-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("test root");
    path
}

fn migrated_store_with_meeting(name: &str) -> Store {
    let root = test_root(name);
    let store = Store::open(root.join("app.db"), root).expect("open store");
    store.migrate().expect("migrate");
    store
        .insert_meeting(&Meeting::new_manual("meeting-1", "Planning", 1_000))
        .expect("insert meeting");
    store
}

fn terminal_transcription_job(id: &str, status: JobStatus) -> ProcessingJob {
    let mut job = ProcessingJob::new(id, "meeting-1", JobKind::Transcribe, status);
    job.attempts = 1;
    job.started_at_ms = Some(1_100);
    job.finished_at_ms = Some(1_900);
    job.last_error = Some("original terminal reason".to_string());
    job.cancel_requested = true;
    job.idempotency_key = Some("transcribe:meeting-1".to_string());
    job
}

fn assert_late_terminal_transition_is_rejected(
    store: &Store,
    job: ProcessingJob,
    transition: impl FnOnce(&Store) -> StoreResult<()>,
) {
    store
        .insert_processing_job(&job)
        .expect("insert terminal job");

    let error = transition(store).expect_err("late transition should reject terminal job");
    let persisted = store.processing_job(&job.id).expect("terminal job");

    assert!(
        error.to_string().contains("not running"),
        "late transition should explain that only running jobs are mutable: {error}"
    );
    assert_eq!(
        persisted, job,
        "late transition must not rewrite a terminal processing job"
    );
}

#[test]
fn processing_job_metadata_round_trips_for_later_worker_transitions() {
    let root = test_root("round-trip");
    let store = Store::open(root.join("app.db"), root).expect("open store");
    store.migrate().expect("migrate");
    store
        .insert_meeting(&Meeting::new_manual("meeting-1", "Planning", 1_000))
        .expect("insert meeting");

    let mut job = ProcessingJob::new(
        "job-1",
        "meeting-1",
        JobKind::Transcribe,
        JobStatus::Running,
    );
    job.attempts = 2;
    job.last_error = Some("whisper model missing".to_string());
    job.started_at_ms = Some(1_100);
    job.finished_at_ms = Some(1_900);
    job.cancel_requested = true;
    job.idempotency_key = Some("transcribe:meeting-1:artifact-sha".to_string());

    store.insert_processing_job(&job).expect("insert job");

    assert_eq!(store.processing_job("job-1").expect("processing job"), job);
}

#[test]
fn processing_job_lifecycle_mutators_update_worker_transition_fields() {
    let root = test_root("lifecycle-mutators");
    let store = Store::open(root.join("app.db"), root).expect("open store");
    store.migrate().expect("migrate");
    store
        .insert_meeting(&Meeting::new_manual("meeting-1", "Planning", 1_000))
        .expect("insert meeting");

    let mut job = ProcessingJob::new(
        "job-1",
        "meeting-1",
        JobKind::Transcribe,
        JobStatus::Running,
    );
    job.attempts = 1;
    job.started_at_ms = Some(1_100);
    job.idempotency_key = Some("transcribe:meeting-1".to_string());
    store.insert_processing_job(&job).expect("insert job");

    store
        .request_processing_job_cancel("job-1")
        .expect("request cancel");
    let canceled_requested = store.processing_job("job-1").expect("cancel-requested job");
    assert_eq!(canceled_requested.status, JobStatus::Running);
    assert!(canceled_requested.cancel_requested);

    store
        .complete_processing_job("job-1", 1_900)
        .expect("complete job");
    let completed = store.processing_job("job-1").expect("completed job");
    assert_eq!(completed.status, JobStatus::Succeeded);
    assert_eq!(completed.finished_at_ms, Some(1_900));
    assert!(!completed.cancel_requested);
    assert_eq!(completed.last_error, None);
}

#[test]
fn cancel_request_rejects_terminal_jobs_without_mutating_them() {
    let root = test_root("cancel-terminal");
    let store = Store::open(root.join("app.db"), root).expect("open store");
    store.migrate().expect("migrate");
    store
        .insert_meeting(&Meeting::new_manual("meeting-1", "Planning", 1_000))
        .expect("insert meeting");

    let job = ProcessingJob::new(
        "job-1",
        "meeting-1",
        JobKind::Transcribe,
        JobStatus::Running,
    );
    store.insert_processing_job(&job).expect("insert job");
    store
        .complete_processing_job("job-1", 1_900)
        .expect("complete job");

    let error = store
        .request_processing_job_cancel("job-1")
        .expect_err("terminal job cannot accept a late cancel request");
    let completed = store.processing_job("job-1").expect("completed job");

    assert!(
        error.to_string().contains("not running"),
        "late cancel should explain that only active jobs are mutable: {error}"
    );
    assert_eq!(completed.status, JobStatus::Succeeded);
    assert_eq!(completed.finished_at_ms, Some(1_900));
    assert!(!completed.cancel_requested);
}

#[test]
fn terminal_transcription_job_with_stale_cancel_flag_does_not_own_future_work() {
    let root = test_root("stale-terminal-cancel");
    let store = Store::open(root.join("app.db"), root).expect("open store");
    store.migrate().expect("migrate");
    store
        .insert_meeting(&Meeting::new_manual("meeting-1", "Planning", 1_000))
        .expect("insert meeting");

    let mut stale_terminal = ProcessingJob::new(
        "job-stale",
        "meeting-1",
        JobKind::Transcribe,
        JobStatus::Succeeded,
    );
    stale_terminal.started_at_ms = Some(1_100);
    stale_terminal.finished_at_ms = Some(1_900);
    stale_terminal.cancel_requested = true;
    store
        .insert_processing_job(&stale_terminal)
        .expect("insert stale terminal job");

    assert_eq!(
        store
            .active_transcription_job_for_meeting("meeting-1")
            .expect("active transcription query"),
        None,
        "terminal rows must not block a later transcription even if a stale cancel flag exists"
    );
}

#[test]
fn processing_job_failure_and_recovery_mutators_record_final_state() {
    let root = test_root("failure-recovery-mutators");
    let store = Store::open(root.join("app.db"), root).expect("open store");
    store.migrate().expect("migrate");
    store
        .insert_meeting(&Meeting::new_manual("meeting-1", "Planning", 1_000))
        .expect("insert meeting");

    let mut failed_job = ProcessingJob::new(
        "job-failed",
        "meeting-1",
        JobKind::Transcribe,
        JobStatus::Running,
    );
    failed_job.attempts = 1;
    store
        .insert_processing_job(&failed_job)
        .expect("insert failed job");
    store
        .fail_processing_job("job-failed", 1_900, "whisper backend failed")
        .expect("fail job");

    let failed = store.processing_job("job-failed").expect("failed job");
    assert_eq!(failed.status, JobStatus::Failed);
    assert_eq!(failed.finished_at_ms, Some(1_900));
    assert_eq!(failed.last_error.as_deref(), Some("whisper backend failed"));
    assert!(!failed.cancel_requested);

    let recovery_job = ProcessingJob::new(
        "job-recovery",
        "meeting-1",
        JobKind::Transcribe,
        JobStatus::Running,
    );
    store
        .insert_processing_job(&recovery_job)
        .expect("insert recovery job");
    store
        .recover_processing_job("job-recovery", 2_100, "worker missing after restart")
        .expect("recover job");

    let recovered = store.processing_job("job-recovery").expect("recovered job");
    assert_eq!(recovered.status, JobStatus::Recovery);
    assert_eq!(recovered.finished_at_ms, Some(2_100));
    assert_eq!(
        recovered.last_error.as_deref(),
        Some("worker missing after restart")
    );
    assert!(!recovered.cancel_requested);
}

#[test]
fn processing_job_complete_rejects_succeeded_and_recovery_jobs_without_rewriting_them() {
    let store = migrated_store_with_meeting("complete-terminal-rejects");

    assert_late_terminal_transition_is_rejected(
        &store,
        terminal_transcription_job("job-succeeded", JobStatus::Succeeded),
        |store| store.complete_processing_job("job-succeeded", 2_500),
    );
    assert_late_terminal_transition_is_rejected(
        &store,
        terminal_transcription_job("job-recovery", JobStatus::Recovery),
        |store| store.complete_processing_job("job-recovery", 2_600),
    );
}

#[test]
fn processing_job_cancel_rejects_succeeded_and_recovery_jobs_without_rewriting_them() {
    let store = migrated_store_with_meeting("cancel-terminal-rejects");

    assert_late_terminal_transition_is_rejected(
        &store,
        terminal_transcription_job("job-succeeded", JobStatus::Succeeded),
        |store| store.cancel_processing_job("job-succeeded", 2_500),
    );
    assert_late_terminal_transition_is_rejected(
        &store,
        terminal_transcription_job("job-recovery", JobStatus::Recovery),
        |store| store.cancel_processing_job("job-recovery", 2_600),
    );
}

#[test]
fn processing_job_fail_rejects_succeeded_job_without_rewriting_it() {
    let store = migrated_store_with_meeting("fail-terminal-rejects");

    assert_late_terminal_transition_is_rejected(
        &store,
        terminal_transcription_job("job-succeeded", JobStatus::Succeeded),
        |store| store.fail_processing_job("job-succeeded", 2_500, "late worker failure"),
    );
}

#[test]
fn processing_job_recovery_rejects_succeeded_job_without_rewriting_it() {
    let store = migrated_store_with_meeting("recovery-terminal-rejects");

    assert_late_terminal_transition_is_rejected(
        &store,
        terminal_transcription_job("job-succeeded", JobStatus::Succeeded),
        |store| {
            store.recover_processing_job(
                "job-succeeded",
                2_500,
                "transcription worker was not running after app restart",
            )
        },
    );
}

#[test]
fn recover_active_transcription_jobs_only_recovers_transcription_workers_without_runtime_owner() {
    let root = test_root("recover-active-transcription");
    let store = Store::open(root.join("app.db"), root).expect("open store");
    store.migrate().expect("migrate");
    store
        .insert_meeting(&Meeting::new_manual("meeting-1", "Planning", 1_000))
        .expect("insert meeting");

    let mut transcription_job = ProcessingJob::new(
        "job-transcribe",
        "meeting-1",
        JobKind::Transcribe,
        JobStatus::Running,
    );
    transcription_job.started_at_ms = Some(1_100);
    store
        .insert_processing_job(&transcription_job)
        .expect("insert transcription job");
    let summary_job = ProcessingJob::new(
        "job-summary",
        "meeting-1",
        JobKind::Summarize,
        JobStatus::Running,
    );
    store
        .insert_processing_job(&summary_job)
        .expect("insert summary job");

    let recovered = store
        .recover_active_transcription_jobs(2_100, "worker missing after restart")
        .expect("recover active transcription jobs");

    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].id, "job-transcribe");
    assert_eq!(recovered[0].status, JobStatus::Recovery);
    assert_eq!(recovered[0].finished_at_ms, Some(2_100));
    assert_eq!(
        recovered[0].last_error.as_deref(),
        Some("worker missing after restart")
    );
    assert_eq!(
        store
            .processing_job("job-summary")
            .expect("summary job")
            .status,
        JobStatus::Running
    );
}

#[test]
fn fresh_migration_creates_processing_job_metadata_columns() {
    let root = test_root("fresh-columns");
    let db_path = root.join("app.db");
    let store = Store::open(&db_path, root).expect("open store");
    store.migrate().expect("migrate");

    assert_eq!(store.schema_version().expect("schema version"), 3);

    let conn = Connection::open(&db_path).expect("read db");
    let columns = processing_job_columns(&conn);
    assert!(columns.contains(&"started_at_ms".to_string()));
    assert!(columns.contains(&"finished_at_ms".to_string()));
    assert!(columns.contains(&"cancel_requested".to_string()));
    assert!(columns.contains(&"idempotency_key".to_string()));
}

#[test]
fn migration_preserves_legacy_processing_jobs_with_default_metadata() {
    let root = test_root("legacy");
    let db_path = root.join("app.db");
    {
        let conn = Connection::open(&db_path).expect("legacy db");
        conn.execute_batch(
            "
            CREATE TABLE processing_jobs (
                id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                status TEXT NOT NULL,
                attempts INTEGER NOT NULL,
                last_error TEXT
            );
            INSERT INTO processing_jobs (
                id, meeting_id, kind, status, attempts, last_error
            ) VALUES (
                'job-legacy', 'meeting-1', 'Summarize', 'Retry', 3, 'network timeout'
            );
            PRAGMA user_version = 2;
            ",
        )
        .expect("legacy schema");
    }

    let store = Store::open(&db_path, root).expect("open store");
    store.migrate().expect("migrate legacy schema");

    assert_eq!(store.schema_version().expect("schema version"), 3);
    let job = store
        .processing_job("job-legacy")
        .expect("legacy processing job");
    assert_eq!(
        job,
        ProcessingJob {
            id: "job-legacy".to_string(),
            meeting_id: "meeting-1".to_string(),
            kind: JobKind::Summarize,
            status: JobStatus::Retry,
            attempts: 3,
            last_error: Some("network timeout".to_string()),
            started_at_ms: None,
            finished_at_ms: None,
            cancel_requested: false,
            idempotency_key: None,
        }
    );
}

fn processing_job_columns(conn: &Connection) -> Vec<String> {
    conn.prepare("PRAGMA table_info(processing_jobs)")
        .expect("table info")
        .query_map([], |row| row.get::<_, String>(1))
        .expect("columns")
        .collect::<Result<Vec<_>, _>>()
        .expect("column names")
}
