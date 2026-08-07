use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{JarvisError, Result};
use crate::types::TrustLevel;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookDocument {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub metadata: PlaybookMetadata,
    pub spec: PlaybookSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookMetadata {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub author: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookSpec {
    #[serde(default)]
    pub triggers: PlaybookTriggers,
    #[serde(default)]
    pub defaults: PlaybookDefaults,
    #[serde(default)]
    pub schedule_trust: Option<TrustLevel>,
    #[serde(default)]
    pub allow_paths: Vec<String>,
    #[serde(default)]
    pub steps: Vec<PlaybookStep>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlaybookTriggers {
    #[serde(default)]
    pub phrases: Vec<String>,
    #[serde(default)]
    pub schedule: Option<String>,
    #[serde(default)]
    pub calendar: Vec<CalendarTrigger>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarTrigger {
    pub title_contains: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookDefaults {
    #[serde(default)]
    pub trust: TrustLevel,
}

impl Default for PlaybookDefaults {
    fn default() -> Self {
        Self {
            trust: TrustLevel::Confirm,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookStep {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type")]
    pub step_type: StepType,
    #[serde(default)]
    pub trust: Option<TrustLevel>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub timeout: Option<String>,
    #[serde(default)]
    pub config: serde_json::Value,
    #[serde(default)]
    pub elevated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StepType {
    Shell,
    FileRead,
    FileWrite,
    FileTransform,
    DraftEmail,
    HttpLocal,
    Plugin,
    LlmTransform,
}

#[derive(Debug, Clone)]
pub struct Playbook {
    pub doc: PlaybookDocument,
    pub path: PathBuf,
    pub content_hash: String,
    pub version: i64,
}

impl Playbook {
    pub fn id(&self) -> &str {
        &self.doc.metadata.name
    }

    pub fn ordered_steps(&self) -> Result<Vec<&PlaybookStep>> {
        topological_sort(&self.doc.spec.steps)
    }

    pub fn matches_phrase(&self, phrase: &str) -> bool {
        let normalized = phrase.trim().to_lowercase();
        self.doc
            .spec
            .triggers
            .phrases
            .iter()
            .any(|p| normalized.contains(&p.to_lowercase()) || p.to_lowercase().contains(&normalized))
    }
}

pub fn load_playbook(path: &Path) -> Result<Playbook> {
    let content = fs::read_to_string(path)
        .map_err(|e| JarvisError::Playbook(format!("read {}: {e}", path.display())))?;
    let doc: PlaybookDocument = serde_yaml::from_str(&content)?;
    validate_playbook(&doc)?;
    let content_hash = hash_content(&content);
    Ok(Playbook {
        doc,
        path: path.to_path_buf(),
        content_hash,
        version: 1,
    })
}

pub fn load_playbooks_from_dir(dir: &Path) -> Result<Vec<Playbook>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut playbooks = Vec::new();
    for entry in walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext == "yaml" || ext == "yml")
        })
    {
        playbooks.push(load_playbook(entry.path())?);
    }
    playbooks.sort_by(|a, b| a.id().cmp(b.id()));
    Ok(playbooks)
}

pub fn validate_playbook(doc: &PlaybookDocument) -> Result<()> {
    if doc.api_version != "jarvis.dev/v1" {
        return Err(JarvisError::Playbook(format!(
            "unsupported apiVersion: {}",
            doc.api_version
        )));
    }
    if doc.kind != "Playbook" {
        return Err(JarvisError::Playbook(format!(
            "unsupported kind: {}",
            doc.kind
        )));
    }
    if doc.metadata.name.is_empty() {
        return Err(JarvisError::Playbook("metadata.name is required".into()));
    }
    if doc.spec.steps.is_empty() {
        return Err(JarvisError::Playbook("spec.steps cannot be empty".into()));
    }

    let mut ids = HashSet::new();
    for step in &doc.spec.steps {
        if step.id.is_empty() {
            return Err(JarvisError::Playbook("step id cannot be empty".into()));
        }
        if !ids.insert(step.id.clone()) {
            return Err(JarvisError::Playbook(format!(
                "duplicate step id: {}",
                step.id
            )));
        }
        validate_step(step)?;
    }

    for step in &doc.spec.steps {
        for dep in &step.depends_on {
            if !ids.contains(dep) {
                return Err(JarvisError::Playbook(format!(
                    "step '{}' depends on unknown step '{}'",
                    step.id, dep
                )));
            }
        }
    }

    Ok(())
}

fn validate_step(step: &PlaybookStep) -> Result<()> {
    match step.step_type {
        StepType::Shell => {
            if step.command.as_ref().is_none_or(|c| c.is_empty()) {
                return Err(JarvisError::Playbook(format!(
                    "shell step '{}' requires command",
                    step.id
                )));
            }
        }
        StepType::FileRead | StepType::FileWrite | StepType::FileTransform => {
            if step.config.is_null() {
                return Err(JarvisError::Playbook(format!(
                    "file step '{}' requires config",
                    step.id
                )));
            }
        }
        StepType::DraftEmail => {
            if step.config.get("to").is_none() {
                return Err(JarvisError::Playbook(format!(
                    "draft-email step '{}' requires config.to",
                    step.id
                )));
            }
        }
        StepType::HttpLocal => {
            if step.config.get("url").is_none() {
                return Err(JarvisError::Playbook(format!(
                    "http-local step '{}' requires config.url",
                    step.id
                )));
            }
        }
        StepType::Plugin => {
            if step.config.get("plugin").is_none() {
                return Err(JarvisError::Playbook(format!(
                    "plugin step '{}' requires config.plugin",
                    step.id
                )));
            }
        }
        StepType::LlmTransform => {
            if step.config.get("prompt").is_none() {
                return Err(JarvisError::Playbook(format!(
                    "llm-transform step '{}' requires config.prompt",
                    step.id
                )));
            }
        }
    }
    Ok(())
}

pub fn topological_sort(steps: &[PlaybookStep]) -> Result<Vec<&PlaybookStep>> {
    let index: HashMap<&str, &PlaybookStep> = steps.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut in_degree: HashMap<&str, usize> = steps.iter().map(|s| (s.id.as_str(), 0)).collect();
    let mut graph: HashMap<&str, Vec<&str>> = HashMap::new();

    for step in steps {
        for dep in &step.depends_on {
            graph.entry(dep.as_str()).or_default().push(step.id.as_str());
            *in_degree.entry(step.id.as_str()).or_insert(0) += 1;
        }
    }

    let mut queue: Vec<&str> = in_degree
        .iter()
        .filter(|(_, deg)| **deg == 0)
        .map(|(id, _)| *id)
        .collect();
    queue.sort();

    let mut ordered = Vec::new();
    while let Some(id) = queue.pop() {
        ordered.push(index[id]);
        if let Some(children) = graph.get(id) {
            for child in children {
                let deg = in_degree.get_mut(child).unwrap();
                *deg -= 1;
                if *deg == 0 {
                    queue.push(child);
                }
            }
        }
    }

    if ordered.len() != steps.len() {
        return Err(JarvisError::Playbook(
            "cyclic dependency detected in steps".into(),
        ));
    }

    Ok(ordered)
}

pub fn hash_content(content: &str) -> String {
    let hash = Sha256::digest(content.as_bytes());
    hex::encode(hash)
}

pub fn playbook_to_yaml(doc: &PlaybookDocument) -> Result<String> {
    Ok(serde_yaml::to_string(doc)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topological_sort_respects_dependencies() {
        let steps = vec![
            PlaybookStep {
                id: "a".into(),
                name: String::new(),
                step_type: StepType::Shell,
                trust: None,
                depends_on: vec![],
                cwd: None,
                command: Some("echo a".into()),
                timeout: None,
                config: serde_json::json!({}),
                elevated: false,
            },
            PlaybookStep {
                id: "b".into(),
                name: String::new(),
                step_type: StepType::Shell,
                trust: None,
                depends_on: vec!["a".into()],
                cwd: None,
                command: Some("echo b".into()),
                timeout: None,
                config: serde_json::json!({}),
                elevated: false,
            },
        ];
        let ordered = topological_sort(&steps).unwrap();
        assert_eq!(ordered[0].id, "a");
        assert_eq!(ordered[1].id, "b");
    }
}
