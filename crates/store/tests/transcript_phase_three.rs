use std::fs;
use std::path::{Path, PathBuf};

use curiosity_domain::{
    ArtifactKind, AudioArtifact, Meeting, ModelRun, RecordingSession, RecordingSource,
    RecordingStatus, SourceChannel, TranscriptSegment, TranscriptVersion,
};
use curiosity_store::{Store, StoreError};
use rusqlite::Connection;

fn test_root(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "curiosity-store-phase-three-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("test root");
    path
}

#[test]
fn transcript_persistence_keeps_timing_source_model_run_and_version_metadata() {
    let root = test_root("metadata");
    let store = migrated_store(&root);
    seed_meeting_with_audio(&store);
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
    let segments = vec![
        TranscriptSegment::with_metadata(
            "segment-1",
            "meeting-1",
            0,
            1_200,
            "hello",
            SourceChannel::Microphone,
            "run-1",
            "version-1",
        ),
        TranscriptSegment::with_metadata(
            "segment-2",
            "meeting-1",
            1_200,
            2_000,
            "there",
            SourceChannel::System,
            "run-1",
            "version-1",
        ),
    ];

    store
        .persist_transcript(&run, &version, &segments)
        .expect("persist transcript");
    let stored = store
        .transcript_segments("meeting-1")
        .expect("read transcript");

    assert_eq!(stored.len(), 2);
    assert_eq!(stored[0].start_ms, 0);
    assert_eq!(stored[0].end_ms, 1_200);
    assert_eq!(stored[0].source_channel, SourceChannel::Microphone);
    assert_eq!(stored[0].model_run_id, "run-1");
    assert_eq!(stored[0].transcript_version_id, "version-1");
    assert_eq!(stored[1].source_channel, SourceChannel::System);
}

#[test]
fn importing_same_audio_and_transcript_twice_is_idempotent_by_meeting_artifact_provider_model_and_version(
) {
    let root = test_root("idempotent");
    let store = migrated_store(&root);
    seed_meeting_with_audio(&store);
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
    let segments = vec![TranscriptSegment::with_metadata(
        "segment-1",
        "meeting-1",
        0,
        1_200,
        "hello",
        SourceChannel::Imported,
        "run-1",
        "version-1",
    )];

    store
        .persist_transcript(&run, &version, &segments)
        .expect("first import");
    store
        .persist_transcript(&run, &version, &segments)
        .expect("second import");

    assert_eq!(store.count("model_runs").expect("model runs"), 1);
    assert_eq!(store.count("transcript_versions").expect("versions"), 1);
    assert_eq!(store.count("transcript_segments").expect("segments"), 1);
}

#[test]
fn importing_same_audio_artifact_twice_preserves_original_row_by_session_kind_and_hash() {
    let root = test_root("idempotent-audio-artifact");
    let store = migrated_store(&root);
    seed_meeting_session(&store);
    let first = AudioArtifact::new_private(
        "artifact-first",
        "session-1",
        ArtifactKind::Imported,
        "meetings/meeting-1/audio/imported-first.wav",
        "sha256:audio",
    );
    let regenerated = AudioArtifact::new_private(
        "artifact-regenerated",
        "session-1",
        ArtifactKind::Imported,
        "meetings/meeting-1/audio/imported-second.wav",
        "sha256:audio",
    );

    let first_id = store
        .insert_audio_artifact(&first)
        .expect("first audio import");
    let regenerated_id = store
        .insert_audio_artifact(&regenerated)
        .expect("same audio import with regenerated artifact id");

    assert_eq!(first_id, "artifact-first");
    assert_eq!(regenerated_id, "artifact-first");
    assert_eq!(store.count("audio_artifacts").expect("audio artifacts"), 1);
}

