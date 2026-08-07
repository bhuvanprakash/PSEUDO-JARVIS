use std::collections::HashMap;

use regex::Regex;

use crate::error::Result;
use crate::playbook::{Playbook, PlaybookStep};
use crate::types::TrustLevel;
use crate::vault::Vault;

pub struct TrustResolver {
    deny_patterns: Vec<Regex>,
}

impl Default for TrustResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl TrustResolver {
    pub fn new() -> Self {
        let patterns = [
            r"(?i)rm\s+-rf\s+/",
            r"(?i):\(\)\s*\{\s*:\|\s*&\s*\}\s*;",
            r"(?i)mkfs\.",
            r"(?i)dd\s+if=/dev/zero",
        ];
        let deny_patterns = patterns
            .iter()
            .filter_map(|p| Regex::new(p).ok())
            .collect();
        Self { deny_patterns }
    }

    pub fn resolve(
        &self,
        vault: &Vault,
        playbook: &Playbook,
        step: &PlaybookStep,
        schedule_triggered: bool,
    ) -> Result<TrustLevel> {
        let grants = load_grants_map(vault, playbook.id())?;
        self.resolve_with_grants(&grants, playbook, step, schedule_triggered)
    }

    pub fn resolve_with_grants(
        &self,
        grants: &HashMap<String, TrustLevel>,
        playbook: &Playbook,
        step: &PlaybookStep,
        schedule_triggered: bool,
    ) -> Result<TrustLevel> {
        if let Some(cmd) = &step.command {
            for pattern in &self.deny_patterns {
                if pattern.is_match(cmd) {
                    return Ok(TrustLevel::Deny);
                }
            }
            if cmd.contains("sudo") && !step.elevated {
                return Ok(TrustLevel::Deny);
            }
        }

        if let Some(level) = step.trust {
            return Ok(level);
        }

        if let Some(level) = grants.get(&step.id) {
            return Ok(*level);
        }
        if let Some(level) = grants.get("*") {
            return Ok(*level);
        }

        if schedule_triggered {
            if let Some(level) = playbook.doc.spec.schedule_trust {
                return Ok(level);
            }
        }

        Ok(playbook.doc.spec.defaults.trust)
    }

    pub fn requires_confirmation(level: TrustLevel) -> bool {
        matches!(level, TrustLevel::Confirm | TrustLevel::Observe)
    }

    pub fn can_execute(level: TrustLevel) -> bool {
        matches!(level, TrustLevel::Auto | TrustLevel::Confirm)
    }

    pub fn is_denied(level: TrustLevel) -> bool {
        matches!(level, TrustLevel::Deny)
    }
}

pub fn load_grants_map(vault: &Vault, playbook_id: &str) -> Result<HashMap<String, TrustLevel>> {
    let mut map = HashMap::new();
    for grant in vault.list_trust_grants()? {
        if grant.playbook_id == playbook_id {
            map.insert(grant.step_id.clone(), grant.level);
        }
    }
    Ok(map)
}

pub fn summarize_step(step: &PlaybookStep) -> String {
    match step.step_type {
        crate::playbook::StepType::Shell => format!(
            "Shell: {}",
            step.command.as_deref().unwrap_or("<empty>")
        ),
        crate::playbook::StepType::FileRead => {
            format!("File read: {}", step.config)
        }
        crate::playbook::StepType::FileWrite => {
            format!("File write: {}", step.config)
        }
        crate::playbook::StepType::FileTransform => {
            format!("File transform: {}", step.config)
        }
        crate::playbook::StepType::DraftEmail => {
            let to = step.config.get("to").and_then(|v| v.as_str()).unwrap_or("?");
            let subject = step
                .config
                .get("subject")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("Draft email to {to}: {subject}")
        }
        crate::playbook::StepType::HttpLocal => {
            format!("HTTP local: {}", step.config)
        }
        crate::playbook::StepType::Plugin => {
            format!("Plugin: {}", step.config)
        }
        crate::playbook::StepType::LlmTransform => {
            format!("LLM transform: {}", step.config)
        }
    }
}
