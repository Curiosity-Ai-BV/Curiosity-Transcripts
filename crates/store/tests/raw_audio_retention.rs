use std::fs;
use std::path::{Path, PathBuf};

use curiosity_domain::{
    ArtifactKind, AudioArtifact, Meeting, MeetingStatus, ModelRun, RawAudioRetentionPolicy,
    RecordingSession, RecordingSource, SourceChannel, TranscriptSegment, TranscriptVersion,
};
use curiosity_store::{CompletedAudioArtifact, Store};
use rusqlite::Connection;

fn test_root(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "curiosity-store-retention-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("test root");
    path
}

#[test]
fn cleanup_deletes_and_tombstones_only_selected_delete_after_app_private_wav_artifacts() {
    let root = test_root("delete-after-cleanup");
    let store = migrated_store(&root);
    seed_completed_imported_session(
        &store,
        &root,
        "meeting-1",
        "session-delete",
        "artifact-delete",
        "meetings/meeting-1/audio/session-delete/imported.wav",
        "delete-after-audio",
        RawAudioRetentionPolicy::DeleteAfterTranscription,
    );
    seed_completed_imported_session(
        &store,
        &root,
        "meeting-1",
        "session-unused",
        "artifact-unused",
        "meetings/meeting-1/audio/session-unused/imported.wav",
        "unused-delete-after-audio",
        RawAudioRetentionPolicy::DeleteAfterTranscription,
    );
    persist_transcript_and_export(&store, &root);

    let report = store
        .cleanup_raw_audio_artifacts_after_transcription("meeting-1", &["artifact-delete"])
        .expect("cleanup delete-after audio");

    assert_eq!(
        report
            .deleted_private_artifacts
            .iter()
            .map(|path| path
                .strip_prefix(&root)
                .expect("private path")
                .to_path_buf())
            .collect::<Vec<_>>(),
        vec![PathBuf::from(
            "meetings/meeting-1/audio/session-delete/imported.wav"
        )]
    );
    assert!(report.missing_private_artifacts.is_empty());
    assert!(report.skipped_private_artifacts.is_empty());
    assert!(!root
        .join("meetings/meeting-1/audio/session-delete/imported.wav")
        .exists());
    assert!(root
        .join("meetings/meeting-1/audio/session-unused/imported.wav")
        .exists());
    assert!(store
        .artifact_tombstoned("artifact-delete")
        .expect("delete artifact tombstoned"));
    assert!(!store
        .artifact_tombstoned("artifact-unused")
        .expect("unused artifact retained"));
    assert_eq!(
        store
            .transcript_segments("meeting-1")
            .expect("transcript rows")
            .len(),
        1
    );
    assert_eq!(
        store
            .search_meetings("cleanup transcript")
            .expect("search rows")
            .iter()
            .map(|result| result.meeting_id.as_str())
            .collect::<Vec<_>>(),
        vec!["meeting-1"]
    );
    assert_eq!(
        store.exported_files("meeting-1").expect("export rows"),
        vec![root.join("exports/meeting-1.md")]
    );
}

#[test]
fn cleanup_tombstones_missing_delete_after_private_wav_without_touching_retained_sessions() {
    let root = test_root("missing-and-retain");
    let store = migrated_store(&root);
    seed_completed_imported_session(
        &store,
        &root,
        "meeting-1",
        "session-delete",
        "artifact-missing",
        "meetings/meeting-1/audio/session-delete/imported.wav",
        "missing-delete-after-audio",
        RawAudioRetentionPolicy::DeleteAfterTranscription,
    );
    seed_completed_imported_session(
        &store,
        &root,
        "meeting-1",
        "session-retain",
        "artifact-retain",
        "meetings/meeting-1/audio/session-retain/imported.wav",
        "retained-audio",
        RawAudioRetentionPolicy::Retain,
    );
    fs::remove_file(root.join("meetings/meeting-1/audio/session-delete/imported.wav"))
        .expect("simulate already deleted private file");

    let report = store
        .cleanup_raw_audio_artifacts_after_transcription(
            "meeting-1",
            &["artifact-missing", "artifact-retain"],
        )
        .expect("cleanup delete-after audio");

    assert!(report.deleted_private_artifacts.is_empty());
    assert_eq!(
        report
            .missing_private_artifacts
            .iter()
            .map(|path| path
                .strip_prefix(&root)
                .expect("private path")
                .to_path_buf())
            .collect::<Vec<_>>(),
        vec![PathBuf::from(
            "meetings/meeting-1/audio/session-delete/imported.wav"
        )]
    );
    assert!(report.skipped_private_artifacts.is_empty());
    assert!(store
        .artifact_tombstoned("artifact-missing")
        .expect("missing artifact tombstoned"));
    assert!(!store
        .artifact_tombstoned("artifact-retain")
        .expect("retain artifact untouched"));
    assert!(root
        .join("meetings/meeting-1/audio/session-retain/imported.wav")
        .exists());
}

