use std::fs;
use std::path::{Path, PathBuf};

use curiosity_domain::{
    ArtifactKind, AudioArtifact, Meeting, ModelRun, RecordingSession, RecordingSource,
    SourceChannel, TranscriptSegment, TranscriptVersion,
};
use curiosity_store::{ArtifactManifest, Store};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn test_root(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "curiosity-store-phase-four-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("test root");
    path
}

#[test]
fn search_index_includes_title_current_text_and_corrected_text_and_rebuild_is_idempotent() {
    let root = test_root("search");
    let store = migrated_store(&root);
    seed_meeting_with_transcript(&store, "meeting-1", "Planning Alpha", "helo roadmap");
    seed_meeting_with_transcript(&store, "meeting-2", "Design Review", "capacity forecast");
    store
        .correct_transcript_segment("meeting-1-segment-1", "hello launch roadmap", 2_500)
        .expect("correct transcript");

    store.rebuild_search_index().expect("first rebuild");
    let title_results = store.search_meetings("Alpha").expect("title search");
    let corrected_results = store.search_meetings("launch").expect("corrected search");
    let original_results = store.search_meetings("helo").expect("original search");

    store.rebuild_search_index().expect("second rebuild");
    let repeated_results = store.search_meetings("launch").expect("repeated search");

    assert_eq!(ids(&title_results), vec!["meeting-1"]);
    assert_eq!(ids(&corrected_results), vec!["meeting-1"]);
    assert_eq!(ids(&original_results), vec!["meeting-1"]);
    assert_eq!(corrected_results, repeated_results);
}

#[test]
fn search_finds_newly_persisted_transcript_text_without_manual_rebuild() {
    let root = test_root("search-auto-persist");
    let store = migrated_store(&root);

    seed_meeting_with_transcript(&store, "meeting-1", "Planning", "fresh transcript phrase");

    assert_eq!(
        ids(&store.search_meetings("fresh").expect("search")),
        vec!["meeting-1"]
    );
}

#[test]
fn insert_meeting_indexes_title_before_transcription_exists() {
    let root = test_root("search-title-only");
    let store = migrated_store(&root);
    let meeting = Meeting::new_manual("meeting-1", "Standalone Planning", 1_000);

    store.insert_meeting(&meeting).expect("insert meeting");

    assert_eq!(
        ids(&store.search_meetings("Standalone").expect("title search")),
        vec!["meeting-1"]
    );
}

#[test]
fn search_ignores_obsolete_transcript_versions_after_retranscription() {
    let root = test_root("search-current-version");
    let store = migrated_store(&root);
    seed_meeting_with_transcript(&store, "meeting-1", "Planning", "obsolete phrase");
    persist_transcript_version(
        &store,
        "meeting-1",
        2,
        "replacement phrase",
        "sha256:meeting-1-v2",
        3_000,
    );

    assert!(store
        .search_meetings("obsolete")
        .expect("obsolete search")
        .is_empty());
    assert_eq!(
        ids(&store.search_meetings("replacement").expect("current search")),
        vec!["meeting-1"]
    );
}

#[test]
fn search_finds_corrected_text_without_manual_rebuild_after_edit() {
    let root = test_root("search-auto-edit");
    let store = migrated_store(&root);
    seed_meeting_with_transcript(&store, "meeting-1", "Planning", "draft transcript phrase");

    store
        .correct_transcript_segment("meeting-1-segment-1", "corrected transcript phrase", 2_500)
        .expect("correct transcript");

    assert_eq!(
        ids(&store.search_meetings("corrected").expect("search")),
        vec!["meeting-1"]
    );
}

