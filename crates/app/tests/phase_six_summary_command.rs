use std::fs;
use std::path::PathBuf;

use std::cell::Cell;

use curiosity_analysis::{AnalysisInput, AnalysisOutcome, FakeMeetingAnalyzer, MeetingAnalyzer};
use curiosity_app::{
    generate_summary_command, generate_summary_command_with_cancellation, AnalysisCommandState,
};
use curiosity_domain::{Meeting, ModelRun, SourceChannel, TranscriptSegment, TranscriptVersion};
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
    assert_eq!(
        dto.analysis.as_ref().expect("analysis").provider,
        "fake-local"
    );
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
fn generate_summary_command_rejects_networked_analysis_before_persistence() {
    let root = test_root("networked-analysis-gate");
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
                1_000,
                "We decided to ship the local recorder.",
                SourceChannel::Mixed,
                "run-1",
                "version-1",
            )],
        )
        .expect("persist transcript");

    let dto = generate_summary_command(
        &store,
        &NetworkedAnalyzer::new("hosted-model", "summary-v1"),
        "meeting-1",
        3_000,
    )
    .expect("networked analysis returns visible gate failure");

    assert!(store
        .current_analysis_result("meeting-1")
        .expect("read analysis")
        .is_none());
    assert_eq!(dto.state, AnalysisCommandState::Failed);
    assert!(dto.analysis.is_none());
    let failure = dto.failure.expect("failure");
    assert_eq!(failure.code, "hosted_provider_gated");
    assert!(failure.message.contains("hosted analysis requires"));
    assert!(failure
        .setup_guidance
        .contains("explicit key selection and transcript data-disclosure confirmation"));
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

#[test]
fn canceled_summary_command_does_not_persist_completed_analyzer_output() {
    let root = test_root("canceled-summary");
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
                1_000,
                "We decided to ship the local recorder.",
                SourceChannel::Mixed,
                "run-1",
                "version-1",
            )],
        )
        .expect("persist transcript");
    let cancelled = Cell::new(false);
    let analyzer = CancelAfterAnalyzer {
        inner: FakeMeetingAnalyzer::new("fake-model", "summary-v1"),
        cancelled: &cancelled,
    };

    let dto =
        generate_summary_command_with_cancellation(&store, &analyzer, "meeting-1", 3_000, || {
            cancelled.get()
        })
        .expect("summary cancellation should be non-fatal");

    assert!(
        dto.is_none(),
        "a cancel request after analysis but before persistence should suppress the command result"
    );
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

struct CancelAfterAnalyzer<'a> {
    inner: FakeMeetingAnalyzer,
    cancelled: &'a Cell<bool>,
}

impl MeetingAnalyzer for CancelAfterAnalyzer<'_> {
    fn analyze(&self, input: AnalysisInput) -> AnalysisOutcome {
        let outcome = self.inner.analyze(input);
        self.cancelled.set(true);
        outcome
    }
}

struct NetworkedAnalyzer {
    inner: FakeMeetingAnalyzer,
}

impl NetworkedAnalyzer {
    fn new(model_name: &str, prompt_template_version: &str) -> Self {
        Self {
            inner: FakeMeetingAnalyzer::new(model_name, prompt_template_version),
        }
    }
}

impl MeetingAnalyzer for NetworkedAnalyzer {
    fn analyze(&self, input: AnalysisInput) -> AnalysisOutcome {
        match self.inner.analyze(input) {
            AnalysisOutcome::Completed(mut analysis) => {
                analysis.provider = "hosted-test".to_string();
                analysis.network_used = true;
                AnalysisOutcome::Completed(analysis)
            }
            failure => failure,
        }
    }
}