#[test]
fn importing_same_generated_audio_artifact_id_twice_is_idempotent_for_the_same_content() {
    let root = test_root("idempotent-audio-artifact-id");
    let store = migrated_store(&root);
    seed_meeting_session(&store);
    let artifact = AudioArtifact::new_private(
        "artifact-1",
        "session-1",
        ArtifactKind::Imported,
        "meetings/meeting-1/audio/imported.wav",
        "sha256:audio",
    );

    let first_id = store
        .insert_audio_artifact(&artifact)
        .expect("first audio import");
    let second_id = store
        .insert_audio_artifact(&artifact)
        .expect("same generated artifact import");

    assert_eq!(first_id, "artifact-1");
    assert_eq!(second_id, "artifact-1");
    assert_eq!(store.count("audio_artifacts").expect("audio artifacts"), 1);
}

#[test]
fn meeting_transcript_read_returns_only_latest_transcript_version_for_the_meeting() {
    let root = test_root("current-version-read");
    let store = migrated_store(&root);
    seed_meeting_with_audio(&store);
    let first_run = ModelRun::new(
        "run-1",
        "meeting-1",
        "sha256:audio-v1",
        "fake-local",
        "fixture-whisper",
        false,
        2_000,
    );
    let first_version = TranscriptVersion::new("version-1", "meeting-1", "run-1", 1, 2_010);
    let second_run = ModelRun::new(
        "run-2",
        "meeting-1",
        "sha256:audio-v2",
        "fake-local",
        "fixture-whisper",
        false,
        3_000,
    );
    let second_version = TranscriptVersion::new("version-2", "meeting-1", "run-2", 1, 3_010);

    store
        .persist_transcript(
            &first_run,
            &first_version,
            &[TranscriptSegment::with_metadata(
                "segment-old",
                "meeting-1",
                0,
                1_000,
                "old transcript",
                SourceChannel::Imported,
                "run-1",
                "version-1",
            )],
        )
        .expect("persist first transcript");
    store
        .persist_transcript(
            &second_run,
            &second_version,
            &[TranscriptSegment::with_metadata(
                "segment-current",
                "meeting-1",
                0,
                1_000,
                "current transcript",
                SourceChannel::Imported,
                "run-2",
                "version-2",
            )],
        )
        .expect("persist current transcript");

    let stored = store
        .transcript_segments("meeting-1")
        .expect("read current transcript");

    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].text, "current transcript");
    assert_eq!(stored[0].transcript_version_id, "version-2");
}

#[test]
fn transcript_idempotency_uses_content_key_not_generated_run_or_segment_ids() {
    let root = test_root("idempotent-generated-ids");
    let store = migrated_store(&root);
    seed_meeting_with_audio(&store);
    let first_run = ModelRun::new(
        "run-first",
        "meeting-1",
        "sha256:audio",
        "fake-local",
        "fixture-whisper",
        false,
        2_000,
    );
    let first_version = TranscriptVersion::new("version-first", "meeting-1", "run-first", 1, 2_010);
    let second_run = ModelRun::new(
        "run-second",
        "meeting-1",
        "sha256:audio",
        "fake-local",
        "fixture-whisper",
        false,
        2_500,
    );
    let second_version =
        TranscriptVersion::new("version-second", "meeting-1", "run-second", 1, 2_510);

    store
        .persist_transcript(
            &first_run,
            &first_version,
            &[TranscriptSegment::with_metadata(
                "segment-first",
                "meeting-1",
                0,
                1_200,
                "hello",
                SourceChannel::Imported,
                "run-first",
                "version-first",
            )],
        )
        .expect("first import");
    store
        .persist_transcript(
            &second_run,
            &second_version,
            &[TranscriptSegment::with_metadata(
                "segment-second",
                "meeting-1",
                0,
                1_200,
                "hello",
                SourceChannel::Imported,
                "run-second",
                "version-second",
            )],
        )
        .expect("same import with regenerated ids");

    let stored = store
        .transcript_segments("meeting-1")
        .expect("read transcript");
    assert_eq!(store.count("model_runs").expect("model runs"), 1);
    assert_eq!(store.count("transcript_versions").expect("versions"), 1);
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].id, "segment-first");
    assert_eq!(stored[0].model_run_id, "run-first");
    assert_eq!(stored[0].transcript_version_id, "version-first");
}