#[test]
fn cleanup_skips_unsafe_or_user_owned_paths_without_tombstoning_rows() {
    let root = test_root("unsafe-skip");
    let outside_root = test_root("unsafe-skip-outside");
    let store = migrated_store(&root);
    seed_meeting(&store, "meeting-1");
    let session = RecordingSession::start(
        "session-unsafe",
        "meeting-1",
        RecordingSource::Imported,
        1_000,
        48_000,
    )
    .with_raw_audio_retention_policy(RawAudioRetentionPolicy::DeleteAfterTranscription)
    .complete(1_500);
    store
        .insert_recording_session(&session)
        .expect("insert session");
    let outside_path = outside_root.join("user-owned.wav");
    fs::write(&outside_path, b"user owned wav").expect("write outside wav");
    store
        .insert_audio_artifact(&AudioArtifact::new_private(
            "artifact-unsafe",
            "session-unsafe",
            ArtifactKind::Imported,
            outside_path.to_string_lossy(),
            "sha256:unsafe",
        ))
        .expect("insert unsafe artifact");
    store
        .complete_recording_session_with_artifacts(
            "meeting-1",
            "session-unsafe",
            1_500,
            RecordingSource::Imported,
            &[CompletedAudioArtifact {
                artifact_id: "artifact-unsafe".to_string(),
                sha256: "sha256:unsafe".to_string(),
            }],
        )
        .expect("complete unsafe artifact");

    let error = store
        .cleanup_raw_audio_artifacts_after_transcription("meeting-1", &["artifact-unsafe"])
        .expect_err("unsafe paths must fail cleanup before tombstoning");

    assert!(
        error
            .to_string()
            .contains("Raw audio retention cleanup preflight failed"),
        "unsafe cleanup should fail before deleting or tombstoning: {error}"
    );
    assert!(
        error
            .to_string()
            .contains(&outside_path.display().to_string()),
        "unsafe cleanup failure should name the blocked path: {error}"
    );
    assert!(outside_path.exists());
    assert!(!store
        .artifact_tombstoned("artifact-unsafe")
        .expect("unsafe row remains retained"));
}

#[test]
fn cleanup_preflights_all_selected_delete_after_artifacts_before_mutating_anything() {
    let root = test_root("preflight-all-before-mutating");
    let store = migrated_store(&root);
    seed_completed_imported_session_with_artifacts(
        &store,
        &root,
        "meeting-1",
        "session-delete",
        RawAudioRetentionPolicy::DeleteAfterTranscription,
        &[
            (
                "artifact-clean",
                "meetings/meeting-1/audio/session-delete/clean.wav",
                "cleanable-delete-after-audio",
            ),
            (
                "artifact-blocked",
                "meetings/meeting-1/audio/session-delete/blocked.wav",
                "blocked-delete-after-audio",
            ),
        ],
    );
    let clean_path = root.join("meetings/meeting-1/audio/session-delete/clean.wav");
    let blocked_path = root.join("meetings/meeting-1/audio/session-delete/blocked.wav");
    fs::remove_file(&blocked_path).expect("remove blocked fixture file");
    fs::create_dir_all(&blocked_path).expect("replace selected artifact path with a directory");

    let error = store
        .cleanup_raw_audio_artifacts_after_transcription(
            "meeting-1",
            &["artifact-clean", "artifact-blocked"],
        )
        .expect_err("preflight failure must abort the whole selected artifact bundle");

    assert!(
        error
            .to_string()
            .contains("Raw audio retention cleanup preflight failed"),
        "blocked cleanup should fail during preflight: {error}"
    );
    assert!(
        error.to_string().contains("artifact-blocked"),
        "blocked cleanup failure should identify the bad artifact: {error}"
    );
    assert!(
        clean_path.exists(),
        "preflight failure must not delete earlier selected artifacts"
    );
    assert!(
        blocked_path.is_dir(),
        "preflight failure must not mutate the blocked artifact path"
    );
    assert!(!store
        .artifact_tombstoned("artifact-clean")
        .expect("clean artifact row remains retained"));
    assert!(!store
        .artifact_tombstoned("artifact-blocked")
        .expect("blocked artifact row remains retained"));
    let mut retryable_artifacts = store
        .completed_wav_artifacts_for_transcription("meeting-1")
        .expect("bundle remains retryable")
        .into_iter()
        .map(|artifact| artifact.artifact_id)
        .collect::<Vec<_>>();
    retryable_artifacts.sort();
    assert_eq!(
        retryable_artifacts,
        vec!["artifact-blocked".to_string(), "artifact-clean".to_string()]
    );
}