#[test]
fn rename_meeting_updates_sqlite_title_and_private_manifest_without_renaming_exports() {
    let root = test_root("rename");
    let export_root = test_root("rename-export");
    let store = migrated_store(&root);
    seed_meeting_with_transcript(&store, "meeting-1", "Original Title", "planning text");
    let manifest_path = root.join("meetings/meeting-1/manifest.json");
    ArtifactManifest::new(
        "meeting-1",
        "meeting-1-session-1",
        "meeting-1-artifact-1",
        "meetings/meeting-1/audio/imported.wav",
        "sha256:meeting-1",
    )
    .write(&manifest_path)
    .expect("write manifest");
    let exported_path = export_root.join("Original Title.md");
    fs::write(&exported_path, b"user export").expect("write export");
    store
        .record_exported_file("meeting-1", &exported_path)
        .expect("record export");

    store
        .rename_meeting("meeting-1", "Renamed Planning")
        .expect("rename meeting");

    assert_eq!(
        store.meeting_title("meeting-1").expect("meeting title"),
        "Renamed Planning"
    );
    let manifest = ArtifactManifest::read(&manifest_path).expect("read manifest");
    assert_eq!(manifest.meeting_id, "meeting-1");
    assert_eq!(manifest.meeting_title.as_deref(), Some("Renamed Planning"));
    assert_eq!(store.exported_files("meeting-1").expect("exports"), vec![exported_path]);
}

#[cfg(unix)]
#[test]
fn rename_meeting_rolls_back_already_replaced_manifests_when_later_manifest_write_fails() {
    let root = test_root("rename-two-manifests-rollback");
    let store = migrated_store(&root);
    seed_meeting_with_transcript(&store, "meeting-1", "Original Title", "planning text");
    let first_manifest_path = root.join("meetings/meeting-1/manifest.json");
    let second_manifest_dir = root.join("meetings/meeting-1-shadow");
    let second_manifest_path = second_manifest_dir.join("manifest.json");
    let mut first_manifest = ArtifactManifest::new(
        "meeting-1",
        "meeting-1-session-1",
        "meeting-1-artifact-1",
        "meetings/meeting-1/audio/imported.wav",
        "sha256:meeting-1",
    );
    first_manifest.meeting_title = Some("Original Title".to_string());
    first_manifest
        .write(&first_manifest_path)
        .expect("write first manifest");
    let mut second_manifest = ArtifactManifest::new(
        "meeting-1",
        "meeting-1-session-1",
        "meeting-1-artifact-shadow",
        "meetings/meeting-1/audio/imported.wav",
        "sha256:meeting-1",
    );
    second_manifest.meeting_title = Some("Original Title".to_string());
    second_manifest
        .write(&second_manifest_path)
        .expect("write second manifest");
    fs::set_permissions(&second_manifest_dir, fs::Permissions::from_mode(0o555))
        .expect("readonly second manifest dir");

    let err = store
        .rename_meeting("meeting-1", "Renamed Planning")
        .expect_err("second manifest failure should abort rename");

    fs::set_permissions(&second_manifest_dir, fs::Permissions::from_mode(0o755))
        .expect("restore second manifest dir");
    assert!(err.to_string().contains("manifest"));
    assert_eq!(
        store.meeting_title("meeting-1").expect("meeting title"),
        "Original Title"
    );
    assert_eq!(
        ArtifactManifest::read(&first_manifest_path)
            .expect("read first manifest")
            .meeting_title
            .as_deref(),
        Some("Original Title")
    );
    assert_eq!(
        ArtifactManifest::read(&second_manifest_path)
            .expect("read second manifest")
            .meeting_title
            .as_deref(),
        Some("Original Title")
    );
}