#[test]
fn replaying_same_transcript_key_with_changed_segments_returns_conflict_instead_of_silently_ignoring(
) {
    let root = test_root("divergent-transcript-replay");
    let store = migrated_store(&root);
    seed_meeting_with_audio(&store);
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
                SourceChannel::Imported,
                "run-1",
                "version-1",
            )],
        )
        .expect("first import");

    let err = store
        .persist_transcript(
            &run,
            &version,
            &[TranscriptSegment::with_metadata(
                "segment-1",
                "meeting-1",
                0,
                1_200,
                "changed text",
                SourceChannel::Imported,
                "run-1",
                "version-1",
            )],
        )
        .expect_err("divergent replay should conflict");

    assert!(err.to_string().contains("transcript replay conflict"));
    let stored = store
        .transcript_segments("meeting-1")
        .expect("read transcript");
    assert_eq!(stored[0].text, "hello");
}

#[test]
fn replaying_same_transcript_key_with_changed_segment_count_returns_conflict() {
    let root = test_root("divergent-transcript-count");
    let store = migrated_store(&root);
    seed_meeting_with_audio(&store);
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
    let first = vec![TranscriptSegment::with_metadata(
        "segment-1",
        "meeting-1",
        0,
        1_200,
        "hello",
        SourceChannel::Imported,
        "run-1",
        "version-1",
    )];
    let divergent = vec![
        TranscriptSegment::with_metadata(
            "segment-1",
            "meeting-1",
            0,
            1_200,
            "hello",
            SourceChannel::Imported,
            "run-1",
            "version-1",
        ),
        TranscriptSegment::with_metadata(
            "segment-2",
            "meeting-1",
            1_200,
            2_000,
            "extra",
            SourceChannel::Imported,
            "run-1",
            "version-1",
        ),
    ];

    store
        .persist_transcript(&run, &version, &first)
        .expect("first import");
    let err = store
        .persist_transcript(&run, &version, &divergent)
        .expect_err("changed segment count should conflict");

    assert!(err.to_string().contains("transcript replay conflict"));
    assert_eq!(store.count("transcript_segments").expect("segments"), 1);
}

#[test]
fn replaying_same_transcript_key_with_changed_model_metadata_returns_conflict() {
    let root = test_root("divergent-transcript-model-metadata");
    let store = migrated_store(&root);
    seed_meeting_with_audio(&store);
    let run = ModelRun::new(
        "run-1",
        "meeting-1",
        "sha256:audio",
        "fake-local",
        "fixture-whisper",
        false,
        2_000,
    );
    let changed_metadata = ModelRun::new(
        "run-1",
        "meeting-1",
        "sha256:audio",
        "fake-local",
        "fixture-whisper",
        true,
        2_000,
    );
    let version = TranscriptVersion::new("version-1", "meeting-1", "run-1", 1, 2_010);
    let segments = vec![TranscriptSegment::with_metadata(
        "segment-1",
        "meeting-1",
        0,
        1_200,
        "hello",
        SourceChannel::Imported,
        "run-1",
        "version-1",
    )];

    store
        .persist_transcript(&run, &version, &segments)
        .expect("first import");
    let err = store
        .persist_transcript(&changed_metadata, &version, &segments)
        .expect_err("changed model metadata should conflict");

    assert!(err.to_string().contains("transcript replay conflict"));
}

