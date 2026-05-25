use std::cell::Cell;

use curiosity_analysis::{
    recommended_analysis_model_presets, AnalysisClientError, AnalysisInput, AnalysisOutcome,
    AnalysisProviderKind, FakeMeetingAnalyzer, HostedAnalysisConfig, OllamaAnalyzer,
    OpenAiCompatibleAnalyzer, ProviderTextClient,
};
use curiosity_domain::{SourceChannel, TranscriptSegment};

fn input() -> AnalysisInput {
    AnalysisInput {
        meeting_id: "meeting-1".to_string(),
        created_at_ms: 10_000,
        transcript_segments: vec![
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
            TranscriptSegment::with_metadata(
                "segment-3",
                "meeting-1",
                2_000,
                3_000,
                "Should we keep hosted analysis disabled by default?",
                SourceChannel::Mixed,
                "run-1",
                "version-1",
            ),
        ],
    }
}

#[test]
fn fake_analyzer_returns_structured_summary_with_citations() {
    let outcome = FakeMeetingAnalyzer::new("fake-model", "summary-v1").analyze(input());

    let AnalysisOutcome::Completed(analysis) = outcome else {
        panic!("fake analyzer should complete");
    };
    assert_eq!(analysis.provider, "fake-local");
    assert_eq!(analysis.model_name, "fake-model");
    assert!(!analysis.network_used);
    assert_eq!(analysis.prompt_template_version, "summary-v1");
    assert!(analysis.summary.contains("local recorder"));
    assert_eq!(analysis.decisions[0].text, "ship the local recorder");
    assert_eq!(analysis.action_items[0].owner.as_deref(), Some("Alex"));
    assert_eq!(analysis.questions[0].citations[0].segment_id, "segment-3");
    assert_eq!(analysis.citations[0].start_ms, 0);
}

#[test]
fn generated_analysis_ids_keep_punctuation_distinct() {
    let first = FakeMeetingAnalyzer::new("model.a", "summary-v1").analyze(input());
    let second = FakeMeetingAnalyzer::new("model-a", "summary-v1").analyze(input());

    let AnalysisOutcome::Completed(first) = first else {
        panic!("first analysis should complete");
    };
    let AnalysisOutcome::Completed(second) = second else {
        panic!("second analysis should complete");
    };

    assert_ne!(first.id, second.id);
}

#[test]
fn malformed_model_output_returns_visible_failure_state() {
    let client = StaticClient::success("{not json");
    let outcome = OllamaAnalyzer::new(client, "llama3.2", "summary-v1").analyze(input());

    let AnalysisOutcome::Failed(failure) = outcome else {
        panic!("malformed output should fail visibly");
    };
    assert_eq!(failure.code, "malformed_model_output");
    assert!(failure.message.contains("structured JSON"));
}

