use std::collections::HashSet;

use curiosity_domain::{
    AnalysisActionItem, AnalysisCitation, AnalysisDecision, AnalysisQuestion, MeetingAnalysis,
    TranscriptSegment,
};
use serde::Deserialize;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnalysisInput {
    pub meeting_id: String,
    pub created_at_ms: u64,
    pub transcript_segments: Vec<TranscriptSegment>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AnalysisOutcome {
    Completed(MeetingAnalysis),
    Failed(AnalysisFailure),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnalysisFailure {
    pub code: String,
    pub message: String,
    pub setup_guidance: String,
    pub client_calls: u32,
}

impl AnalysisFailure {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            setup_guidance: String::new(),
            client_calls: 0,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AnalysisClientError {
    Unavailable(String),
    Transport(String),
}

pub trait ProviderTextClient {
    fn complete(&self, model_name: &str, prompt: &str) -> Result<String, AnalysisClientError>;

    fn call_count(&self) -> u32 {
        0
    }
}

pub trait MeetingAnalyzer {
    fn analyze(&self, input: AnalysisInput) -> AnalysisOutcome;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnalysisProviderKind {
    OllamaLocal,
    OllamaCloud,
    OpenAiCompatibleHosted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnalysisModelPreset {
    pub id: &'static str,
    pub display_name: &'static str,
    pub provider_kind: AnalysisProviderKind,
    pub model_tag: &'static str,
    pub network_used: bool,
    pub requires_data_disclosure: bool,
    pub default_candidate: bool,
    pub setup_notes: &'static str,
}

const RECOMMENDED_ANALYSIS_MODEL_PRESETS: &[AnalysisModelPreset] = &[
    AnalysisModelPreset {
        id: "ollama-qwen3-6-27b",
        display_name: "Qwen 3.6 27B",
        provider_kind: AnalysisProviderKind::OllamaLocal,
        model_tag: "qwen3.6:27b",
        network_used: false,
        requires_data_disclosure: false,
        default_candidate: true,
        setup_notes: "Install Ollama locally, then run `ollama pull qwen3.6:27b`.",
    },
    AnalysisModelPreset {
        id: "ollama-gemma4-31b",
        display_name: "Gemma 4 31B",
        provider_kind: AnalysisProviderKind::OllamaLocal,
        model_tag: "gemma4:31b",
        network_used: false,
        requires_data_disclosure: false,
        default_candidate: true,
        setup_notes: "Install Ollama locally, then run `ollama pull gemma4:31b`.",
    },
    AnalysisModelPreset {
        id: "ollama-cloud-deepseek-v3-2",
        display_name: "DeepSeek V3.2 Cloud",
        provider_kind: AnalysisProviderKind::OllamaCloud,
        model_tag: "deepseek-v3.2:cloud",
        network_used: true,
        requires_data_disclosure: true,
        default_candidate: false,
        setup_notes: "Uses Ollama cloud. Select a cloud key/account and confirm transcript data disclosure before use.",
    },
    AnalysisModelPreset {
        id: "hosted-deepseek-v3-2-speciale",
        display_name: "DeepSeek V3.2 Speciale",
        provider_kind: AnalysisProviderKind::OpenAiCompatibleHosted,
        model_tag: "DeepSeek-V3.2-Speciale",
        network_used: true,
        requires_data_disclosure: true,
        default_candidate: false,
        setup_notes: "Use only through a hosted OpenAI-compatible or external API path with explicit key selection and data-disclosure confirmation.",
    },
];

pub fn recommended_analysis_model_presets() -> &'static [AnalysisModelPreset] {
    RECOMMENDED_ANALYSIS_MODEL_PRESETS
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostedAnalysisConfig {
    pub selected_key_name: Option<String>,
    pub data_disclosure_confirmed: bool,
}

pub struct FakeMeetingAnalyzer {
    model_name: String,
    prompt_template_version: String,
}

impl FakeMeetingAnalyzer {
    pub fn new(model_name: impl Into<String>, prompt_template_version: impl Into<String>) -> Self {
        Self {
            model_name: model_name.into(),
            prompt_template_version: prompt_template_version.into(),
        }
    }

    pub fn analyze(&self, input: AnalysisInput) -> AnalysisOutcome {
        MeetingAnalyzer::analyze(self, input)
    }
}

impl MeetingAnalyzer for FakeMeetingAnalyzer {
    fn analyze(&self, input: AnalysisInput) -> AnalysisOutcome {
        if input.transcript_segments.is_empty() {
            return no_transcript_failure(0);
        }
        let citations = input
            .transcript_segments
            .iter()
            .map(segment_citation)
            .collect::<Vec<_>>();
        let summary = format!(
            "Summary of {}: {}",
            input.meeting_id, input.transcript_segments[0].text
        );
        let decisions = input
            .transcript_segments
            .iter()
            .filter_map(fake_decision)
            .collect::<Vec<_>>();
        let action_items = input
            .transcript_segments
            .iter()
            .filter_map(fake_action_item)
            .collect::<Vec<_>>();
        let questions = input
            .transcript_segments
            .iter()
            .filter_map(fake_question)
            .collect::<Vec<_>>();
        AnalysisOutcome::Completed(MeetingAnalysis {
            id: analysis_id(
                &input.meeting_id,
                "fake-local",
                &self.model_name,
                &self.prompt_template_version,
            ),
            meeting_id: input.meeting_id,
            provider: "fake-local".to_string(),
            model_name: self.model_name.clone(),
            network_used: false,
            created_at_ms: input.created_at_ms,
            prompt_template_version: self.prompt_template_version.clone(),
            summary,
            decisions,
            action_items,
            questions,
            citations,
        })
    }
}

pub struct OllamaAnalyzer<C> {
    client: C,
    model_name: String,
    prompt_template_version: String,
    provider: &'static str,
    network_used: bool,
    hosted_config: Option<HostedAnalysisConfig>,
}

impl<C> OllamaAnalyzer<C>
where
    C: ProviderTextClient,
{
    pub fn new(client: C, model_name: impl Into<String>, prompt_template_version: impl Into<String>) -> Self {
        Self {
            client,
            model_name: model_name.into(),
            prompt_template_version: prompt_template_version.into(),
            provider: "ollama",
            network_used: false,
            hosted_config: None,
        }
    }

    pub fn new_cloud(
        client: C,
        model_name: impl Into<String>,
        prompt_template_version: impl Into<String>,
        config: HostedAnalysisConfig,
    ) -> Self {
        Self {
            client,
            model_name: model_name.into(),
            prompt_template_version: prompt_template_version.into(),
            provider: "ollama-cloud",
            network_used: true,
            hosted_config: Some(config),
        }
    }

    pub fn analyze(&self, input: AnalysisInput) -> AnalysisOutcome {
        MeetingAnalyzer::analyze(self, input)
    }
}

impl<C> MeetingAnalyzer for OllamaAnalyzer<C>
where
    C: ProviderTextClient,
{
    fn analyze(&self, input: AnalysisInput) -> AnalysisOutcome {
        if input.transcript_segments.is_empty() {
            return no_transcript_failure(self.client.call_count());
        }
        if local_ollama_model_is_hosted_or_cloud(&self.model_name) && self.hosted_config.is_none() {
            return hosted_gate_failure(self.client.call_count());
        }
        if let Some(config) = &self.hosted_config {
            if hosted_config_is_gated(config) {
                return hosted_gate_failure(self.client.call_count());
            }
        }
        match self.client.complete(&self.model_name, &prompt(&input)) {
            Ok(text) => parse_model_output(
                &text,
                input,
                self.provider,
                &self.model_name,
                self.network_used,
                &self.prompt_template_version,
            ),
            Err(AnalysisClientError::Unavailable(message)) => {
                let mut failure = AnalysisFailure::new(
                    "ollama_unavailable",
                    format!("Ollama is unavailable: {message}"),
                );
                failure.setup_guidance =
                    "Start Ollama with `ollama serve`, install the selected model, then retry."
                        .to_string();
                failure.client_calls = self.client.call_count();
                AnalysisOutcome::Failed(failure)
            }
            Err(AnalysisClientError::Transport(message)) => {
                let mut failure = AnalysisFailure::new("provider_transport_error", message);
                failure.client_calls = self.client.call_count();
                AnalysisOutcome::Failed(failure)
            }
        }
    }
}

pub struct OpenAiCompatibleAnalyzer<C> {
    client: C,
    model_name: String,
    prompt_template_version: String,
    config: HostedAnalysisConfig,
}

impl<C> OpenAiCompatibleAnalyzer<C>
where
    C: ProviderTextClient,
{
    pub fn new(
        client: C,
        model_name: impl Into<String>,
        prompt_template_version: impl Into<String>,
        config: HostedAnalysisConfig,
    ) -> Self {
        Self {
            client,
            model_name: model_name.into(),
            prompt_template_version: prompt_template_version.into(),
            config,
        }
    }

    pub fn analyze(&self, input: AnalysisInput) -> AnalysisOutcome {
        MeetingAnalyzer::analyze(self, input)
    }
}

impl<C> MeetingAnalyzer for OpenAiCompatibleAnalyzer<C>
where
    C: ProviderTextClient,
{
    fn analyze(&self, input: AnalysisInput) -> AnalysisOutcome {
        if input.transcript_segments.is_empty() {
            return no_transcript_failure(self.client.call_count());
        }
        if hosted_config_is_gated(&self.config) {
            return hosted_gate_failure(self.client.call_count());
        }
        match self.client.complete(&self.model_name, &prompt(&input)) {
            Ok(text) => parse_model_output(
                &text,
                input,
                "openai-compatible",
                &self.model_name,
                true,
                &self.prompt_template_version,
            ),
            Err(AnalysisClientError::Unavailable(message))
            | Err(AnalysisClientError::Transport(message)) => {
                let mut failure = AnalysisFailure::new("provider_transport_error", message);
                failure.client_calls = self.client.call_count();
                AnalysisOutcome::Failed(failure)
            }
        }
    }
}

fn local_ollama_model_is_hosted_or_cloud(model_name: &str) -> bool {
    model_name.ends_with(":cloud")
        || recommended_analysis_model_presets().iter().any(|preset| {
            preset.provider_kind != AnalysisProviderKind::OllamaLocal
                && (preset.model_tag == model_name || preset.id == model_name)
        })
}

fn no_transcript_failure(client_calls: u32) -> AnalysisOutcome {
    AnalysisOutcome::Failed(AnalysisFailure {
        code: "no_transcript_segments".to_string(),
        message: "Generate a transcript before requesting a summary.".to_string(),
        setup_guidance: String::new(),
        client_calls,
    })
}

fn hosted_config_is_gated(config: &HostedAnalysisConfig) -> bool {
    config.selected_key_name.as_deref().unwrap_or("").is_empty()
        || !config.data_disclosure_confirmed
}

fn hosted_gate_failure(client_calls: u32) -> AnalysisOutcome {
    AnalysisOutcome::Failed(AnalysisFailure {
        code: "hosted_provider_gated".to_string(),
        message: "Select an API key and confirm transcript data disclosure before hosted analysis."
            .to_string(),
        setup_guidance: String::new(),
        client_calls,
    })
}

fn parse_model_output(
    text: &str,
    input: AnalysisInput,
    provider: &str,
    model_name: &str,
    network_used: bool,
    prompt_template_version: &str,
) -> AnalysisOutcome {
    let payload = match serde_json::from_str::<ModelPayload>(text) {
        Ok(payload) => payload,
        Err(err) => {
            return AnalysisOutcome::Failed(AnalysisFailure::new(
                "malformed_model_output",
                format!("Model output was not structured JSON matching the summary schema: {err}"),
            ))
        }
    };
    let valid_segment_ids = input
        .transcript_segments
        .iter()
        .map(|segment| segment.id.as_str())
        .collect::<HashSet<_>>();
    for citation in payload.all_citations() {
        if !valid_segment_ids.contains(citation.segment_id.as_str()) {
            return AnalysisOutcome::Failed(AnalysisFailure::new(
                "malformed_model_output",
                format!(
                    "Model output cited unknown transcript segment {}",
                    citation.segment_id
                ),
            ));
        }
    }
    AnalysisOutcome::Completed(MeetingAnalysis {
        id: analysis_id(
            &input.meeting_id,
            provider,
            model_name,
            prompt_template_version,
        ),
        meeting_id: input.meeting_id,
        provider: provider.to_string(),
        model_name: model_name.to_string(),
        network_used,
        created_at_ms: input.created_at_ms,
        prompt_template_version: prompt_template_version.to_string(),
        summary: payload.summary,
        decisions: payload.decisions,
        action_items: payload.action_items,
        questions: payload.questions,
        citations: payload.citations,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelPayload {
    summary: String,
    decisions: Vec<AnalysisDecision>,
    action_items: Vec<AnalysisActionItem>,
    questions: Vec<AnalysisQuestion>,
    citations: Vec<AnalysisCitation>,
}

impl ModelPayload {
    fn all_citations(&self) -> Vec<&AnalysisCitation> {
        let mut citations = self.citations.iter().collect::<Vec<_>>();
        for decision in &self.decisions {
            citations.extend(decision.citations.iter());
        }
        for action_item in &self.action_items {
            citations.extend(action_item.citations.iter());
        }
        for question in &self.questions {
            citations.extend(question.citations.iter());
        }
        citations
    }
}

fn prompt(input: &AnalysisInput) -> String {
    let mut lines = vec![
        "Return strict JSON with summary, decisions, action_items, questions, and citations."
            .to_string(),
    ];
    for segment in &input.transcript_segments {
        lines.push(format!(
            "[{} {}-{}] {}",
            segment.id, segment.start_ms, segment.end_ms, segment.text
        ));
    }
    lines.join("\n")
}

fn fake_decision(segment: &TranscriptSegment) -> Option<AnalysisDecision> {
    let lower = segment.text.to_ascii_lowercase();
    let marker = "decided to ";
    let start = lower.find(marker)? + marker.len();
    Some(AnalysisDecision {
        text: trim_sentence(&segment.text[start..]),
        citations: vec![segment_citation(segment)],
    })
}

fn fake_action_item(segment: &TranscriptSegment) -> Option<AnalysisActionItem> {
    let will_index = segment.text.find(" will ")?;
    let owner = segment.text[..will_index].trim();
    let rest = &segment.text[will_index + " will ".len()..];
    let (text, due_date) = match rest.rsplit_once(" by ") {
        Some((task, due)) => (trim_sentence(task), Some(trim_sentence(due))),
        None => (trim_sentence(rest), None),
    };
    Some(AnalysisActionItem {
        text,
        owner: (!owner.is_empty()).then(|| owner.to_string()),
        due_date,
        citations: vec![segment_citation(segment)],
    })
}

fn fake_question(segment: &TranscriptSegment) -> Option<AnalysisQuestion> {
    if segment.text.contains('?') {
        Some(AnalysisQuestion {
            text: trim_sentence(&segment.text),
            citations: vec![segment_citation(segment)],
        })
    } else {
        None
    }
}

fn segment_citation(segment: &TranscriptSegment) -> AnalysisCitation {
    AnalysisCitation {
        segment_id: segment.id.clone(),
        start_ms: segment.start_ms,
        end_ms: segment.end_ms,
    }
}

fn trim_sentence(text: &str) -> String {
    text.trim()
        .trim_end_matches('.')
        .trim_end_matches('?')
        .trim()
        .to_string()
}

fn analysis_id(meeting_id: &str, provider: &str, model_name: &str, prompt_version: &str) -> String {
    format!(
        "analysis-{}-{}-{}-{}",
        safe_id_part(meeting_id),
        safe_id_part(provider),
        safe_id_part(model_name),
        safe_id_part(prompt_version)
    )
}

fn safe_id_part(part: &str) -> String {
    part.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}