#[test]
fn failed_transcript_persist_rolls_back_version_and_partial_segments_so_retry_can_succeed() {
    let root = test_root("atomic-transcript-persist");
    let store = migrated_store(&root);
    seed_meeting_with_audio(&store);
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
    let duplicate_segment_ids = vec![
        TranscriptSegment::with_metadata(
            "segment-duplicate",
            "meeting-1",
            0,
            1_000,
            "first partial write",
            SourceChannel::Imported,
            "run-1",
            "version-1",
        ),
        TranscriptSegment::with_metadata(
            "segment-duplicate",
            "meeting-1",
            1_000,
            2_000,
            "second insert fails",
            SourceChannel::Imported,
            "run-1",
            "version-1",
        ),
    ];
    let valid_retry = vec![
        TranscriptSegment::with_metadata(
            "segment-1",
            "meeting-1",
            0,
            1_000,
            "first final segment",
            SourceChannel::Imported,
            "run-1",
            "version-1",
        ),
        TranscriptSegment::with_metadata(
            "segment-2",
            "meeting-1",
            1_000,
            2_000,
            "second final segment",
            SourceChannel::Imported,
            "run-1",
            "version-1",
        ),
    ];

    store
        .persist_transcript(&run, &version, &duplicate_segment_ids)
        .expect_err("duplicate segment id should fail mid-persist");

    assert_eq!(store.count("transcript_versions").expect("versions"), 0);
    assert_eq!(store.count("transcript_segments").expect("segments"), 0);
    assert!(store
        .transcript_segments("meeting-1")
        .expect("visible transcript after failed persist")
        .is_empty());

    store
        .persist_transcript(&run, &version, &valid_retry)
        .expect("valid retry after rollback");
    let stored = store
        .transcript_segments("meeting-1")
        .expect("read retried transcript");

    assert_eq!(stored.len(), 2);
    assert_eq!(stored[0].text, "first final segment");
    assert_eq!(stored[1].text, "second final segment");
}

#[test]
fn correcting_transcript_text_preserves_original_timing_and_original_text() {
    let root = test_root("correction");
    let store = migrated_store(&root);
    seed_meeting_with_audio(&store);
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
                "helo world",
                SourceChannel::Mixed,
                "run-1",
                "version-1",
            )],
        )
        .expect("persist transcript");

    store
        .correct_transcript_segment("segment-1", "hello world", 2_500)
        .expect("correct transcript");
    let stored = store
        .transcript_segments("meeting-1")
        .expect("read transcript");

    assert_eq!(stored[0].text, "hello world");
    assert_eq!(stored[0].original_text.as_deref(), Some("helo world"));
    assert_eq!(stored[0].start_ms, 0);
    assert_eq!(stored[0].end_ms, 1_200);
    assert_eq!(stored[0].transcript_version_id, "version-1");
}

#[test]
fn transcript_replay_conflict_returns_typed_error_and_preserves_display_text() {
    let root = test_root("typed-replay-conflict");
    let store = migrated_store(&root);
    seed_meeting_with_audio(&store);
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
    let original = TranscriptSegment::with_metadata(
        "segment-1",
        "meeting-1",
        0,
        1_200,
        "hello world",
        SourceChannel::Mixed,
        "run-1",
        "version-1",
    );
    store
        .persist_transcript(&run, &version, std::slice::from_ref(&original))
        .expect("persist transcript");

    let changed = TranscriptSegment::with_metadata(
        "segment-1",
        "meeting-1",
        0,
        1_200,
        "changed text",
        SourceChannel::Mixed,
        "run-1",
        "version-1",
    );
    let err = store
        .persist_transcript(&run, &version, &[changed])
        .expect_err("changed transcript replay should conflict");

    assert_eq!(
        err.to_string(),
        "transcript replay conflict: segment content changed"
    );
    match err {
        StoreError::ReplayConflict(message) => {
            assert_eq!(
                message,
                "transcript replay conflict: segment content changed"
            );
        }
        other => panic!("expected replay conflict, got {other:?}"),
    }
}