#[test]
fn missing_required_model_fields_return_visible_failure_state() {
    let client = StaticClient::success(r#"{"summary":"too small"}"#);
    let outcome = OllamaAnalyzer::new(client, "llama3.2", "summary-v1").analyze(input());

    let AnalysisOutcome::Failed(failure) = outcome else {
        panic!("missing fields should fail visibly");
    };
    assert_eq!(failure.code, "malformed_model_output");
    assert!(failure.message.contains("decisions"));
}

#[test]
fn hosted_provider_is_gated_before_any_client_call_without_key_selection() {
    let client = CountingClient::success(valid_model_json());
    let outcome = OpenAiCompatibleAnalyzer::new(
        client,
        "gpt-4.1-mini",
        "summary-v1",
        HostedAnalysisConfig {
            selected_key_name: None,
            data_disclosure_confirmed: true,
        },
    )
    .analyze(input());

    let AnalysisOutcome::Failed(failure) = outcome else {
        panic!("hosted provider should be gated");
    };
    assert_eq!(failure.code, "hosted_provider_gated");
    assert_eq!(failure.client_calls, 0);
}

#[test]
fn hosted_provider_is_gated_before_any_client_call_without_disclosure_confirmation() {
    let client = CountingClient::success(valid_model_json());
    let outcome = OpenAiCompatibleAnalyzer::new(
        client,
        "gpt-4.1-mini",
        "summary-v1",
        HostedAnalysisConfig {
            selected_key_name: Some("work-key".to_string()),
            data_disclosure_confirmed: false,
        },
    )
    .analyze(input());

    let AnalysisOutcome::Failed(failure) = outcome else {
        panic!("hosted provider should be gated");
    };
    assert_eq!(failure.code, "hosted_provider_gated");
    assert_eq!(failure.client_calls, 0);
}

#[test]
fn ollama_unavailable_returns_setup_guidance_instead_of_crashing() {
    let client = StaticClient::unavailable("connection refused");
    let outcome = OllamaAnalyzer::new(client, "llama3.2", "summary-v1").analyze(input());

    let AnalysisOutcome::Failed(failure) = outcome else {
        panic!("unavailable Ollama should be a visible failure");
    };
    assert_eq!(failure.code, "ollama_unavailable");
    assert!(failure.setup_guidance.contains("ollama serve"));
}

#[test]
fn ollama_provider_path_generates_cited_summary_without_real_server() {
    let client = StaticClient::success(valid_model_json());
    let outcome = OllamaAnalyzer::new(client, "llama3.2", "summary-v1").analyze(input());

    let AnalysisOutcome::Completed(analysis) = outcome else {
        panic!("valid Ollama output should complete");
    };
    assert_eq!(analysis.provider, "ollama");
    assert_eq!(analysis.model_name, "llama3.2");
    assert!(!analysis.network_used);
    assert_eq!(analysis.decisions[0].citations[0].segment_id, "segment-1");
}

#[test]
fn summary_json_schema_requires_structured_decisions_instead_of_string_lists() {
    let schema = curiosity_analysis::summary_json_schema();
    let decision_item = &schema["properties"]["decisions"]["items"];

    assert_eq!(decision_item["type"], "object");
    assert_eq!(decision_item["properties"]["text"]["type"], "string");
    assert_eq!(decision_item["properties"]["citations"]["type"], "array");
}

#[test]
fn openai_compatible_provider_runs_only_after_key_and_disclosure_opt_in() {
    let client = CountingClient::success(valid_model_json());
    let outcome = OpenAiCompatibleAnalyzer::new(
        client,
        "gpt-4.1-mini",
        "summary-v1",
        HostedAnalysisConfig {
            selected_key_name: Some("work-key".to_string()),
            data_disclosure_confirmed: true,
        },
    )
    .analyze(input());

    let AnalysisOutcome::Completed(analysis) = outcome else {
        panic!("hosted provider should run after explicit opt in");
    };
    assert_eq!(analysis.provider, "openai-compatible");
    assert_eq!(analysis.model_name, "gpt-4.1-mini");
    assert!(analysis.network_used);
}

#[test]
fn recommended_presets_include_qwen_and_gemma_as_local_ollama_candidates() {
    let presets = recommended_analysis_model_presets();
    let qwen = preset(presets, "ollama-qwen3-6-27b");
    let gemma = preset(presets, "ollama-gemma4-31b");

    assert_eq!(qwen.display_name, "Qwen 3.6 27B");
    assert_eq!(qwen.provider_kind, AnalysisProviderKind::OllamaLocal);
    assert_eq!(qwen.model_tag, "qwen3.6:27b");
    assert!(qwen.default_candidate);
    assert!(!qwen.network_used);
    assert!(!qwen.requires_data_disclosure);
    assert!(qwen.setup_notes.contains("ollama pull qwen3.6:27b"));

    assert_eq!(gemma.display_name, "Gemma 4 31B");
    assert_eq!(gemma.provider_kind, AnalysisProviderKind::OllamaLocal);
    assert_eq!(gemma.model_tag, "gemma4:31b");
    assert!(gemma.default_candidate);
    assert!(!gemma.network_used);
    assert!(!gemma.requires_data_disclosure);
    assert!(gemma.setup_notes.contains("ollama pull gemma4:31b"));
}

#[test]
fn deepseek_presets_are_not_marked_as_local_ollama_models() {
    let presets = recommended_analysis_model_presets();
    let cloud = preset(presets, "ollama-cloud-deepseek-v3-2");
    let speciale = preset(presets, "hosted-deepseek-v3-2-speciale");

    assert_eq!(cloud.display_name, "DeepSeek V3.2 Cloud");
    assert_eq!(cloud.provider_kind, AnalysisProviderKind::OllamaCloud);
    assert_eq!(cloud.model_tag, "deepseek-v3.2:cloud");
    assert!(cloud.network_used);
    assert!(cloud.requires_data_disclosure);
    assert!(!cloud.default_candidate);

    assert_eq!(speciale.display_name, "DeepSeek V3.2 Speciale");
    assert_eq!(
        speciale.provider_kind,
        AnalysisProviderKind::OpenAiCompatibleHosted
    );
    assert_eq!(speciale.model_tag, "DeepSeek-V3.2-Speciale");
    assert!(speciale.network_used);
    assert!(speciale.requires_data_disclosure);
    assert!(!speciale.default_candidate);
}

#[test]
fn ollama_cloud_preset_is_gated_before_any_client_call_without_disclosure() {
    let client = CountingClient::success(valid_model_json());
    let outcome = OllamaAnalyzer::new_cloud(
        client,
        "deepseek-v3.2:cloud",
        "summary-v1",
        HostedAnalysisConfig {
            selected_key_name: Some("ollama-cloud".to_string()),
            data_disclosure_confirmed: false,
        },
    )
    .analyze(input());

    let AnalysisOutcome::Failed(failure) = outcome else {
        panic!("Ollama cloud should require disclosure");
    };
    assert_eq!(failure.code, "hosted_provider_gated");
    assert_eq!(failure.client_calls, 0);
}

#[test]
fn deepseek_speciale_hosted_preset_uses_existing_hosted_gating_path() {
    let client = CountingClient::success(valid_model_json());
    let speciale = preset(
        recommended_analysis_model_presets(),
        "hosted-deepseek-v3-2-speciale",
    );

    let outcome = OpenAiCompatibleAnalyzer::new(
        client,
        speciale.model_tag,
        "summary-v1",
        HostedAnalysisConfig {
            selected_key_name: None,
            data_disclosure_confirmed: true,
        },
    )
    .analyze(input());

    let AnalysisOutcome::Failed(failure) = outcome else {
        panic!("Speciale preset should require hosted key selection");
    };
    assert_eq!(failure.code, "hosted_provider_gated");
    assert_eq!(failure.client_calls, 0);
}

#[test]
fn cloud_ollama_tag_cannot_be_run_through_local_ollama_constructor() {
    let client = CountingClient::success(valid_model_json());
    let outcome = OllamaAnalyzer::new(client, "deepseek-v3.2:cloud", "summary-v1").analyze(input());

    let AnalysisOutcome::Failed(failure) = outcome else {
        panic!("cloud Ollama tag should not run through local constructor");
    };
    assert_eq!(failure.code, "hosted_provider_gated");
    assert_eq!(failure.client_calls, 0);
}

#[test]
fn deepseek_speciale_tag_cannot_be_run_through_local_ollama_constructor() {
    let client = CountingClient::success(valid_model_json());
    let outcome =
        OllamaAnalyzer::new(client, "DeepSeek-V3.2-Speciale", "summary-v1").analyze(input());

    let AnalysisOutcome::Failed(failure) = outcome else {
        panic!("hosted Speciale tag should not run through local constructor");
    };
    assert_eq!(failure.code, "hosted_provider_gated");
    assert_eq!(failure.client_calls, 0);
}

#[test]
fn hosted_preset_id_cannot_be_run_through_local_ollama_constructor() {
    let client = CountingClient::success(valid_model_json());
    let outcome =
        OllamaAnalyzer::new(client, "hosted-deepseek-v3-2-speciale", "summary-v1").analyze(input());

    let AnalysisOutcome::Failed(failure) = outcome else {
        panic!("hosted preset id should not run through local constructor");
    };
    assert_eq!(failure.code, "hosted_provider_gated");
    assert_eq!(failure.client_calls, 0);
}

#[test]
fn ollama_provider_rejects_empty_transcript_before_client_call() {
    let client = CountingClient::success(valid_model_json());
    let outcome = OllamaAnalyzer::new(client, "qwen3.6:27b", "summary-v1").analyze(empty_input());

    let AnalysisOutcome::Failed(failure) = outcome else {
        panic!("empty transcript should fail before provider call");
    };
    assert_eq!(failure.code, "no_transcript_segments");
    assert_eq!(failure.client_calls, 0);
}

#[test]
fn hosted_provider_rejects_empty_transcript_before_client_call_even_when_gated_config_is_present() {
    let client = CountingClient::success(valid_model_json());
    let outcome = OpenAiCompatibleAnalyzer::new(
        client,
        "gpt-4.1-mini",
        "summary-v1",
        HostedAnalysisConfig {
            selected_key_name: None,
            data_disclosure_confirmed: false,
        },
    )
    .analyze(empty_input());

    let AnalysisOutcome::Failed(failure) = outcome else {
        panic!("empty transcript should fail before hosted gate or provider call");
    };
    assert_eq!(failure.code, "no_transcript_segments");
    assert_eq!(failure.client_calls, 0);
}

fn valid_model_json() -> &'static str {
    r#"{
        "summary": "The team agreed to ship local recording and keep hosted analysis gated.",
        "decisions": [
            {
                "text": "Ship the local recorder",
                "citations": [{"segment_id":"segment-1","start_ms":0,"end_ms":1000}]
            }
        ],
        "action_items": [
            {
                "text": "Update the summary tests",
                "owner": "Alex",
                "due_date": "Friday",
                "citations": [{"segment_id":"segment-2","start_ms":1000,"end_ms":2000}]
            }
        ],
        "questions": [
            {
                "text": "Should hosted analysis stay disabled by default?",
                "citations": [{"segment_id":"segment-3","start_ms":2000,"end_ms":3000}]
            }
        ],
        "citations": [
            {"segment_id":"segment-1","start_ms":0,"end_ms":1000},
            {"segment_id":"segment-2","start_ms":1000,"end_ms":2000}
        ]
    }"#
}