#[cfg(unix)]
#[test]
fn rename_meeting_preserves_old_sqlite_title_and_manifest_when_manifest_write_fails() {
    let root = test_root("rename-manifest-write-failure");
    let store = migrated_store(&root);
    seed_meeting_with_transcript(&store, "meeting-1", "Original Title", "planning text");
    let manifest_dir = root.join("meetings/meeting-1");
    let manifest_path = manifest_dir.join("manifest.json");
    let mut manifest = ArtifactManifest::new(
        "meeting-1",
        "meeting-1-session-1",
        "meeting-1-artifact-1",
        "meetings/meeting-1/audio/imported.wav",
        "sha256:meeting-1",
    );
    manifest.meeting_title = Some("Original Title".to_string());
    manifest.write(&manifest_path).expect("write manifest");
    fs::set_permissions(&manifest_path, fs::Permissions::from_mode(0o444))
        .expect("readonly manifest");
    fs::set_permissions(&manifest_dir, fs::Permissions::from_mode(0o555))
        .expect("readonly manifest dir");

    let err = store
        .rename_meeting("meeting-1", "Renamed Planning")
        .expect_err("manifest write failure should abort rename");

    fs::set_permissions(&manifest_dir, fs::Permissions::from_mode(0o755))
        .expect("restore dir permissions");
    fs::set_permissions(&manifest_path, fs::Permissions::from_mode(0o644))
        .expect("restore file permissions");
    assert!(err.to_string().contains("manifest"));
    assert_eq!(
        store.meeting_title("meeting-1").expect("meeting title"),
        "Original Title"
    );
    let manifest = ArtifactManifest::read(&manifest_path).expect("read manifest");
    assert_eq!(manifest.meeting_title.as_deref(), Some("Original Title"));
}

#[test]
fn export_round_trip_preserves_title_timestamps_transcript_and_edits() {
    let root = test_root("export");
    let export_root = test_root("export-output");
    let store = migrated_store(&root);
    seed_meeting_with_transcript(&store, "meeting-1", "Planning", "helo world");
    store
        .correct_transcript_segment("meeting-1-segment-1", "hello world", 2_500)
        .expect("correct transcript");

    let export_path = store
        .export_meeting_json("meeting-1", &export_root)
        .expect("export meeting");
    let round_trip = Store::read_meeting_export_json(&export_path).expect("read export");

    assert_eq!(round_trip.meeting_id, "meeting-1");
    assert_eq!(round_trip.title, "Planning");
    assert_eq!(round_trip.started_at_ms, 1_000);
    assert_eq!(round_trip.segments[0].start_ms, 0);
    assert_eq!(round_trip.segments[0].end_ms, 1_200);
    assert_eq!(round_trip.segments[0].text, "hello world");
    assert_eq!(round_trip.segments[0].original_text.as_deref(), Some("helo world"));
    assert_eq!(round_trip.segments[0].edits[0].previous_text, "helo world");
    assert_eq!(round_trip.segments[0].edits[0].corrected_text, "hello world");
    assert_eq!(store.exported_files("meeting-1").expect("exports"), vec![export_path]);
}

#[test]
fn export_meeting_json_rejects_meeting_ids_that_escape_export_root() {
    let root = test_root("export-traversal");
    let export_root = test_root("export-traversal-output");
    let store = migrated_store(&root);
    let meeting = Meeting::new_manual("../escape", "Unsafe", 1_000);
    store.insert_meeting(&meeting).expect("insert meeting");
    let escaped_path = export_root.join("../escape.json");
    let _ = fs::remove_file(&escaped_path);

    let err = store
        .export_meeting_json("../escape", &export_root)
        .expect_err("unsafe meeting id should not become a path");

    assert!(err.to_string().contains("safe export filename"));
    assert!(!escaped_path.exists());
}

