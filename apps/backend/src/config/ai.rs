//! The self-hosted language model behind the AI features. Without one, the AI features are off; with
//! one, they are on. It is set either in the environment or by an admin in the app (which wins).
//!
//! The model runs on your own hardware in a server that speaks the OpenAI chat protocol (for
//! example `llama.cpp` or Ollama). There is no cloud provider and no API key anywhere.

use std::env;

use super::ConfigError;

#[derive(Clone, Debug)]
pub struct AiConfig {
    /// Base URL of the local model server, e.g. `http://127.0.0.1:8081`.
    pub model_url: String,
    /// Name sent as `model`; the server may ignore it.
    pub model_name: String,
    /// Upper bound for the length of one answer, in tokens.
    pub max_output_tokens: u32,
}

fn optional(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

pub const DEFAULT_MODEL_NAME: &str = "work-assistant";
pub const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 1024;

impl AiConfig {
    /// Checks what an admin typed: a plain http(s) address (no login in it, no query), a model
    /// name and a sane answer length.
    pub fn parse(url: &str, name: &str, max_output_tokens: u32) -> Result<Self, &'static str> {
        let url = url.trim().trim_end_matches('/');
        let parsed = reqwest::Url::parse(url).map_err(|_| "the address is not a valid URL")?;
        if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
            return Err("the address must start with http:// or https://");
        }
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err("the address must not contain a user name or password");
        }
        if parsed.query().is_some() || parsed.fragment().is_some() {
            return Err("the address must not contain a query or a fragment");
        }
        if url.len() > 500 {
            return Err("the address is too long");
        }
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 100 || name.chars().any(char::is_control) {
            return Err("the model name must be 1 to 100 characters");
        }
        if !(64..=8_000).contains(&max_output_tokens) {
            return Err("the answer length must be between 64 and 8000 tokens");
        }
        Ok(Self {
            model_url: url.to_owned(),
            model_name: name.to_owned(),
            max_output_tokens,
        })
    }
}

pub(super) fn ai_from_env() -> Result<Option<AiConfig>, ConfigError> {
    let Some(url) = optional("WORK_AI_MODEL_URL") else {
        return Ok(None);
    };
    let max_output_tokens = match optional("WORK_AI_MAX_OUTPUT_TOKENS") {
        None => DEFAULT_MAX_OUTPUT_TOKENS,
        Some(v) => v.parse::<u32>().map_err(|_| {
            ConfigError::Invalid("WORK_AI_MAX_OUTPUT_TOKENS must be between 64 and 8000".into())
        })?,
    };
    let name = optional("WORK_AI_MODEL_NAME").unwrap_or_else(|| DEFAULT_MODEL_NAME.into());
    AiConfig::parse(&url, &name, max_output_tokens)
        .map(Some)
        .map_err(|e| ConfigError::Invalid(format!("WORK_AI_MODEL_*: {e}")))
}
