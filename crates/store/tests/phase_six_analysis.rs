use std::fs;
use std::path::PathBuf;

use curiosity_domain::{AnalysisCitation, Meeting, MeetingAnalysis};
use curiosity_store::Store;

fn test_root(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "curiosity-store-phase-six-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("test root");
    path
}

#[test]
fn analysis_result_persistence_records_provider_metadata_and_avoids_duplicate_current_rows() {
    let root = test_root("metadata-idempotence");
    let store = Store::open(root.join("app.db"), root).expect("open store");
    store.migrate().expect("migrate");
    store
        .insert_meeting(&Meeting::new_manual("meeting-1", "Planning", 1_000))
        .expect("insert meeting");
    let analysis = MeetingAnalysis {
        id: "analysis-1".to_string(),
        meeting_id: "meeting-1".to_string(),
        provider: "ollama".to_string(),
        model_name: "llama3.2".to_string(),
        network_used: false,
        created_at_ms: 2_000,
        prompt_template_version: "summary-v1".to_string(),
        summary: "Local summary".to_string(),
        decisions: Vec::new(),
        action_items: Vec::new(),
        questions: Vec::new(),
        citations: vec![AnalysisCitation {
            segment_id: "segment-1".to_string(),
            start_ms: 0,
            end_ms: 1_000,
        }],
    };

    store
        .persist_analysis_result(&analysis)
        .expect("persist analysis");
    store
        .persist_analysis_result(&analysis)
        .expect("persist same analysis again");
    let stored = store
        .current_analysis_result("meeting-1")
        .expect("read current analysis")
        .expect("analysis exists");

    assert_eq!(store.count("analysis_results").expect("analysis count"), 1);
    assert_eq!(stored.provider, "ollama");
    assert_eq!(stored.model_name, "llama3.2");
    assert!(!stored.network_used);
    assert_eq!(stored.created_at_ms, 2_000);
    assert_eq!(stored.prompt_template_version, "summary-v1");
    assert_eq!(stored.citations[0].segment_id, "segment-1");
}

#[test]
fn divergent_analysis_replay_returns_conflict_and_preserves_first_result() {
    let root = test_root("divergent-replay");
    let store = Store::open(root.join("app.db"), root).expect("open store");
    store.migrate().expect("migrate");
    store
        .insert_meeting(&Meeting::new_manual("meeting-1", "Planning", 1_000))
        .expect("insert meeting");
    let first = analysis("analysis-1", "First local summary");
    let mut divergent = analysis("analysis-2", "Changed local summary");
    divergent.created_at_ms = 3_000;

    store
        .persist_analysis_result(&first)
        .expect("persist first analysis");
    let err = store
        .persist_analysis_result(&divergent)
        .expect_err("divergent replay should conflict");
    let stored = store
        .current_analysis_result("meeting-1")
        .expect("read current analysis")
        .expect("analysis exists");

    assert!(err.to_string().contains("analysis replay conflict"));
    assert_eq!(store.count("analysis_results").expect("analysis count"), 1);
    assert_eq!(stored.id, "analysis-1");
    assert_eq!(stored.summary, "First local summary");
    assert_eq!(stored.created_at_ms, 2_000);
}

fn analysis(id: &str, summary: &str) -> MeetingAnalysis {
    MeetingAnalysis {
        id: id.to_string(),
        meeting_id: "meeting-1".to_string(),
        provider: "ollama".to_string(),
        model_name: "llama3.2".to_string(),
        network_used: false,
        created_at_ms: 2_000,
        prompt_template_version: "summary-v1".to_string(),
        summary: summary.to_string(),
        decisions: Vec::new(),
        action_items: Vec::new(),
        questions: Vec::new(),
        citations: vec![AnalysisCitation {
            segment_id: "segment-1".to_string(),
            start_ms: 0,
            end_ms: 1_000,
        }],
    }
}