#[test]
fn failed_transcript_correction_rolls_back_text_history_version_and_search_state() {
    let root = test_root("correction-atomic-search-failure");
    let db_path = root.join("app.db");
    let store = Store::open(&db_path, root.to_path_buf()).expect("open store");
    store.migrate().expect("migrate");
    seed_meeting_with_audio(&store);
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
                "original atomic phrase",
                SourceChannel::Mixed,
                "run-1",
                "version-1",
            )],
        )
        .expect("persist transcript");

    let db = Connection::open(&db_path).expect("open sqlite");
    db.execute_batch(
        "
        DROP TABLE meeting_search;
        CREATE TABLE meeting_search (
            meeting_id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            transcript_text TEXT NOT NULL,
            CHECK (instr(transcript_text, 'corrected atomic phrase') = 0)
        );
        INSERT INTO meeting_search (meeting_id, title, transcript_text)
        VALUES ('meeting-1', 'Planning', 'original atomic phrase');
        ",
    )
    .expect("replace search table with failing test double");

    let err = store
        .correct_transcript_segment("segment-1", "corrected atomic phrase", 2_500)
        .expect_err("search refresh failure should abort correction");

    assert!(err.to_string().contains("CHECK constraint failed"));
    let stored = store
        .transcript_segments("meeting-1")
        .expect("read transcript");
    let edits = store
        .transcript_segment_edits("segment-1")
        .expect("read edit history");
    let edited_at_ms: Option<u64> = db
        .query_row(
            "SELECT edited_at_ms FROM transcript_versions WHERE id = 'version-1'",
            [],
            |row| row.get(0),
        )
        .expect("read version edit timestamp");
    let indexed_text: String = db
        .query_row(
            "SELECT transcript_text FROM meeting_search WHERE meeting_id = 'meeting-1'",
            [],
            |row| row.get(0),
        )
        .expect("read search row");

    assert_eq!(stored[0].text, "original atomic phrase");
    assert_eq!(stored[0].original_text, None);
    assert!(edits.is_empty());
    assert_eq!(edited_at_ms, None);
    assert_eq!(indexed_text, "original atomic phrase");
}

#[test]
fn multiple_transcript_corrections_preserve_full_edit_trail_without_changing_timing() {
    let root = test_root("correction-history");
    let store = migrated_store(&root);
    seed_meeting_with_audio(&store);
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
                "helo wrld",
                SourceChannel::Mixed,
                "run-1",
                "version-1",
            )],
        )
        .expect("persist transcript");

    store
        .correct_transcript_segment("segment-1", "hello wrld", 2_500)
        .expect("first correction");
    store
        .correct_transcript_segment("segment-1", "hello world", 2_700)
        .expect("second correction");

    let stored = store
        .transcript_segments("meeting-1")
        .expect("read transcript");
    let edits = store
        .transcript_segment_edits("segment-1")
        .expect("read edit history");

    assert_eq!(stored[0].text, "hello world");
    assert_eq!(stored[0].start_ms, 0);
    assert_eq!(stored[0].end_ms, 1_200);
    assert_eq!(edits.len(), 2);
    assert_eq!(edits[0].previous_text, "helo wrld");
    assert_eq!(edits[0].corrected_text, "hello wrld");
    assert_eq!(edits[1].previous_text, "hello wrld");
    assert_eq!(edits[1].corrected_text, "hello world");
}

