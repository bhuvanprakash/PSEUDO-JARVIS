use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct CompleteOpts {
    pub model: String,
    pub temperature: f32,
}

impl Default for CompleteOpts {
    fn default() -> Self {
        Self {
            model: "llama3.2:3b".into(),
            temperature: 0.2,
        }
    }
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, prompt: &str, opts: CompleteOpts) -> jarvis_core::Result<String>;
    async fn match_playbook(&self, phrase: &str, playbooks: &[String]) -> jarvis_core::Result<Option<String>>;
    async fn draft_playbook_yaml(&self, commands: &[String], name_hint: &str) -> jarvis_core::Result<String>;
}

pub struct NoopProvider;

#[async_trait]
impl LlmProvider for NoopProvider {
    async fn complete(&self, _prompt: &str, _opts: CompleteOpts) -> jarvis_core::Result<String> {
        Err(jarvis_core::error::JarvisError::Other("LLM disabled".into()))
    }

    async fn match_playbook(&self, phrase: &str, playbooks: &[String]) -> jarvis_core::Result<Option<String>> {
        let normalized = phrase.to_lowercase();
        Ok(playbooks
            .iter()
            .find(|p| normalized.contains(&p.to_lowercase()))
            .cloned())
    }

    async fn draft_playbook_yaml(&self, _commands: &[String], _name_hint: &str) -> jarvis_core::Result<String> {
        Err(jarvis_core::error::JarvisError::Other("LLM disabled".into()))
    }
}
