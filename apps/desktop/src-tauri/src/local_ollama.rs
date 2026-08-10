use std::net::IpAddr;
use std::time::Duration;

use curiosity_analysis::{
    recommended_analysis_model_presets, AnalysisClientError, AnalysisProviderKind,
};
use serde::Serialize;
use url::Url;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct OllamaConnectionTestView {
    pub(super) state: String,
    pub(super) message: String,
    pub(super) setup_guidance: String,
    pub(super) selected_local_model_tag: Option<String>,
    pub(super) installed_local_models: Option<Vec<String>>,
    pub(super) pull_command: Option<String>,
}

impl OllamaConnectionTestView {
    fn unavailable(message: impl Into<String>, setup_guidance: impl Into<String>) -> Self {
        Self {
            state: "Unavailable".to_string(),
            message: message.into(),
            setup_guidance: setup_guidance.into(),
            selected_local_model_tag: None,
            installed_local_models: None,
            pull_command: None,
        }
    }

    fn with_selected_local_model_tag(mut self, model_tag: String) -> Self {
        self.selected_local_model_tag = Some(model_tag);
        self
    }
}

pub(super) trait OllamaHttpTransport {
    fn post_json(
        &self,
        url: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, OllamaHttpError>;
    fn get_json(&self, url: &str) -> Result<serde_json::Value, OllamaHttpError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum OllamaHttpError {
    Unavailable(String),
    Http { status: u16, body: String },
    MalformedResponse(String),
}

impl std::fmt::Display for OllamaHttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(message) | Self::MalformedResponse(message) => write!(f, "{message}"),
            Self::Http { status, body } => write!(f, "Ollama returned HTTP {status}: {body}"),
        }
    }
}