#[test]
fn completed_wav_artifact_for_transcription_excludes_incomplete_and_tombstoned_rows() {
    let root = test_root("completed-wav-for-transcription");
    let store = migrated_store(&root);
    seed_meeting_session(&store);
    store
        .insert_recording_session(&RecordingSession::start(
            "session-complete",
            "meeting-1",
            RecordingSource::Microphone,
            1_001,
            48_000,
        ))
        .expect("complete session");
    store
        .insert_recording_session(&RecordingSession::start(
            "session-cross-meeting-path",
            "meeting-1",
            RecordingSource::Microphone,
            1_003,
            48_000,
        ))
        .expect("cross-path session");
    store
        .insert_recording_session(&RecordingSession::start(
            "session-tombstoned",
            "meeting-1",
            RecordingSource::Microphone,
            1_002,
            48_000,
        ))
        .expect("tombstoned session");

    store
        .insert_audio_artifact(&AudioArtifact::new_private(
            "artifact-incomplete",
            "session-1",
            ArtifactKind::RawMic,
            "meetings/meeting-1/audio/incomplete.wav",
            "sha256:pending:artifact-incomplete",
        ))
        .expect("incomplete artifact");
    store
        .insert_audio_artifact(&AudioArtifact::new_private(
            "artifact-complete",
            "session-complete",
            ArtifactKind::RawMic,
            "meetings/meeting-1/audio/raw-mic.wav",
            "sha256:pending:artifact-complete",
        ))
        .expect("complete artifact");
    store
        .insert_audio_artifact(&AudioArtifact::new_private(
            "artifact-cross-meeting-path",
            "session-cross-meeting-path",
            ArtifactKind::RawMic,
            "meetings/other-meeting/audio/raw-mic.wav",
            "sha256:pending:artifact-cross-meeting-path",
        ))
        .expect("cross-path artifact");
    store
        .insert_audio_artifact(&AudioArtifact::new_private(
            "artifact-tombstoned",
            "session-tombstoned",
            ArtifactKind::RawMic,
            "meetings/meeting-1/audio/tombstoned.wav",
            "sha256:pending:artifact-tombstoned",
        ))
        .expect("tombstoned artifact");
    store
        .complete_audio_artifact("artifact-complete", "sha256:complete")
        .expect("mark complete");
    store
        .complete_audio_artifact("artifact-tombstoned", "sha256:tombstoned")
        .expect("mark tombstoned complete");
    store
        .complete_audio_artifact("artifact-cross-meeting-path", "sha256:cross")
        .expect("mark cross-path complete");
    store
        .tombstone_audio_artifact("artifact-tombstoned")
        .expect("mark tombstoned");
    store
        .update_recording_session_status(
            "session-complete",
            RecordingStatus::Complete,
            Some(1_100),
            None,
        )
        .expect("complete selected session");
    store
        .update_recording_session_status(
            "session-cross-meeting-path",
            RecordingStatus::Complete,
            Some(1_100),
            None,
        )
        .expect("complete cross-path session");
    store
        .update_recording_session_status(
            "session-tombstoned",
            RecordingStatus::Complete,
            Some(1_100),
            None,
        )
        .expect("complete tombstoned session");

    let artifact = store
        .completed_wav_artifact_for_transcription("meeting-1")
        .expect("select artifact")
        .expect("complete wav artifact");

    assert_eq!(artifact.artifact_id, "artifact-complete");
    assert_eq!(artifact.recording_session_id, "session-complete");
    assert_eq!(artifact.path, "meetings/meeting-1/audio/raw-mic.wav");
    assert_eq!(artifact.sha256, "sha256:complete");
}

