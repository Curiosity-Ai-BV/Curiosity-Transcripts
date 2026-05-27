use std::fs;
use std::path::{Path, PathBuf};

use curiosity_domain::{
    ArtifactKind, AudioArtifact, JobKind, JobStatus, Meeting, MeetingStatus, RecordingSession,
    RecordingSource, RecordingStatus,
};
use curiosity_store::{
    ArtifactManifest, DeleteReport, RecoverableArtifact, RepairConflict, RepairStatus, Store,
    WriteStatus,
};
use rusqlite::Connection;

fn test_root(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("curiosity-store-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("test root");
    path
}

#[test]
fn migrate_creates_local_sqlite_db_and_persists_core_loop_rows() {
    let root = test_root("migrate");
    let store = Store::open(root.join("app.db"), root.clone()).expect("open store");
    store.migrate().expect("migrate");

    let meeting = Meeting::new_manual("meeting-1", "Planning", 1_000);
    let session = RecordingSession::start(
        "session-1",
        meeting.id.clone(),
        RecordingSource::Microphone,
        1_010,
        48_000,
    );
    let artifact = AudioArtifact::new_private(
        "artifact-1",
        "session-1",
        ArtifactKind::RawMic,
        "meetings/meeting-1/audio/raw-mic.wav",
        "sha256:abc",
    );
    let job = curiosity_domain::ProcessingJob::new(
        "job-1",
        meeting.id.clone(),
        JobKind::Transcribe,
        JobStatus::Queued,
    );

    store.insert_meeting(&meeting).expect("insert meeting");
    store
        .insert_recording_session(&session)
        .expect("insert session");
    store
        .insert_audio_artifact(&artifact)
        .expect("insert artifact");
    store.insert_processing_job(&job).expect("insert job");

    assert_eq!(store.count("meetings").expect("meetings"), 1);
    assert_eq!(store.count("recording_sessions").expect("sessions"), 1);
    assert_eq!(store.count("audio_artifacts").expect("artifacts"), 1);
    assert_eq!(store.count("processing_jobs").expect("jobs"), 1);
}

#[test]
fn migrate_upgrades_legacy_audio_artifact_columns_and_sets_schema_version() {
    let root = test_root("legacy-migrate");
    let db_path = root.join("app.db");
    {
        let conn = Connection::open(&db_path).expect("legacy db");
        conn.execute_batch(
            "
            CREATE TABLE audio_artifacts (
                id TEXT PRIMARY KEY,
                recording_session_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                path TEXT NOT NULL,
                sha256 TEXT NOT NULL,
                retained INTEGER NOT NULL
            );
            PRAGMA user_version = 0;
            ",
        )
        .expect("legacy schema");
    }

    let store = Store::open(&db_path, root.clone()).expect("open store");
    store.migrate().expect("migrate legacy schema");

    assert_eq!(store.schema_version().expect("schema version"), 2);
    let conn = Connection::open(&db_path).expect("read migrated db");
    let columns = conn
        .prepare("PRAGMA table_info(audio_artifacts)")
        .expect("table info")
        .query_map([], |row| row.get::<_, String>(1))
        .expect("columns")
        .collect::<Result<Vec<_>, _>>()
        .expect("column names");
    assert!(columns.contains(&"write_status".to_string()));
    assert!(columns.contains(&"recovery_status".to_string()));
    assert!(columns.contains(&"tombstoned".to_string()));
}

#[test]
fn pending_artifact_hashes_fail_loud_instead_of_deduping_to_first_row() {
    let root = test_root("pending-hash-dedupe");
    let store = Store::open(root.join("app.db"), root.clone()).expect("open store");
    store.migrate().expect("migrate");
    seed_meeting_session(&store, "meeting-1", "session-1");

    let first = AudioArtifact::new_private(
        "artifact-1",
        "session-1",
        ArtifactKind::Mixed,
        "meetings/meeting-1/audio/mixed-1.pcm",
        "sha256:pending",
    );
    let second = AudioArtifact::new_private(
        "artifact-2",
        "session-1",
        ArtifactKind::Mixed,
        "meetings/meeting-1/audio/mixed-2.pcm",
        "sha256:pending",
    );

    assert_eq!(
        store
            .insert_audio_artifact(&first)
            .expect("first pending insert"),
        "artifact-1"
    );
    let err = store
        .insert_audio_artifact(&second)
        .expect_err("same pending sentinel should not silently return the first artifact id");
    assert!(
        err.to_string().to_ascii_lowercase().contains("unique"),
        "unexpected pending sentinel failure: {err}"
    );
}

#[test]
fn artifact_manifest_json_uses_explicit_stable_status_strings() {
    let manifest = ArtifactManifest::new(
        "meeting-1",
        "session-1",
        "artifact-1",
        "meetings/meeting-1/audio/raw-mic.wav",
        "sha256:partial",
    )
    .mark_interrupted_recoverable();

    let json = serde_json::to_string(&manifest).expect("serialize manifest");

    assert!(json.contains(r#""write_status":"Writing""#));
    assert!(json.contains(r#""recovery_status":"Recoverable""#));
    let parsed: ArtifactManifest = serde_json::from_str(&json).expect("parse manifest");
    assert_eq!(parsed.write_status, WriteStatus::Writing);
    assert_eq!(parsed.recovery_status, RepairStatus::Recoverable);
}

#[test]
fn startup_repair_reconciles_incomplete_db_rows_with_artifact_manifests_after_crash() {
    let root = test_root("repair");
    let store = Store::open(root.join("app.db"), root.clone()).expect("open store");
    store.migrate().expect("migrate");
    seed_crashed_meeting(&store, &root);

    let report = store.repair_startup().expect("repair startup");

    assert_eq!(report.recovered_artifacts, vec!["artifact-1"]);
    assert_eq!(report.recovered_jobs, vec!["job-1"]);
    assert_eq!(
        store
            .artifact_recovery_status("artifact-1")
            .expect("artifact status"),
        RepairStatus::Recovered
    );
    assert_eq!(
        store.job_status("job-1").expect("job status"),
        JobStatus::Recovery
    );
    assert_eq!(
        store
            .recording_session_status("session-1")
            .expect("session status"),
        "Recovered"
    );
    assert_eq!(
        store.meeting_status("meeting-1").expect("meeting status"),
        "Recovered"
    );
}

#[test]
fn startup_repair_skips_deleted_tombstoned_artifact_manifest_without_recovering_rows_or_jobs() {
    let root = test_root("repair-deleted-tombstoned");
    let db_path = root.join("app.db");
    let store = Store::open(&db_path, root.clone()).expect("open store");
    store.migrate().expect("migrate");
    seed_crashed_meeting(&store, &root);

    store
        .tombstone_audio_artifact("artifact-1")
        .expect("tombstone artifact");
    {
        let conn = Connection::open(&db_path).expect("delete intent db connection");
        conn.execute(
            "UPDATE meetings SET status = 'Deleted', deleted_at_ms = 1234 WHERE id = ?1",
            ["meeting-1"],
        )
        .expect("mark meeting deleted");
    }

    let report = store.repair_startup().expect("repair startup");

    assert!(report.recovered_artifacts.is_empty(), "{report:?}");
    assert!(report.recovered_jobs.is_empty(), "{report:?}");
    assert_eq!(
        report.conflicts,
        vec![RepairConflict::DeletedOrTombstonedArtifact {
            artifact_id: "artifact-1".to_string(),
        }]
    );
    assert_eq!(
        store
            .artifact_recovery_status("artifact-1")
            .expect("artifact status"),
        RepairStatus::NotNeeded
    );
    assert_eq!(
        store.job_status("job-1").expect("job status"),
        JobStatus::Running
    );
    assert_eq!(
        store
            .recording_session_status("session-1")
            .expect("session status"),
        "Recording"
    );
    assert_eq!(
        store.meeting_status("meeting-1").expect("meeting status"),
        "Deleted"
    );
    assert!(store
        .artifact_tombstoned("artifact-1")
        .expect("artifact remains tombstoned"));
}

#[test]
fn startup_repair_skips_failed_recording_manifest_without_recovering_rows_or_jobs() {
    let root = test_root("repair-failed-recording");
    let store = Store::open(root.join("app.db"), root.clone()).expect("open store");
    store.migrate().expect("migrate");
    seed_crashed_meeting(&store, &root);
    store
        .update_recording_session_status(
            "session-1",
            RecordingStatus::Failed,
            Some(2_000),
            Some("stop failed"),
        )
        .expect("mark session failed");
    store
        .update_meeting_status("meeting-1", MeetingStatus::Failed, Some(2_000))
        .expect("mark meeting failed");

    let report = store.repair_startup().expect("repair startup");

    assert!(report.recovered_artifacts.is_empty(), "{report:?}");
    assert!(report.recovered_jobs.is_empty(), "{report:?}");
    assert_eq!(
        report.conflicts,
        vec![RepairConflict::InactiveRecordingArtifact {
            artifact_id: "artifact-1".to_string(),
            meeting_status: "Failed".to_string(),
            session_status: "Failed".to_string(),
        }]
    );
    assert_eq!(
        store
            .artifact_recovery_status("artifact-1")
            .expect("artifact status"),
        RepairStatus::NotNeeded
    );
    assert_eq!(
        store.job_status("job-1").expect("job status"),
        JobStatus::Running
    );
    assert_eq!(
        store
            .recording_session_status("session-1")
            .expect("session status"),
        "Failed"
    );
    assert_eq!(
        store.meeting_status("meeting-1").expect("meeting status"),
        "Failed"
    );
}

#[test]
fn delete_meeting_removes_private_artifacts_but_reports_exports_outside_app_control() {
    let root = test_root("delete");
    let export_root = test_root("delete-export");
    let store = Store::open(root.join("app.db"), root.clone()).expect("open store");
    store.migrate().expect("migrate");

    let meeting = Meeting::new_manual("meeting-1", "Planning", 1_000);
    store.insert_meeting(&meeting).expect("insert meeting");
    let session = RecordingSession::start(
        "session-1",
        meeting.id.clone(),
        RecordingSource::Microphone,
        1_010,
        48_000,
    );
    store
        .insert_recording_session(&session)
        .expect("insert session");
    let private_path = root.join("meetings/meeting-1/audio/raw-mic.wav");
    fs::create_dir_all(private_path.parent().expect("parent")).expect("private dirs");
    fs::write(&private_path, b"private audio").expect("private file");
    let artifact = AudioArtifact::new_private(
        "artifact-1",
        "session-1",
        ArtifactKind::RawMic,
        private_path
            .strip_prefix(&root)
            .expect("relative")
            .to_string_lossy(),
        "sha256:abc",
    );
    store
        .insert_audio_artifact(&artifact)
        .expect("insert private artifact");

    let exported_path = export_root.join("meeting.md");
    fs::write(&exported_path, b"# exported transcript").expect("export file");
    store
        .record_exported_file("meeting-1", &exported_path)
        .expect("record export");

    let other_meeting = Meeting::new_manual("meeting-2", "Design review", 2_000);
    store
        .insert_meeting(&other_meeting)
        .expect("insert other meeting");
    let other_session = RecordingSession::start(
        "session-2",
        other_meeting.id.clone(),
        RecordingSource::Microphone,
        2_010,
        48_000,
    );
    store
        .insert_recording_session(&other_session)
        .expect("insert other session");
    let other_private_path = root.join("meetings/meeting-2/audio/raw-mic.wav");
    fs::create_dir_all(other_private_path.parent().expect("parent")).expect("other private dirs");
    fs::write(&other_private_path, b"other private audio").expect("other private file");
    let other_artifact = AudioArtifact::new_private(
        "artifact-2",
        "session-2",
        ArtifactKind::RawMic,
        other_private_path
            .strip_prefix(&root)
            .expect("relative")
            .to_string_lossy(),
        "sha256:def",
    );
    store
        .insert_audio_artifact(&other_artifact)
        .expect("insert other private artifact");

    let report: DeleteReport = store.delete_meeting("meeting-1").expect("delete meeting");

    assert_eq!(report.deleted_private_artifacts, vec![private_path.clone()]);
    assert_eq!(
        report.exported_files_outside_app_control,
        vec![exported_path.clone()]
    );
    assert!(!private_path.exists());
    assert!(other_private_path.exists());
    assert!(exported_path.exists());
    assert!(store.meeting_deleted("meeting-1").expect("meeting deleted"));
    assert!(!store
        .meeting_deleted("meeting-2")
        .expect("other meeting not deleted"));
}

#[test]
fn delete_meeting_skips_absolute_artifact_paths_outside_app_storage() {
    let root = test_root("delete-absolute");
    let outside_root = test_root("delete-absolute-outside");
    let store = Store::open(root.join("app.db"), root.clone()).expect("open store");
    store.migrate().expect("migrate");
    seed_meeting_session(&store, "meeting-1", "session-1");

    let outside_path = outside_root.join("user-owned.wav");
    fs::write(&outside_path, b"user owned").expect("outside file");
    let artifact = AudioArtifact::new_private(
        "artifact-absolute",
        "session-1",
        ArtifactKind::RawMic,
        outside_path.to_string_lossy(),
        "sha256:absolute",
    );
    store
        .insert_audio_artifact(&artifact)
        .expect("insert artifact");

    let report = store.delete_meeting("meeting-1").expect("delete meeting");

    assert!(report.deleted_private_artifacts.is_empty());
    assert_eq!(report.skipped_private_artifacts, vec![outside_path.clone()]);
    assert!(outside_path.exists());
}

#[test]
fn delete_meeting_skips_relative_artifact_paths_that_escape_app_storage() {
    let root = test_root("delete-relative-escape");
    let store = Store::open(root.join("app.db"), root.clone()).expect("open store");
    store.migrate().expect("migrate");
    seed_meeting_session(&store, "meeting-1", "session-1");

    let outside_path = root.parent().expect("temp parent").join(format!(
        "curiosity-store-escaped-{}.wav",
        std::process::id()
    ));
    let _ = fs::remove_file(&outside_path);
    fs::write(&outside_path, b"user owned").expect("outside file");
    let escaped_db_path = PathBuf::from("..").join(outside_path.file_name().expect("file name"));
    let expected_skipped = root.join(&escaped_db_path);
    let artifact = AudioArtifact::new_private(
        "artifact-relative-escape",
        "session-1",
        ArtifactKind::RawMic,
        escaped_db_path.to_string_lossy(),
        "sha256:escape",
    );
    store
        .insert_audio_artifact(&artifact)
        .expect("insert artifact");

    let report = store.delete_meeting("meeting-1").expect("delete meeting");

    assert!(report.deleted_private_artifacts.is_empty());
    assert_eq!(report.skipped_private_artifacts, vec![expected_skipped]);
    assert!(outside_path.exists());
    let _ = fs::remove_file(&outside_path);
}

#[test]
fn delete_meeting_tombstones_safe_app_artifact_rows_when_file_is_already_missing() {
    let root = test_root("delete-missing-private-file");
    let store = Store::open(root.join("app.db"), root.clone()).expect("open store");
    store.migrate().expect("migrate");
    seed_meeting_session(&store, "meeting-1", "session-1");

    let missing_relative_path = "meetings/meeting-1/audio/missing.wav";
    let artifact = AudioArtifact::new_private(
        "artifact-missing-file",
        "session-1",
        ArtifactKind::RawMic,
        missing_relative_path,
        "sha256:missing",
    );
    store
        .insert_audio_artifact(&artifact)
        .expect("insert artifact");

    let report = store.delete_meeting("meeting-1").expect("delete meeting");

    assert!(report.deleted_private_artifacts.is_empty());
    assert!(report.skipped_private_artifacts.is_empty());
    assert!(store
        .artifact_tombstoned("artifact-missing-file")
        .expect("artifact tombstoned"));
}

#[cfg(unix)]
#[test]
fn delete_meeting_skips_symlink_artifact_paths_that_escape_app_storage() {
    let root = test_root("delete-symlink-escape");
    let outside_root = test_root("delete-symlink-escape-outside");
    let store = Store::open(root.join("app.db"), root.clone()).expect("open store");
    store.migrate().expect("migrate");
    seed_meeting_session(&store, "meeting-1", "session-1");

    let outside_path = outside_root.join("user-owned.wav");
    fs::write(&outside_path, b"user owned").expect("outside file");
    let symlink_dir = root.join("meetings/meeting-1/audio/link");
    fs::create_dir_all(symlink_dir.parent().expect("symlink parent")).expect("symlink parent");
    std::os::unix::fs::symlink(&outside_root, &symlink_dir).expect("symlink to outside root");
    let db_path = PathBuf::from("meetings/meeting-1/audio/link/user-owned.wav");
    let artifact = AudioArtifact::new_private(
        "artifact-symlink-escape",
        "session-1",
        ArtifactKind::RawMic,
        db_path.to_string_lossy(),
        "sha256:symlink",
    );
    store
        .insert_audio_artifact(&artifact)
        .expect("insert artifact");

    let report = store.delete_meeting("meeting-1").expect("delete meeting");

    assert!(report.deleted_private_artifacts.is_empty());
    assert_eq!(report.skipped_private_artifacts, vec![root.join(db_path)]);
    assert!(outside_path.exists());
}

#[test]
fn startup_repair_reports_manifest_db_conflicts_without_recovering_rows_or_jobs() {
    let cases = [
        (
            "artifact-id",
            ConflictingManifest {
                artifact_id: "artifact-missing",
                session_id: "session-1",
                meeting_id: "meeting-1",
                path: "meetings/meeting-1/audio/raw-mic.wav",
                sha256: "sha256:partial",
            },
            RepairConflict::MissingArtifact {
                artifact_id: "artifact-missing".to_string(),
            },
        ),
        (
            "path",
            ConflictingManifest {
                artifact_id: "artifact-1",
                session_id: "session-1",
                meeting_id: "meeting-1",
                path: "meetings/meeting-1/audio/other.wav",
                sha256: "sha256:partial",
            },
            RepairConflict::MismatchedPath {
                artifact_id: "artifact-1".to_string(),
                manifest_path: "meetings/meeting-1/audio/other.wav".to_string(),
                db_path: "meetings/meeting-1/audio/raw-mic.wav".to_string(),
            },
        ),
        (
            "hash",
            ConflictingManifest {
                artifact_id: "artifact-1",
                session_id: "session-1",
                meeting_id: "meeting-1",
                path: "meetings/meeting-1/audio/raw-mic.wav",
                sha256: "sha256:wrong",
            },
            RepairConflict::MismatchedHash {
                artifact_id: "artifact-1".to_string(),
                manifest_sha256: "sha256:wrong".to_string(),
                db_sha256: "sha256:partial".to_string(),
            },
        ),
        (
            "session",
            ConflictingManifest {
                artifact_id: "artifact-1",
                session_id: "session-other",
                meeting_id: "meeting-1",
                path: "meetings/meeting-1/audio/raw-mic.wav",
                sha256: "sha256:partial",
            },
            RepairConflict::MismatchedSession {
                artifact_id: "artifact-1".to_string(),
                manifest_session_id: "session-other".to_string(),
                db_session_id: "session-1".to_string(),
            },
        ),
        (
            "meeting",
            ConflictingManifest {
                artifact_id: "artifact-1",
                session_id: "session-1",
                meeting_id: "meeting-other",
                path: "meetings/meeting-1/audio/raw-mic.wav",
                sha256: "sha256:partial",
            },
            RepairConflict::MismatchedMeeting {
                artifact_id: "artifact-1".to_string(),
                manifest_meeting_id: "meeting-other".to_string(),
                db_meeting_id: "meeting-1".to_string(),
            },
        ),
    ];

    for (name, conflicting, expected_conflict) in cases {
        let root = test_root(&format!("repair-conflict-{name}"));
        let store = Store::open(root.join("app.db"), root.clone()).expect("open store");
        store.migrate().expect("migrate");
        seed_crashed_meeting(&store, &root);
        conflicting.write_manifest(&root);

        let report = store.repair_startup().expect("repair startup");

        assert_eq!(report.conflicts, vec![expected_conflict], "{name}");
        assert!(report.recovered_artifacts.is_empty(), "{name}");
        assert!(report.recovered_jobs.is_empty(), "{name}");
        assert_eq!(
            store
                .artifact_recovery_status("artifact-1")
                .expect("artifact status"),
            RepairStatus::NotNeeded,
            "{name}"
        );
        assert_eq!(
            store.job_status("job-1").expect("job status"),
            JobStatus::Running,
            "{name}"
        );
    }
}

#[test]
fn startup_repair_rejects_partial_mixed_artifact_sets_without_recovering_rows_or_jobs() {
    let root = test_root("repair-partial-mixed");
    let store = Store::open(root.join("app.db"), root.clone()).expect("open store");
    store.migrate().expect("migrate");

    let meeting = Meeting::new_manual("meeting-1", "Planning", 1_000);
    store.insert_meeting(&meeting).expect("insert meeting");
    let session = RecordingSession::start(
        "session-1",
        "meeting-1",
        RecordingSource::Mixed,
        1_010,
        48_000,
    );
    store
        .insert_recording_session(&session)
        .expect("insert session");

    let mic_path = "meetings/meeting-1/audio/recording-1/raw-mic.wav";
    let system_path = "meetings/meeting-1/audio/recording-1/raw-system.wav";
    let absolute_mic_path = root.join(mic_path);
    fs::create_dir_all(absolute_mic_path.parent().expect("mic parent")).expect("mic dir");
    fs::write(&absolute_mic_path, b"partial mic wav").expect("mic artifact file");
    store
        .insert_audio_artifact(&AudioArtifact::new_private(
            "artifact-mic",
            "session-1",
            ArtifactKind::RawMic,
            mic_path,
            "sha256:pending:mic",
        ))
        .expect("insert mic artifact");
    store
        .insert_audio_artifact(&AudioArtifact::new_private(
            "artifact-system",
            "session-1",
            ArtifactKind::RawSystem,
            system_path,
            "sha256:pending:system",
        ))
        .expect("insert system artifact");
    store
        .write_recoverable_artifact_manifests(
            "meeting-1",
            "session-1",
            &[
                RecoverableArtifact {
                    artifact_id: "artifact-mic".to_string(),
                    path: mic_path.to_string(),
                    sha256: "sha256:pending:mic".to_string(),
                },
                RecoverableArtifact {
                    artifact_id: "artifact-system".to_string(),
                    path: system_path.to_string(),
                    sha256: "sha256:pending:system".to_string(),
                },
            ],
        )
        .expect("write mixed manifest");
    let job = curiosity_domain::ProcessingJob::new(
        "job-1",
        "meeting-1",
        JobKind::Transcribe,
        JobStatus::Running,
    );
    store.insert_processing_job(&job).expect("insert job");

    let report = store.repair_startup().expect("repair startup");

    assert_eq!(
        report.conflicts,
        vec![RepairConflict::MissingFile {
            artifact_id: "artifact-system".to_string(),
            path: system_path.to_string(),
        }]
    );
    assert!(report.recovered_artifacts.is_empty());
    assert!(report.recovered_jobs.is_empty());
    assert_eq!(
        store
            .artifact_recovery_status("artifact-mic")
            .expect("mic status"),
        RepairStatus::NotNeeded
    );
    assert_eq!(
        store
            .artifact_recovery_status("artifact-system")
            .expect("system status"),
        RepairStatus::NotNeeded
    );
    assert_eq!(
        store.job_status("job-1").expect("job status"),
        JobStatus::Running
    );
    assert_eq!(
        store
            .recording_session_status("session-1")
            .expect("session status"),
        "Recording"
    );
    assert!(store
        .completed_wav_artifacts_for_transcription("meeting-1")
        .expect("transcription artifacts")
        .is_empty());
}

#[test]
fn startup_repair_recovers_required_mic_when_optional_system_artifact_is_missing() {
    let root = test_root("repair-optional-system-missing");
    let store = Store::open(root.join("app.db"), root.clone()).expect("open store");
    store.migrate().expect("migrate");

    let meeting = Meeting::new_manual("meeting-1", "Planning", 1_000);
    store.insert_meeting(&meeting).expect("insert meeting");
    let session = RecordingSession::start(
        "session-1",
        "meeting-1",
        RecordingSource::Microphone,
        1_010,
        48_000,
    );
    store
        .insert_recording_session(&session)
        .expect("insert session");

    let mic_path = "meetings/meeting-1/audio/recording-1/raw-mic.wav";
    let system_path = "meetings/meeting-1/audio/recording-1/raw-system.wav";
    let absolute_mic_path = root.join(mic_path);
    fs::create_dir_all(absolute_mic_path.parent().expect("mic parent")).expect("mic dir");
    fs::write(&absolute_mic_path, b"recoverable mic wav").expect("mic artifact file");
    store
        .insert_audio_artifact(&AudioArtifact::new_private(
            "artifact-mic",
            "session-1",
            ArtifactKind::RawMic,
            mic_path,
            "sha256:pending:mic",
        ))
        .expect("insert mic artifact");
    store
        .insert_audio_artifact(&AudioArtifact::new_private(
            "artifact-system",
            "session-1",
            ArtifactKind::RawSystem,
            system_path,
            "sha256:pending:system",
        ))
        .expect("insert system artifact");
    store
        .write_recoverable_artifact_manifests(
            "meeting-1",
            "session-1",
            &[
                RecoverableArtifact {
                    artifact_id: "artifact-mic".to_string(),
                    path: mic_path.to_string(),
                    sha256: "sha256:pending:mic".to_string(),
                },
                RecoverableArtifact {
                    artifact_id: "artifact-system".to_string(),
                    path: system_path.to_string(),
                    sha256: "sha256:pending:system".to_string(),
                },
            ],
        )
        .expect("write recoverable manifest");
    let job = curiosity_domain::ProcessingJob::new(
        "job-1",
        "meeting-1",
        JobKind::Transcribe,
        JobStatus::Running,
    );
    store.insert_processing_job(&job).expect("insert job");

    let report = store.repair_startup().expect("repair startup");
    let artifacts = store
        .completed_wav_artifacts_for_transcription("meeting-1")
        .expect("transcription artifacts");

    assert!(report.conflicts.is_empty());
    assert_eq!(report.recovered_artifacts, vec!["artifact-mic"]);
    assert_eq!(report.recovered_jobs, vec!["job-1"]);
    assert_eq!(
        store
            .artifact_recovery_status("artifact-mic")
            .expect("mic status"),
        RepairStatus::Recovered
    );
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].kind, "RawMic");
}

#[test]
fn processing_jobs_round_trip_all_phase_one_statuses() {
    let root = test_root("jobs");
    let store = Store::open(root.join("app.db"), root).expect("open store");
    store.migrate().expect("migrate");
    let meeting = Meeting::new_manual("meeting-1", "Planning", 1_000);
    store.insert_meeting(&meeting).expect("insert meeting");

    let statuses = [
        JobStatus::Queued,
        JobStatus::Running,
        JobStatus::Succeeded,
        JobStatus::Failed,
        JobStatus::Canceled,
        JobStatus::Retry,
        JobStatus::Recovery,
    ];

    for status in statuses {
        let job = curiosity_domain::ProcessingJob::new(
            format!("job-{status:?}"),
            "meeting-1",
            JobKind::Transcribe,
            status,
        );
        store.insert_processing_job(&job).expect("insert job");
        assert_eq!(store.job_status(&job.id).expect("stored status"), status);
    }
}

struct ConflictingManifest {
    artifact_id: &'static str,
    session_id: &'static str,
    meeting_id: &'static str,
    path: &'static str,
    sha256: &'static str,
}

impl ConflictingManifest {
    fn write_manifest(&self, root: &Path) {
        ArtifactManifest::new(
            self.meeting_id,
            self.session_id,
            self.artifact_id,
            self.path,
            self.sha256,
        )
        .mark_interrupted_recoverable()
        .write(root.join("meetings/meeting-1/manifest.json"))
        .expect("write conflicting manifest");
    }
}

fn seed_meeting_session(store: &Store, meeting_id: &str, session_id: &str) {
    let meeting = Meeting::new_manual(meeting_id, "Planning", 1_000);
    store.insert_meeting(&meeting).expect("insert meeting");
    let session = RecordingSession::start(
        session_id,
        meeting.id.clone(),
        RecordingSource::Microphone,
        1_010,
        48_000,
    );
    store
        .insert_recording_session(&session)
        .expect("insert session");
}

fn seed_crashed_meeting(store: &Store, root: &Path) {
    seed_meeting_session(store, "meeting-1", "session-1");

    let private_path = root.join("meetings/meeting-1/audio/raw-mic.wav");
    fs::create_dir_all(private_path.parent().expect("parent")).expect("private dirs");
    fs::write(&private_path, b"partial wav").expect("artifact file");
    let artifact = AudioArtifact::new_private(
        "artifact-1",
        "session-1",
        ArtifactKind::RawMic,
        private_path
            .strip_prefix(root)
            .expect("relative")
            .to_string_lossy(),
        "sha256:partial",
    );
    store
        .insert_audio_artifact(&artifact)
        .expect("insert artifact");

    ArtifactManifest::new(
        "meeting-1",
        "session-1",
        artifact.id.clone(),
        private_path
            .strip_prefix(root)
            .expect("relative")
            .to_string_lossy(),
        artifact.sha256.clone(),
    )
    .mark_interrupted_recoverable()
    .write(root.join("meetings/meeting-1/manifest.json"))
    .expect("write manifest");

    let job = curiosity_domain::ProcessingJob::new(
        "job-1",
        "meeting-1",
        JobKind::Transcribe,
        JobStatus::Running,
    );
    store.insert_processing_job(&job).expect("insert job");
}
