use std::fs;
use std::path::PathBuf;

use curiosity_domain::{JobKind, JobStatus, Meeting, ProcessingJob};
use curiosity_store::Store;
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