fn empty_input() -> AnalysisInput {
    AnalysisInput {
        meeting_id: "meeting-1".to_string(),
        created_at_ms: 10_000,
        transcript_segments: Vec::new(),
    }
}

fn preset<'a>(
    presets: &'a [curiosity_analysis::AnalysisModelPreset],
    id: &str,
) -> &'a curiosity_analysis::AnalysisModelPreset {
    presets
        .iter()
        .find(|preset| preset.id == id)
        .unwrap_or_else(|| panic!("missing preset {id}"))
}

#[derive(Clone)]
struct StaticClient {
    result: Result<String, AnalysisClientError>,
}

impl StaticClient {
    fn success(text: impl Into<String>) -> Self {
        Self {
            result: Ok(text.into()),
        }
    }

    fn unavailable(message: impl Into<String>) -> Self {
        Self {
            result: Err(AnalysisClientError::Unavailable(message.into())),
        }
    }
}

impl ProviderTextClient for StaticClient {
    fn complete(&self, _model_name: &str, _prompt: &str) -> Result<String, AnalysisClientError> {
        self.result.clone()
    }

    fn call_count(&self) -> u32 {
        0
    }
}

struct CountingClient {
    response: String,
    calls: Cell<u32>,
}

impl CountingClient {
    fn success(text: impl Into<String>) -> Self {
        Self {
            response: text.into(),
            calls: Cell::new(0),
        }
    }
}

impl ProviderTextClient for CountingClient {
    fn complete(&self, _model_name: &str, _prompt: &str) -> Result<String, AnalysisClientError> {
        self.calls.set(self.calls.get() + 1);
        Ok(self.response.clone())
    }

    fn call_count(&self) -> u32 {
        self.calls.get()
    }
}
