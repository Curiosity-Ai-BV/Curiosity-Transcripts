use curiosity_domain::SourceChannel;
use curiosity_transcription::{
    export_json, export_markdown, export_srt, AudioFixture, FakeLocalTranscriber, FixtureLine,
    LocalTranscriber, ModelState,
};

#[test]
fn fake_local_transcriber_turns_fixture_lines_into_ordered_segments_without_hardware() {
    let transcriber = FakeLocalTranscriber::new("fake-local", "fixture-whisper", 1);
    let fixture = AudioFixture {
        meeting_id: "meeting-1".to_string(),
        source_artifact_sha256: "sha256:audio-fixture".to_string(),
        lines: vec![
            FixtureLine {
                start_ms: 1_500,
                end_ms: 2_500,
                source_channel: SourceChannel::System,
                text: "second".to_string(),
            },
            FixtureLine {
                start_ms: 0,
                end_ms: 1_000,
                source_channel: SourceChannel::Microphone,
                text: "first".to_string(),
            },
        ],
    };

    let document = transcriber
        .transcribe_fixture(&fixture)
        .expect("fake transcription");

    assert_eq!(document.provider, "fake-local");
    assert_eq!(document.model_name, "fixture-whisper");
    assert_eq!(document.source_artifact_sha256, "sha256:audio-fixture");
    assert_eq!(document.segments[0].text, "first");
    assert_eq!(
        document.segments[0].source_channel,
        SourceChannel::Microphone
    );
    assert_eq!(document.segments[1].text, "second");
    assert_eq!(document.segments[1].source_channel, SourceChannel::System);
    assert_eq!(document.segments[0].model_run_id, document.model_run_id);
    assert_eq!(
        document.segments[0].transcript_version_id,
        document.transcript_version_id
    );
}

#[test]
fn model_state_machine_names_local_setup_failures_without_starting_downloads_or_network() {
    let states = [
        ModelState::Missing,
        ModelState::Downloading {
            downloaded_bytes: 256,
            total_bytes: 1_024,
        },
        ModelState::Ready {
            model_name: "fixture-whisper".to_string(),
            sha256: "sha256:model".to_string(),
        },
        ModelState::FailedHash {
            expected_sha256: "sha256:expected".to_string(),
            actual_sha256: "sha256:actual".to_string(),
        },
        ModelState::IncompatibleHardware {
            reason: "missing acceleration feature".to_string(),
        },
    ];

    assert_eq!(states.len(), 5);
}

#[test]
fn transcript_exports_are_deterministic_markdown_json_and_srt_strings() {
    let segments = FakeLocalTranscriber::new("fake-local", "fixture-whisper", 1)
        .transcribe_fixture(&AudioFixture {
            meeting_id: "meeting-1".to_string(),
            source_artifact_sha256: "sha256:audio-fixture".to_string(),
            lines: vec![
                FixtureLine {
                    start_ms: 0,
                    end_ms: 1_234,
                    source_channel: SourceChannel::Mixed,
                    text: "hello world".to_string(),
                },
                FixtureLine {
                    start_ms: 65_000,
                    end_ms: 66_250,
                    source_channel: SourceChannel::Mixed,
                    text: "next minute".to_string(),
                },
            ],
        })
        .expect("fake transcription")
        .segments;

    assert_eq!(
        export_markdown(&segments),
        "- [00:00] hello world\n- [01:05] next minute"
    );
    assert!(export_json(&segments)
        .expect("json export")
        .contains("\"source_channel\": \"Mixed\""));
    assert_eq!(
        export_srt(&segments),
        "1\n00:00:00,000 --> 00:00:01,234\nhello world\n\n2\n00:01:05,000 --> 00:01:06,250\nnext minute\n"
    );
}
