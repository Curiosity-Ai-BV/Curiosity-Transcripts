use std::fs;
use std::path::{Path, PathBuf};

use curiosity_app::{
    delete_meeting_command, export_meeting_json_command, list_meetings_dto, meeting_detail_dto,
    rename_meeting_command, search_meetings_dto,
};
use curiosity_domain::{
    ArtifactKind, AudioArtifact, Meeting, ModelRun, RecordingSession, RecordingSource,
    SourceChannel, TranscriptSegment, TranscriptVersion,
};
use curiosity_store::Store;

fn test_root(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "curiosity-app-phase-four-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("test root");
    path
}

#[test]
fn phase_four_commands_find_open_rename_delete_and_export_without_provider_dependencies() {
    let root = test_root("commands");
    let export_root = test_root("commands-export");
    let store = migrated_store(&root);
    seed_meeting_with_transcript(&store, "meeting-1", "Planning", "local transcript");

    assert_eq!(list_meetings_dto(&store).expect("list")[0].title, "Planning");
    assert_eq!(
        meeting_detail_dto(&store, "meeting-1")
            .expect("open")
            .transcript_segments[0]
            .text,
        "local transcript"
    );

    store.rebuild_search_index().expect("rebuild search");
    assert_eq!(
        search_meetings_dto(&store, "local").expect("search")[0].meeting_id,
        "meeting-1"
    );

    let renamed = rename_meeting_command(&store, "meeting-1", "Renamed").expect("rename");
    assert_eq!(renamed.title, "Renamed");

    let exported = export_meeting_json_command(&store, "meeting-1", &export_root).expect("export");
    assert!(PathBuf::from(&exported.path).exists());

    let deleted = delete_meeting_command(&store, "meeting-1").expect("delete");
    assert_eq!(deleted.meeting_id, "meeting-1");
    assert!(deleted.deleted_private_artifacts.is_empty());
    assert_eq!(deleted.remaining_exports, vec![exported.path]);
    assert!(search_meetings_dto(&store, "local")
        .expect("search after delete")
        .is_empty());
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