#[test]
fn cleanup_tombstone_transaction_failure_leaves_files_and_rows_retryable() {
    let root = test_root("tombstone-transaction-failure");
    let store = migrated_store(&root);
    seed_completed_imported_session_with_artifacts(
        &store,
        &root,
        "meeting-1",
        "session-delete",
        RawAudioRetentionPolicy::DeleteAfterTranscription,
        &[
            (
                "artifact-clean",
                "meetings/meeting-1/audio/session-delete/clean.wav",
                "cleanable-delete-after-audio",
            ),
            (
                "artifact-blocked",
                "meetings/meeting-1/audio/session-delete/blocked.wav",
                "blocked-delete-after-audio",
            ),
        ],
    );
    let clean_path = root.join("meetings/meeting-1/audio/session-delete/clean.wav");
    let blocked_path = root.join("meetings/meeting-1/audio/session-delete/blocked.wav");
    let trigger_conn = Connection::open(root.join("app.db")).expect("open trigger connection");
    trigger_conn
        .execute_batch(
            "
            CREATE TRIGGER abort_raw_audio_retention_tombstone
            BEFORE UPDATE OF retained, tombstoned ON audio_artifacts
            WHEN NEW.tombstoned = 1
              AND OLD.id IN ('artifact-clean', 'artifact-blocked')
            BEGIN
              SELECT RAISE(ABORT, 'forced tombstone failure');
            END;
            ",
        )
        .expect("install tombstone failure trigger");

    let error = store
        .cleanup_raw_audio_artifacts_after_transcription(
            "meeting-1",
            &["artifact-clean", "artifact-blocked"],
        )
        .expect_err("tombstone transaction failure must fail cleanup");

    assert!(
        error.to_string().contains("forced tombstone failure"),
        "cleanup should surface the tombstone transaction failure: {error}"
    );
    assert!(
        clean_path.exists(),
        "tombstone transaction failure must not delete selected files"
    );
    assert!(
        blocked_path.exists(),
        "tombstone transaction failure must not delete selected files"
    );
    assert!(!store
        .artifact_tombstoned("artifact-clean")
        .expect("clean artifact row remains retained"));
    assert!(!store
        .artifact_tombstoned("artifact-blocked")
        .expect("blocked artifact row remains retained"));
    let mut retryable_artifacts = store
        .completed_wav_artifacts_for_transcription("meeting-1")
        .expect("bundle remains retryable")
        .into_iter()
        .map(|artifact| artifact.artifact_id)
        .collect::<Vec<_>>();
    retryable_artifacts.sort();
    assert_eq!(
        retryable_artifacts,
        vec!["artifact-blocked".to_string(), "artifact-clean".to_string()]
    );
}

#[test]
fn pending_raw_audio_retention_cleanup_removes_tombstoned_delete_after_private_files() {
    let root = test_root("pending-cleanup-safe");
    let store = migrated_store(&root);
    seed_completed_imported_session(
        &store,
        &root,
        "meeting-1",
        "session-delete",
        "artifact-delete",
        "meetings/meeting-1/audio/session-delete/imported.wav",
        "delete-after-audio",
        RawAudioRetentionPolicy::DeleteAfterTranscription,
    );
    let artifact_path = root.join("meetings/meeting-1/audio/session-delete/imported.wav");
    store
        .tombstone_audio_artifact("artifact-delete")
        .expect("simulate committed tombstone before file removal");
    assert!(artifact_path.exists());
    assert!(store
        .completed_wav_artifact_for_transcription("meeting-1")
        .expect("normal transcription query no longer sees tombstoned row")
        .is_none());

    let report = store
        .finalize_pending_raw_audio_retention_cleanup()
        .expect("finalize pending raw cleanup");

    assert_eq!(
        report.deleted_private_artifacts,
        vec![artifact_path.clone()]
    );
    assert!(report.missing_private_artifacts.is_empty());
    assert!(report.skipped_private_artifacts.is_empty());
    assert!(!artifact_path.exists());
    assert!(store
        .artifact_tombstoned("artifact-delete")
        .expect("artifact row remains tombstoned"));
}

