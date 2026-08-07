use jarvis_core::playbook::{load_playbook, Playbook, PlaybookDocument, PlaybookMetadata, PlaybookSpec, PlaybookStep, StepType};
use jarvis_core::types::PatternRecord;
use jarvis_core::Vault;
use jarvis_llm::LlmProvider;

use crate::pattern::PatternMiner;

pub struct PatternSuggester<'a> {
    vault: &'a Vault,
    llm: &'a dyn LlmProvider,
}

impl<'a> PatternSuggester<'a> {
    pub fn new(vault: &'a Vault, llm: &'a dyn LlmProvider) -> Self {
        Self { vault, llm }
    }

    pub async fn scan(&self, days: i64) -> jarvis_core::Result<Vec<PatternRecord>> {
        let patterns = PatternMiner::mine(self.vault, days)?;
        for pattern in &patterns {
            self.vault.upsert_pattern(pattern)?;
        }
        Ok(patterns)
    }

    pub async fn accept_pattern(&self, pattern_id: &str) -> jarvis_core::Result<Playbook> {
        let patterns = self.vault.list_patterns(None)?;
        let pattern = patterns
            .into_iter()
            .find(|p| p.id == pattern_id)
            .ok_or_else(|| jarvis_core::error::JarvisError::Other("pattern not found".into()))?;

        let name = pattern
            .suggested_name
            .clone()
            .unwrap_or_else(|| "suggested-workflow".into());

        let yaml = self
            .llm
            .draft_playbook_yaml(&pattern.command_sequence, &name)
            .await
            .unwrap_or_else(|_| fallback_yaml(&name, &pattern.command_sequence));

        let doc: PlaybookDocument = serde_yaml::from_str(&yaml)
            .unwrap_or_else(|_| fallback_doc(&name, &pattern.command_sequence));

        let mut playbook = Playbook {
            doc,
            path: self.vault.paths.playbooks().join(format!("{name}.yaml")),
            content_hash: String::new(),
            version: 1,
        };
        jarvis_core::playbook::validate_playbook(&playbook.doc)?;
        let path = self.vault.save_playbook_file(&playbook)?;
        playbook = load_playbook(&path)?;
        self.vault.update_pattern_status(pattern_id, "accepted")?;
        Ok(playbook)
    }

    pub fn dismiss_pattern(&self, pattern_id: &str) -> jarvis_core::Result<()> {
        self.vault.update_pattern_status(pattern_id, "dismissed")
    }
}

fn fallback_doc(name: &str, commands: &[String]) -> PlaybookDocument {
    PlaybookDocument {
        api_version: "jarvis.dev/v1".into(),
        kind: "Playbook".into(),
        metadata: PlaybookMetadata {
            name: name.into(),
            description: "Auto-suggested workflow".into(),
            tags: vec!["suggested".into()],
            author: "jarvis".into(),
        },
        spec: PlaybookSpec {
            triggers: Default::default(),
            defaults: Default::default(),
            schedule_trust: None,
            allow_paths: vec![".".into()],
            steps: commands
                .iter()
                .enumerate()
                .map(|(i, cmd)| PlaybookStep {
                    id: format!("step-{}", i + 1),
                    name: format!("Step {}", i + 1),
                    step_type: StepType::Shell,
                    trust: None,
                    depends_on: if i == 0 {
                        vec![]
                    } else {
                        vec![format!("step-{i}")]
                    },
                    cwd: None,
                    command: Some(cmd.clone()),
                    timeout: None,
                    config: serde_json::json!({}),
                    elevated: false,
                })
                .collect(),
        },
    }
}

fn fallback_yaml(name: &str, commands: &[String]) -> String {
    serde_yaml::to_string(&fallback_doc(name, commands)).unwrap_or_default()
}