impl From<OllamaHttpError> for AnalysisClientError {
    fn from(error: OllamaHttpError) -> Self {
        match error {
            OllamaHttpError::Unavailable(message) => Self::Unavailable(message),
            OllamaHttpError::Http { .. } | OllamaHttpError::MalformedResponse(_) => {
                Self::Transport(error.to_string())
            }
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct UreqOllamaHttpTransport;

impl OllamaHttpTransport for UreqOllamaHttpTransport {
    fn post_json(
        &self,
        url: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, OllamaHttpError> {
        ollama_ureq_agent()
            .post(url)
            .send_json(body)
            .map_err(ollama_http_error_from_ureq)?
            .into_json()
            .map_err(|error| {
                OllamaHttpError::MalformedResponse(format!("parse Ollama response JSON: {error}"))
            })
    }

    fn get_json(&self, url: &str) -> Result<serde_json::Value, OllamaHttpError> {
        ollama_ureq_agent()
            .get(url)
            .call()
            .map_err(ollama_http_error_from_ureq)?
            .into_json()
            .map_err(|error| {
                OllamaHttpError::MalformedResponse(format!("parse Ollama response JSON: {error}"))
            })
    }
}

fn ollama_ureq_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout_write(Duration::from_secs(30))
        .timeout_read(Duration::from_secs(120))
        .build()
}

fn ollama_http_error_from_ureq(error: ureq::Error) -> OllamaHttpError {
    match error {
        ureq::Error::Status(code, response) => {
            let status_text = response.status_text().to_string();
            let body = response.into_string().unwrap_or_else(|error| {
                format!("{status_text}; read response body failed: {error}")
            });
            let body = body.trim();
            OllamaHttpError::Http {
                status: code,
                body: if body.is_empty() {
                    status_text
                } else {
                    body.to_string()
                },
            }
        }
        ureq::Error::Transport(error) => OllamaHttpError::Unavailable(error.to_string()),
    }
}

pub(super) fn test_ollama_connection_value<T>(
    base_url: &str,
    model_name: &str,
    transport: &T,
) -> OllamaConnectionTestView
where
    T: OllamaHttpTransport,
{
    if let Err(error) = validate_local_ollama_model(model_name) {
        return OllamaConnectionTestView::unavailable(
            error.to_string(),
            "Choose a local Ollama model tag such as qwen3.6:27b or gemma4:31b.",
        );
    }
    let selected_model_tag = canonical_local_ollama_model_tag(model_name);
    let url = match local_ollama_endpoint(base_url, "/api/tags") {
        Ok(url) => url,
        Err(error) => {
            return OllamaConnectionTestView::unavailable(
                error.to_string(),
                "Use a local Ollama base URL such as http://127.0.0.1:11434.",
            )
            .with_selected_local_model_tag(selected_model_tag);
        }
    };
    let response = match transport.get_json(&url) {
        Ok(response) => response,
        Err(error) => {
            return OllamaConnectionTestView::unavailable(
                format!("Ollama is unavailable: {error}"),
                "Start Ollama with `ollama serve`, then retry.",
            )
            .with_selected_local_model_tag(selected_model_tag);
        }
    };
    let installed_models = installed_ollama_model_names(&response);
    let matched_model = installed_models
        .iter()
        .find(|installed_model| ollama_model_matches_request(installed_model, model_name))
        .cloned();
    if let Some(installed_model) = matched_model {
        OllamaConnectionTestView {
            state: "Available".to_string(),
            message: format!("Ollama is reachable and {installed_model} is installed."),
            setup_guidance: String::new(),
            selected_local_model_tag: Some(selected_model_tag),
            installed_local_models: Some(installed_models),
            pull_command: None,
        }
    } else {
        let pull_command = format!("ollama pull {selected_model_tag}");
        let installed_hint = if installed_models.is_empty() {
            " No local models were reported by Ollama.".to_string()
        } else {
            format!(" Installed local models: {}.", installed_models.join(", "))
        };
        let mut view = OllamaConnectionTestView::unavailable(
            format!("Ollama is reachable, but {selected_model_tag} is not installed."),
            format!(
                "Install the selected model with `{pull_command}`, then retry.{installed_hint}"
            ),
        )
        .with_selected_local_model_tag(selected_model_tag);
        view.installed_local_models = Some(installed_models);
        view.pull_command = Some(pull_command);
        view
    }
}

fn installed_ollama_model_names(response: &serde_json::Value) -> Vec<String> {
    let mut names = response
        .get("models")
        .and_then(|models| models.as_array())
        .into_iter()
        .flatten()
        .flat_map(|model| {
            ["name", "model"].into_iter().filter_map(|field| {
                model
                    .get(field)
                    .and_then(|name| name.as_str())
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(str::to_string)
            })
        })
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}

fn ollama_model_matches_request(installed_model: &str, requested_model: &str) -> bool {
    let installed_model = normalized_ollama_model_name(installed_model);
    requested_ollama_model_aliases(requested_model)
        .iter()
        .any(|alias| alias == &installed_model)
}

fn requested_ollama_model_aliases(requested_model: &str) -> Vec<String> {
    let trimmed = requested_model.trim();
    let mut aliases = Vec::new();
    push_unique_alias(&mut aliases, normalized_ollama_model_name(trimmed));
    if !trimmed.contains(':') {
        push_unique_alias(
            &mut aliases,
            normalized_ollama_model_name(&format!("{trimmed}:latest")),
        );
    }
    push_unique_alias(
        &mut aliases,
        normalized_ollama_model_name(&canonical_local_ollama_model_tag(trimmed)),
    );
    aliases
}

fn push_unique_alias(aliases: &mut Vec<String>, alias: String) {
    if !alias.is_empty() && !aliases.contains(&alias) {
        aliases.push(alias);
    }
}

pub(super) fn canonical_local_ollama_model_tag(model_name: &str) -> String {
    let trimmed = model_name.trim();
    let normalized = normalized_ollama_model_name(trimmed);
    recommended_analysis_model_presets()
        .iter()
        .find(|preset| {
            preset.provider_kind == AnalysisProviderKind::OllamaLocal
                && (normalized_ollama_model_name(preset.model_tag) == normalized
                    || normalized_ollama_model_name(preset.id) == normalized
                    || normalized_ollama_model_name(preset.display_name) == normalized)
        })
        .map(|preset| preset.model_tag.to_string())
        .unwrap_or_else(|| trimmed.to_ascii_lowercase())
}

fn normalized_ollama_model_name(model_name: &str) -> String {
    model_name
        .trim()
        .chars()
        .filter(|ch| !ch.is_ascii_whitespace())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

pub(super) fn validate_local_ollama_model(model_name: &str) -> Result<(), AnalysisClientError> {
    let trimmed = model_name.trim();
    if trimmed.is_empty() {
        return Err(AnalysisClientError::Transport(
            "Choose a local Ollama model before requesting analysis.".to_string(),
        ));
    }
    let normalized = normalized_ollama_model_name(trimmed);
    let is_hosted = normalized.ends_with(":cloud")
        || recommended_analysis_model_presets().iter().any(|preset| {
            preset.provider_kind != AnalysisProviderKind::OllamaLocal
                && (normalized_ollama_model_name(preset.model_tag) == normalized
                    || normalized_ollama_model_name(preset.id) == normalized
                    || normalized_ollama_model_name(preset.display_name) == normalized)
        });
    if is_hosted {
        return Err(AnalysisClientError::Transport(
            "hosted or cloud model tags cannot use the local Ollama privacy path.".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn local_ollama_endpoint(
    base_url: &str,
    path: &str,
) -> Result<String, AnalysisClientError> {
    let base_url = base_url.trim();
    let mut url = Url::parse(base_url).map_err(|error| {
        AnalysisClientError::Transport(format!("Ollama base URL is invalid: {error}"))
    })?;
    let local_url_error =
        "Ollama base URL must be a local loopback http(s) URL without credentials, query, or fragment.";
    if has_explicit_url_userinfo(base_url)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(AnalysisClientError::Transport(local_url_error.to_string()));
    }
    let is_loopback = match url.host_str() {
        Some("localhost") => true,
        Some(host) => host
            .parse::<IpAddr>()
            .map(|address| address.is_loopback())
            .unwrap_or(false),
        None => false,
    };
    if !is_loopback {
        return Err(AnalysisClientError::Transport(local_url_error.to_string()));
    }
    match url.scheme() {
        "http" | "https" => {}
        _ => return Err(AnalysisClientError::Transport(local_url_error.to_string())),
    }
    url.set_path(path);
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.to_string())
}

fn has_explicit_url_userinfo(url: &str) -> bool {
    let Some(authority_start) = url.find("://").map(|index| index + 3) else {
        return false;
    };
    let authority_end = url[authority_start..]
        .find(['/', '?', '#'])
        .map(|index| authority_start + index)
        .unwrap_or(url.len());
    url[authority_start..authority_end].contains('@')
}