#[test]
fn pending_raw_audio_retention_cleanup_fails_loud_for_unsafe_tombstoned_paths() {
    let root = test_root("pending-cleanup-unsafe");
    let outside_root = test_root("pending-cleanup-unsafe-outside");
    let store = migrated_store(&root);
    seed_meeting(&store, "meeting-1");
    let session = RecordingSession::start(
        "session-unsafe",
        "meeting-1",
        RecordingSource::Imported,
        1_000,
        48_000,
    )
    .with_raw_audio_retention_policy(RawAudioRetentionPolicy::DeleteAfterTranscription)
    .complete(1_500);
    store
        .insert_recording_session(&session)
        .expect("insert session");
    let outside_path = outside_root.join("user-owned.wav");
    fs::write(&outside_path, b"user owned wav").expect("write outside wav");
    store
        .insert_audio_artifact(&AudioArtifact::new_private(
            "artifact-unsafe",
            "session-unsafe",
            ArtifactKind::Imported,
            outside_path.to_string_lossy(),
            "sha256:unsafe",
        ))
        .expect("insert unsafe artifact");
    store
        .complete_recording_session_with_artifacts(
            "meeting-1",
            "session-unsafe",
            1_500,
            RecordingSource::Imported,
            &[CompletedAudioArtifact {
                artifact_id: "artifact-unsafe".to_string(),
                sha256: "sha256:unsafe".to_string(),
            }],
        )
        .expect("complete unsafe artifact");
    store
        .tombstone_audio_artifact("artifact-unsafe")
        .expect("simulate committed tombstone before file removal");

    let error = store
        .finalize_pending_raw_audio_retention_cleanup()
        .expect_err("unsafe pending raw cleanup should fail loudly");

    assert!(
        error
            .to_string()
            .contains("Pending raw audio retention cleanup failed"),
        "unsafe pending cleanup should fail loudly: {error}"
    );
    assert!(
        error
            .to_string()
            .contains(&outside_path.display().to_string()),
        "unsafe pending cleanup failure should name the blocked path: {error}"
    );
    assert!(outside_path.exists());
    assert!(store
        .artifact_tombstoned("artifact-unsafe")
        .expect("unsafe artifact row remains tombstoned"));
}

#[test]
fn pending_raw_audio_retention_cleanup_leaves_deleted_meeting_paths_for_delete_cleanup() {
    let root = test_root("pending-cleanup-deleted-meeting");
    let outside_root = test_root("pending-cleanup-deleted-meeting-outside");
    let store = migrated_store(&root);
    seed_meeting(&store, "meeting-1");
    let session = RecordingSession::start(
        "session-unsafe",
        "meeting-1",
        RecordingSource::Imported,
        1_000,
        48_000,
    )
    .with_raw_audio_retention_policy(RawAudioRetentionPolicy::DeleteAfterTranscription)
    .complete(1_500);
    store
        .insert_recording_session(&session)
        .expect("insert session");
    let outside_path = outside_root.join("user-owned.wav");
    fs::write(&outside_path, b"user owned wav").expect("write outside wav");
    store
        .insert_audio_artifact(&AudioArtifact::new_private(
            "artifact-unsafe",
            "session-unsafe",
            ArtifactKind::Imported,
            outside_path.to_string_lossy(),
            "sha256:unsafe",
        ))
        .expect("insert unsafe artifact");
    store
        .complete_recording_session_with_artifacts(
            "meeting-1",
            "session-unsafe",
            1_500,
            RecordingSource::Imported,
            &[CompletedAudioArtifact {
                artifact_id: "artifact-unsafe".to_string(),
                sha256: "sha256:unsafe".to_string(),
            }],
        )
        .expect("complete unsafe artifact");
    store
        .tombstone_audio_artifact("artifact-unsafe")
        .expect("simulate committed tombstone before file removal");
    store
        .update_meeting_status("meeting-1", MeetingStatus::Deleted, Some(2_000))
        .expect("mark meeting deleted before pending delete finalization");

    let report = store
        .finalize_pending_raw_audio_retention_cleanup()
        .expect("raw retention cleanup should leave deleted meetings to delete cleanup");

    assert!(report.deleted_private_artifacts.is_empty());
    assert!(report.missing_private_artifacts.is_empty());
    assert!(report.skipped_private_artifacts.is_empty());
    assert!(outside_path.exists());
    assert!(store
        .artifact_tombstoned("artifact-unsafe")
        .expect("deleted meeting artifact row remains for delete finalization"));
}

