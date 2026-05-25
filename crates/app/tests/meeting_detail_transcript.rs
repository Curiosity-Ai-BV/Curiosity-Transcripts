use std::fs;
use std::path::PathBuf;

use curiosity_app::meeting_detail_dto;
use curiosity_domain::{Meeting, ModelRun, SourceChannel, TranscriptSegment, TranscriptVersion};
use curiosity_store::Store;

fn test_root(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "curiosity-app-phase-three-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("test root");
    path
}

#[test]
fn meeting_detail_dto_includes_transcript_segments_from_sqlite() {
    let root = test_root("meeting-detail");
    let store = Store::open(root.join("app.db"), root).expect("open store");
    store.migrate().expect("migrate");
    store
        .insert_meeting(&Meeting::new_manual("meeting-1", "Planning", 1_000))
        .expect("insert meeting");
    let run = ModelRun::new(
        "run-1",
        "meeting-1",
        "sha256:audio",
        "fake-local",
        "fixture-whisper",
        false,
        2_000,
    );
    let version = TranscriptVersion::new("version-1", "meeting-1", "run-1", 1, 2_010);
    store
        .persist_transcript(
            &run,
            &version,
            &[TranscriptSegment::with_metadata(
                "segment-1",
                "meeting-1",
                0,
                1_200,
                "hello",
                SourceChannel::Mixed,
                "run-1",
                "version-1",
            )],
        )
        .expect("persist transcript");

    let dto = meeting_detail_dto(&store, "meeting-1").expect("meeting detail");

    assert_eq!(dto.meeting_id, "meeting-1");
    assert_eq!(dto.title, "Planning");
    assert_eq!(dto.transcript_segments.len(), 1);
    assert_eq!(dto.transcript_segments[0].text, "hello");
    assert_eq!(dto.transcript_segments[0].start_ms, 0);
    assert_eq!(dto.transcript_segments[0].source_channel, "Mixed");
    assert_eq!(dto.transcript_segments[0].model_run_id, "run-1");
}