#[test]
fn delete_meeting_removes_private_rows_and_search_results_but_reports_exports() {
    let root = test_root("delete-private-rows");
    let export_root = test_root("delete-private-rows-export");
    let store = migrated_store(&root);
    seed_meeting_with_transcript(&store, "meeting-1", "Planning", "delete me");
    let exported_path = export_root.join("meeting.json");
    fs::write(&exported_path, b"export").expect("write export");
    store
        .record_exported_file("meeting-1", &exported_path)
        .expect("record export");
    store.rebuild_search_index().expect("rebuild search");

    let report = store.delete_meeting("meeting-1").expect("delete meeting");

    assert_eq!(report.exported_files_outside_app_control, vec![exported_path]);
    assert_eq!(store.count("model_runs").expect("model runs"), 0);
    assert_eq!(
        store.count("recording_sessions").expect("recording sessions"),
        0
    );
    assert_eq!(store.count("audio_artifacts").expect("audio artifacts"), 0);
    assert_eq!(store.count("transcript_versions").expect("versions"), 0);
    assert_eq!(store.count("transcript_segments").expect("segments"), 0);
    assert_eq!(
        store
            .count("transcript_segment_edits")
            .expect("segment edits"),
        0
    );
    assert!(store.search_meetings("delete").expect("search").is_empty());
}

#[test]
fn delete_meeting_removes_only_target_private_rows_and_keeps_other_meeting_searchable() {
    let root = test_root("delete-target-only");
    let store = migrated_store(&root);
    seed_meeting_with_transcript(&store, "meeting-1", "Planning", "delete me");
    seed_meeting_with_transcript(&store, "meeting-2", "Design", "keep me");

    store.delete_meeting("meeting-1").expect("delete meeting");

    assert_eq!(store.count("model_runs").expect("model runs"), 1);
    assert_eq!(
        store.count("recording_sessions").expect("recording sessions"),
        1
    );
    assert_eq!(store.count("audio_artifacts").expect("audio artifacts"), 1);
    assert_eq!(store.count("transcript_versions").expect("versions"), 1);
    assert_eq!(store.count("transcript_segments").expect("segments"), 1);
    assert_eq!(
        ids(&store.search_meetings("keep").expect("search kept")),
        vec!["meeting-2"]
    );
    assert!(store.search_meetings("delete").expect("search deleted").is_empty());
}

#[test]
fn delete_meeting_removes_private_manifest_for_deleted_meeting_only() {
    let root = test_root("delete-manifest");
    let store = migrated_store(&root);
    seed_meeting_with_transcript(&store, "meeting-1", "Planning", "delete me");
    seed_meeting_with_transcript(&store, "meeting-2", "Design", "keep me");
    let deleted_manifest_path = root.join("meetings/meeting-1/manifest.json");
    ArtifactManifest::new(
        "meeting-1",
        "meeting-1-session-1",
        "meeting-1-artifact-1",
        "meetings/meeting-1/audio/imported.wav",
        "sha256:meeting-1",
    )
    .write(&deleted_manifest_path)
    .expect("write deleted manifest");
    let kept_manifest_path = root.join("meetings/meeting-2/manifest.json");
    ArtifactManifest::new(
        "meeting-2",
        "meeting-2-session-1",
        "meeting-2-artifact-1",
        "meetings/meeting-2/audio/imported.wav",
        "sha256:meeting-2",
    )
    .write(&kept_manifest_path)
    .expect("write kept manifest");

    store.delete_meeting("meeting-1").expect("delete meeting");

    assert!(!deleted_manifest_path.exists());
    assert!(kept_manifest_path.exists());
}