fn migrated_store(root: &Path) -> Store {
    let store = Store::open(root.join("app.db"), root.to_path_buf()).expect("open store");
    store.migrate().expect("migrate");
    store
}

fn seed_meeting(store: &Store, meeting_id: &str) {
    let meeting = Meeting::new_manual(meeting_id, "Retention cleanup", 1_000);
    store.insert_meeting(&meeting).expect("insert meeting");
}

#[expect(
    clippy::too_many_arguments,
    reason = "test fixture keeps the persisted policy and artifact identity explicit"
)]
fn seed_completed_imported_session(
    store: &Store,
    root: &Path,
    meeting_id: &str,
    session_id: &str,
    artifact_id: &str,
    relative_path: &str,
    contents: &str,
    policy: RawAudioRetentionPolicy,
) {
    if store.meeting_title(meeting_id).is_err() {
        seed_meeting(store, meeting_id);
    }
    let session = RecordingSession::start(
        session_id,
        meeting_id,
        RecordingSource::Imported,
        1_000,
        48_000,
    )
    .with_raw_audio_retention_policy(policy);
    let artifact_path = root.join(relative_path);
    fs::create_dir_all(artifact_path.parent().expect("artifact parent"))
        .expect("artifact parent dir");
    fs::write(&artifact_path, contents.as_bytes()).expect("write private wav");
    store
        .insert_recording_session(&session)
        .expect("insert session");
    store
        .insert_audio_artifact(&AudioArtifact::new_private(
            artifact_id,
            session_id,
            ArtifactKind::Imported,
            relative_path,
            format!("sha256:{artifact_id}"),
        ))
        .expect("insert artifact");
    store
        .complete_recording_session_with_artifacts(
            meeting_id,
            session_id,
            1_500,
            RecordingSource::Imported,
            &[CompletedAudioArtifact {
                artifact_id: artifact_id.to_string(),
                sha256: format!("sha256:{artifact_id}"),
            }],
        )
        .expect("complete recording");
}

fn seed_completed_imported_session_with_artifacts(
    store: &Store,
    root: &Path,
    meeting_id: &str,
    session_id: &str,
    policy: RawAudioRetentionPolicy,
    artifacts: &[(&str, &str, &str)],
) {
    if store.meeting_title(meeting_id).is_err() {
        seed_meeting(store, meeting_id);
    }
    let session = RecordingSession::start(
        session_id,
        meeting_id,
        RecordingSource::Imported,
        1_000,
        48_000,
    )
    .with_raw_audio_retention_policy(policy);
    store
        .insert_recording_session(&session)
        .expect("insert session");
    let mut completed_artifacts = Vec::new();
    for (artifact_id, relative_path, contents) in artifacts {
        let artifact_path = root.join(relative_path);
        fs::create_dir_all(artifact_path.parent().expect("artifact parent"))
            .expect("artifact parent dir");
        fs::write(&artifact_path, contents.as_bytes()).expect("write private wav");
        store
            .insert_audio_artifact(&AudioArtifact::new_private(
                *artifact_id,
                session_id,
                ArtifactKind::Imported,
                *relative_path,
                format!("sha256:{artifact_id}"),
            ))
            .expect("insert artifact");
        completed_artifacts.push(CompletedAudioArtifact {
            artifact_id: (*artifact_id).to_string(),
            sha256: format!("sha256:{artifact_id}"),
        });
    }
    store
        .complete_recording_session_with_artifacts(
            meeting_id,
            session_id,
            1_500,
            RecordingSource::Imported,
            &completed_artifacts,
        )
        .expect("complete recording");
}

fn persist_transcript_and_export(store: &Store, root: &Path) {
    let run = ModelRun::new(
        "run-1",
        "meeting-1",
        "sha256:artifact-delete",
        "fake-local",
        "fixture-whisper",
        false,
        2_000,
    );
    let version = TranscriptVersion::new("version-1", "meeting-1", "run-1", 1, 2_010);
    let segments = vec![TranscriptSegment::with_metadata(
        "segment-1",
        "meeting-1",
        0,
        1_000,
        "cleanup transcript remains searchable",
        SourceChannel::Imported,
        "run-1",
        "version-1",
    )];
    store
        .persist_transcript(&run, &version, &segments)
        .expect("persist transcript");
    let export_path = root.join("exports/meeting-1.md");
    fs::create_dir_all(export_path.parent().expect("export parent")).expect("export dir");
    fs::write(&export_path, b"user export").expect("write export");
    store
        .record_exported_file("meeting-1", &export_path)
        .expect("record export");
}
