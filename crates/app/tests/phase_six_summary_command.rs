use std::fs;
use std::path::PathBuf;

use std::cell::Cell;

use curiosity_analysis::{AnalysisInput, AnalysisOutcome, FakeMeetingAnalyzer, MeetingAnalyzer};
use curiosity_app::{generate_summary_command, AnalysisCommandState};
use curiosity_domain::{
    Meeting, ModelRun, SourceChannel, TranscriptSegment, TranscriptVersion,
};
use curiosity_store::Store;

fn test_root(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "curiosity-app-phase-six-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("test root");
    path
}

#[test]
fn generate_summary_command_uses_current_transcript_and_persists_structured_result() {
    let root = test_root("generate-summary");
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
            &[
                TranscriptSegment::with_metadata(
                    "segment-1",
                    "meeting-1",
                    0,
                    1_000,
                    "We decided to ship the local recorder.",
                    SourceChannel::Mixed,
                    "run-1",
                    "version-1",
                ),
                TranscriptSegment::with_metadata(
                    "segment-2",
                    "meeting-1",
                    1_000,
                    2_000,
                    "Alex will update the summary tests by Friday.",
                    SourceChannel::Mixed,
                    "run-1",
                    "version-1",
                ),
            ],
        )
        .expect("persist transcript");

    let dto = generate_summary_command(
        &store,
        &FakeMeetingAnalyzer::new("fake-model", "summary-v1"),
        "meeting-1",
        3_000,
    )
    .expect("generate summary");

    assert_eq!(dto.state, AnalysisCommandState::Complete);
    assert_eq!(dto.analysis.as_ref().expect("analysis").provider, "fake-local");
    assert_eq!(
        store
            .current_analysis_result("meeting-1")
            .expect("read persisted analysis")
            .expect("analysis exists")
            .prompt_template_version,
        "summary-v1"
    );
}

#[test]
fn generate_summary_command_rejects_missing_transcript_before_analyzer_call() {
    let root = test_root("missing-transcript");
    let store = Store::open(root.join("app.db"), root).expect("open store");
    store.migrate().expect("migrate");
    store
        .insert_meeting(&Meeting::new_manual("meeting-1", "Planning", 1_000))
        .expect("insert meeting");
    let analyzer = CountingAnalyzer::new();

    let dto = generate_summary_command(&store, &analyzer, "meeting-1", 3_000)
        .expect("missing transcript returns visible failure dto");

    assert_eq!(dto.state, AnalysisCommandState::Failed);
    assert_eq!(
        dto.failure.as_ref().expect("failure").code,
        "no_transcript_segments"
    );
    assert_eq!(analyzer.call_count(), 0);
    assert!(store
        .current_analysis_result("meeting-1")
        .expect("read analysis")
        .is_none());
}

struct CountingAnalyzer {
    calls: Cell<u32>,
}

impl CountingAnalyzer {
    fn new() -> Self {
        Self {
            calls: Cell::new(0),
        }
    }

    fn call_count(&self) -> u32 {
        self.calls.get()
    }
}

impl MeetingAnalyzer for CountingAnalyzer {
    fn analyze(&self, _input: AnalysisInput) -> AnalysisOutcome {
        self.calls.set(self.calls.get() + 1);
        panic!("app command should reject empty transcript before analyzer call");
    }
}
