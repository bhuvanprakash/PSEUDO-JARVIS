use std::path::PathBuf;
use std::sync::Arc;

use jarvis_core::paths::VaultPaths;
use jarvis_core::playbook::PlaybookStep;
use jarvis_core::variables::VariableContext;
use jarvis_llm::LlmProvider;
use serde::{Deserialize, Serialize};

use crate::steps;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub data: serde_json::Value,
}

#[derive(Clone)]
pub struct ExecutionContext {
    pub run_id: String,
    pub paths: VaultPaths,
    pub resolved_allow: Arc<Vec<PathBuf>>,
    pub backup_dir: std::path::PathBuf,
    pub llm: Option<Arc<dyn LlmProvider>>,
    pub llm_model: String,
    pub http_client: reqwest::Client,
}

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub output: StepOutput,
    pub files_changed: Vec<String>,
}

pub struct Executor;

impl Executor {
    pub async fn execute_step(
        ctx: &ExecutionContext,
        vars: &VariableContext,
        step: &PlaybookStep,
    ) -> jarvis_core::Result<ExecutionResult> {
        steps::dispatch(ctx, vars, step).await
    }

    pub fn parse_timeout(step: &PlaybookStep) -> std::time::Duration {
        step.timeout
            .as_deref()
            .and_then(parse_duration)
            .unwrap_or(std::time::Duration::from_secs(300))
    }
}

fn parse_duration(s: &str) -> Option<std::time::Duration> {
    if let Some(secs) = s.strip_suffix('s') {
        secs.parse().ok().map(std::time::Duration::from_secs)
    } else if let Some(mins) = s.strip_suffix('m') {
        mins.parse()
            .ok()
            .map(|m: u64| std::time::Duration::from_secs(m * 60))
    } else {
        s.parse().ok().map(std::time::Duration::from_secs)
    }
}
