use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::provider::{CompleteOpts, LlmProvider};

pub struct OllamaProvider {
    endpoint: String,
    client: reqwest::Client,
}

impl OllamaProvider {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Serialize)]
struct GenerateRequest {
    model: String,
    prompt: String,
    stream: bool,
    options: GenerateOptions,
}

#[derive(Serialize)]
struct GenerateOptions {
    temperature: f32,
}

#[derive(Deserialize)]
struct GenerateResponse {
    response: String,
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    async fn complete(&self, prompt: &str, opts: CompleteOpts) -> jarvis_core::Result<String> {
        let url = format!("{}/api/generate", self.endpoint.trim_end_matches('/'));
        let body = GenerateRequest {
            model: opts.model,
            prompt: prompt.to_string(),
            stream: false,
            options: GenerateOptions {
                temperature: opts.temperature,
            },
        };
        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| jarvis_core::error::JarvisError::Other(format!("ollama: {e}")))?;
        if !response.status().is_success() {
            return Err(jarvis_core::error::JarvisError::Other(format!(
                "ollama returned {}",
                response.status()
            )));
        }
        let parsed: GenerateResponse = response
            .json()
            .await
            .map_err(|e| jarvis_core::error::JarvisError::Other(format!("ollama parse: {e}")))?;
        Ok(parsed.response)
    }

    async fn match_playbook(&self, phrase: &str, playbooks: &[String]) -> jarvis_core::Result<Option<String>> {
        if playbooks.is_empty() {
            return Ok(None);
        }
        let prompt = format!(
            "Given user phrase: \"{phrase}\"\nAvailable playbooks: {playbooks:?}\n\
             Reply with ONLY the exact playbook id that best matches, or NONE."
        );
        let result = self
            .complete(&prompt, CompleteOpts::default())
            .await?;
        let trimmed = result.trim();
        if trimmed.eq_ignore_ascii_case("NONE") {
            return Ok(None);
        }
        Ok(playbooks
            .iter()
            .find(|p| p.eq_ignore_ascii_case(trimmed))
            .cloned())
    }

    async fn draft_playbook_yaml(&self, commands: &[String], name_hint: &str) -> jarvis_core::Result<String> {
        let prompt = format!(
            "Generate a PSEUDO-JARVIS playbook YAML (apiVersion: jarvis.dev/v1, kind: Playbook) \
             named '{name_hint}' with shell steps for these commands:\n{commands:?}\n\
             Output ONLY valid YAML."
        );
        self.complete(&prompt, CompleteOpts {
            model: "llama3.2:3b".into(),
            temperature: 0.1,
        })
        .await
    }
}

pub fn create_provider(enabled: bool, endpoint: &str) -> Box<dyn LlmProvider> {
    if enabled {
        Box::new(OllamaProvider::new(endpoint))
    } else {
        Box::new(crate::provider::NoopProvider)
    }
}