#[test]
fn completed_wav_artifacts_for_transcription_returns_latest_private_mic_then_system_pair() {
    let root = test_root("completed-wav-bundle-for-transcription");
    let store = migrated_store(&root);
    seed_meeting_session(&store);
    store
        .insert_recording_session(&RecordingSession::start(
            "session-mixed",
            "meeting-1",
            RecordingSource::Mixed,
            1_001,
            48_000,
        ))
        .expect("mixed session");
    store
        .insert_recording_session(&RecordingSession::start(
            "session-newer-unsafe",
            "meeting-1",
            RecordingSource::Microphone,
            1_002,
            48_000,
        ))
        .expect("unsafe session");

    store
        .insert_audio_artifact(&AudioArtifact::new_private(
            "artifact-system",
            "session-mixed",
            ArtifactKind::RawSystem,
            "meetings/meeting-1/audio/recording-1/raw-system.wav",
            "sha256:pending:system",
        ))
        .expect("system artifact");
    store
        .insert_audio_artifact(&AudioArtifact::new_private(
            "artifact-mic",
            "session-mixed",
            ArtifactKind::RawMic,
            "meetings/meeting-1/audio/recording-1/raw-mic.wav",
            "sha256:pending:mic",
        ))
        .expect("mic artifact");
    store
        .insert_audio_artifact(&AudioArtifact::new_private(
            "artifact-unsafe",
            "session-newer-unsafe",
            ArtifactKind::RawMic,
            "../escape.wav",
            "sha256:pending:unsafe",
        ))
        .expect("unsafe artifact");
    store
        .complete_audio_artifact("artifact-system", "sha256:system")
        .expect("complete system");
    store
        .complete_audio_artifact("artifact-mic", "sha256:mic")
        .expect("complete mic");
    store
        .complete_audio_artifact("artifact-unsafe", "sha256:unsafe")
        .expect("complete unsafe");
    store
        .update_recording_session_status(
            "session-mixed",
            RecordingStatus::Complete,
            Some(1_100),
            None,
        )
        .expect("complete mixed session");
    store
        .update_recording_session_status(
            "session-newer-unsafe",
            RecordingStatus::Complete,
            Some(1_100),
            None,
        )
        .expect("complete unsafe session");

    let artifacts = store
        .completed_wav_artifacts_for_transcription("meeting-1")
        .expect("select artifacts");

    assert_eq!(
        artifacts
            .iter()
            .map(|artifact| artifact.artifact_id.as_str())
            .collect::<Vec<_>>(),
        vec!["artifact-mic", "artifact-system"]
    );
    assert_eq!(artifacts[0].kind, "RawMic");
    assert_eq!(artifacts[1].kind, "RawSystem");
}

#[test]
fn completed_wav_artifacts_for_transcription_rejects_partial_mixed_session() {
    let root = test_root("completed-wav-partial-mixed");
    let store = migrated_store(&root);
    seed_meeting_session(&store);
    store
        .insert_recording_session(&RecordingSession::start(
            "session-mixed",
            "meeting-1",
            RecordingSource::Mixed,
            1_001,
            48_000,
        ))
        .expect("mixed session");
    store
        .insert_audio_artifact(&AudioArtifact::new_private(
            "artifact-mic",
            "session-mixed",
            ArtifactKind::RawMic,
            "meetings/meeting-1/audio/recording-1/raw-mic.wav",
            "sha256:pending:mic",
        ))
        .expect("mic artifact");
    store
        .complete_audio_artifact("artifact-mic", "sha256:mic")
        .expect("complete mic");
    store
        .update_recording_session_status(
            "session-mixed",
            RecordingStatus::Complete,
            Some(1_100),
            None,
        )
        .expect("complete mixed session");

    let artifacts = store
        .completed_wav_artifacts_for_transcription("meeting-1")
        .expect("select artifacts");

    assert!(artifacts.is_empty());
}

fn migrated_store(root: &Path) -> Store {
    let store = Store::open(root.join("app.db"), root.to_path_buf()).expect("open store");
    store.migrate().expect("migrate");
    store
}

fn seed_meeting_with_audio(store: &Store) {
    seed_meeting_session(store);
    store
        .insert_audio_artifact(&AudioArtifact::new_private(
            "artifact-1",
            "session-1",
            ArtifactKind::Imported,
            "meetings/meeting-1/audio/imported.wav",
            "sha256:audio",
        ))
        .expect("insert artifact");
}

fn seed_meeting_session(store: &Store) {
    let meeting = Meeting::new_manual("meeting-1", "Planning", 1_000);
    store.insert_meeting(&meeting).expect("insert meeting");
    let session = RecordingSession::start(
        "session-1",
        "meeting-1",
        RecordingSource::Imported,
        1_000,
        48_000,
    );
    store
        .insert_recording_session(&session)
        .expect("insert session");
}