#[cfg(unix)]
#[test]
fn delete_meeting_retry_succeeds_after_file_removal_failure_marks_delete_intent() {
    let root = test_root("delete-retry-after-file-failure");
    let store = migrated_store(&root);
    seed_meeting_with_transcript(&store, "meeting-1", "Planning", "delete me");
    let artifact_path = root.join("meetings/meeting-1/audio/imported.wav");
    fs::create_dir_all(artifact_path.parent().expect("artifact parent")).expect("artifact dir");
    fs::write(&artifact_path, b"private audio").expect("artifact file");
    fs::set_permissions(
        artifact_path.parent().expect("artifact parent"),
        fs::Permissions::from_mode(0o555),
    )
    .expect("readonly artifact dir");

    let err = store
        .delete_meeting("meeting-1")
        .expect_err("readonly artifact dir should fail file removal");

    assert!(err.to_string().contains("Permission") || err.to_string().contains("permission"));
    assert!(store.meeting_deleted("meeting-1").expect("meeting deleted intent"));
    assert!(store
        .artifact_tombstoned("meeting-1-artifact-1")
        .expect("artifact tombstoned intent"));
    assert!(artifact_path.exists());

    fs::set_permissions(
        artifact_path.parent().expect("artifact parent"),
        fs::Permissions::from_mode(0o755),
    )
    .expect("restore artifact dir");
    let report = store.delete_meeting("meeting-1").expect("retry delete");

    assert_eq!(report.deleted_private_artifacts, vec![artifact_path.clone()]);
    assert!(!artifact_path.exists());
    assert_eq!(store.count("audio_artifacts").expect("audio artifacts"), 0);
    assert_eq!(
        store.count("recording_sessions").expect("recording sessions"),
        0
    );
    assert!(store.search_meetings("delete").expect("search").is_empty());
}

fn migrated_store(root: &Path) -> Store {
    let store = Store::open(root.join("app.db"), root.to_path_buf()).expect("open store");
    store.migrate().expect("migrate");
    store
}

fn seed_meeting_with_transcript(store: &Store, meeting_id: &str, title: &str, text: &str) {
    let meeting = Meeting::new_manual(meeting_id, title, 1_000);
    store.insert_meeting(&meeting).expect("insert meeting");
    let session_id = format!("{meeting_id}-session-1");
    let session = RecordingSession::start(
        &session_id,
        meeting_id,
        RecordingSource::Imported,
        1_000,
        48_000,
    );
    store
        .insert_recording_session(&session)
        .expect("insert session");
    store
        .insert_audio_artifact(&AudioArtifact::new_private(
            format!("{meeting_id}-artifact-1"),
            &session_id,
            ArtifactKind::Imported,
            format!("meetings/{meeting_id}/audio/imported.wav"),
            format!("sha256:{meeting_id}"),
        ))
        .expect("insert artifact");
    let run = ModelRun::new(
        format!("{meeting_id}-run-1"),
        meeting_id,
        format!("sha256:{meeting_id}"),
        "fake-local",
        "fixture-whisper",
        false,
        2_000,
    );
    let version = TranscriptVersion::new(
        format!("{meeting_id}-version-1"),
        meeting_id,
        format!("{meeting_id}-run-1"),
        1,
        2_010,
    );
    store
        .persist_transcript(
            &run,
            &version,
            &[TranscriptSegment::with_metadata(
                format!("{meeting_id}-segment-1"),
                meeting_id,
                0,
                1_200,
                text,
                SourceChannel::Imported,
                &run.id,
                &version.id,
            )],
        )
        .expect("persist transcript");
}

fn persist_transcript_version(
    store: &Store,
    meeting_id: &str,
    version_number: u32,
    text: &str,
    sha256: &str,
    created_at_ms: u64,
) {
    let run = ModelRun::new(
        format!("{meeting_id}-run-{version_number}"),
        meeting_id,
        sha256,
        "fake-local",
        "fixture-whisper",
        false,
        created_at_ms,
    );
    let version = TranscriptVersion::new(
        format!("{meeting_id}-version-{version_number}"),
        meeting_id,
        run.id.clone(),
        version_number,
        created_at_ms + 10,
    );
    store
        .persist_transcript(
            &run,
            &version,
            &[TranscriptSegment::with_metadata(
                format!("{meeting_id}-segment-{version_number}"),
                meeting_id,
                0,
                1_200,
                text,
                SourceChannel::Imported,
                &run.id,
                &version.id,
            )],
        )
        .expect("persist transcript version");
}

fn ids(results: &[curiosity_store::MeetingSearchResult]) -> Vec<&str> {
    results
        .iter()
        .map(|result| result.meeting_id.as_str())
        .collect()
}
