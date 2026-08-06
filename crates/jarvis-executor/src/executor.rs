use std::time::Duration;

use jarvis_core::playbook::PlaybookStep;
use jarvis_core::variables::VariableContext;
use jarvis_core::VaultPaths;
use serde::{Deserialize, Serialize};

use crate::steps;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub run_id: String,
    pub paths: VaultPaths,
    pub allow_paths: Vec<String>,
    pub vars: VariableContext,
    pub backup_dir: std::path::PathBuf,
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
        step: &PlaybookStep,
    ) -> jarvis_core::Result<ExecutionResult> {
        steps::dispatch(ctx, step).await
    }

    pub fn parse_timeout(step: &PlaybookStep) -> Duration {
        step.timeout
            .as_deref()
            .and_then(parse_duration)
            .unwrap_or(Duration::from_secs(300))
    }
}

fn parse_duration(s: &str) -> Option<Duration> {
    if let Some(secs) = s.strip_suffix('s') {
        secs.parse().ok().map(Duration::from_secs)
    } else if let Some(mins) = s.strip_suffix('m') {
        mins.parse().ok().map(|m: u64| Duration::from_secs(m * 60))
    } else {
        s.parse().ok().map(Duration::from_secs)
    }
}
