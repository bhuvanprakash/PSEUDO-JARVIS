use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use jarvis_core::error::JarvisError;
use jarvis_core::playbook::Playbook;
use jarvis_core::trust::{summarize_step, TrustResolver};
use jarvis_core::types::{ConfirmRequest, ConfirmResponse, TrustLevel};
use jarvis_core::variables::VariableContext;
use jarvis_executor::{ExecutionContext, Executor};
use jarvis_replay::{ReplayRecorder, ReplayStepLog};
use uuid::Uuid;

use crate::state::AppState;

pub type ConfirmCallback = Box<
    dyn Fn(ConfirmRequest) -> Pin<Box<dyn Future<Output = ConfirmResponse> + Send>> + Send + Sync,
>;

pub struct RunOptions {
    pub playbook_id: Option<String>,
    pub phrase: Option<String>,
    pub trigger: String,
    pub schedule_triggered: bool,
    pub auto_approve: bool,
}

pub struct Orchestrator;

impl Orchestrator {
    pub async fn run(
        state: &AppState,
        options: RunOptions,
        confirm: Option<ConfirmCallback>,
    ) -> jarvis_core::Result<String> {
        let playbook = resolve_playbook(state, &options).await?;
        let run_id = Uuid::new_v4().to_string();
        state.vault.create_run(
            &run_id,
            playbook.id(),
            playbook.version,
            &options.trigger,
            options.phrase.as_deref(),
        )?;

        let steps = playbook.ordered_steps()?;
        let backup_dir = state.paths.run_backup_dir(&run_id);
        std::fs::create_dir_all(&backup_dir)?;

        let mut recorder = ReplayRecorder::new(
            &state.paths,
            &run_id,
            playbook.id(),
            playbook.version,
            &options.trigger,
            options.phrase.as_deref(),
            steps.len(),
        )?;

        let mut vars = VariableContext::new(&run_id);
        let trust_resolver = TrustResolver::new();
        let mut files_diff = Vec::new();

        state
            .vault
            .update_run_status(&run_id, "running", None)?;

        for step in steps {
            let trust = trust_resolver.resolve(
                &state.vault,
                &playbook,
                step,
                options.schedule_triggered,
            )?;

            if TrustResolver::is_denied(trust) {
                let msg = format!("step '{}' denied by trust policy", step.id);
                state
                    .vault
                    .update_run_status(&run_id, "failed", Some(&msg))?;
                recorder.finish("failed")?;
                return Err(JarvisError::TrustDenied {
                    step: step.id.clone(),
                    reason: msg,
                });
            }

            if TrustResolver::requires_confirmation(trust) && !options.auto_approve {
                if let Some(ref callback) = confirm {
                    let response = callback(ConfirmRequest {
                        run_id: run_id.clone(),
                        step_id: step.id.clone(),
                        step_name: if step.name.is_empty() {
                            step.id.clone()
                        } else {
                            step.name.clone()
                        },
                        trust,
                        summary: summarize_step(step),
                        details: serde_json::json!({
                            "type": format!("{:?}", step.step_type),
                            "config": step.config,
                            "command": step.command,
                        }),
                    })
                    .await;

                    recorder.record_step(&ReplayStepLog {
                        step_id: step.id.clone(),
                        trust_level: trust.to_string(),
                        status: "awaiting_confirm".into(),
                        input: None,
                        output: Some(serde_json::json!({ "response": format!("{:?}", response) })),
                        started_at: chrono::Utc::now().to_rfc3339(),
                        finished_at: chrono::Utc::now().to_rfc3339(),
                    })?;

                    match response {
                        ConfirmResponse::Abort => {
                            state
                                .vault
                                .update_run_status(&run_id, "cancelled", None)?;
                            recorder.finish("cancelled")?;
                            return Ok(run_id);
                        }
                        ConfirmResponse::Skip => {
                            state.vault.insert_run_step(
                                &run_id,
                                &step.id,
                                &trust.to_string(),
                                "skipped",
                                None,
                                None,
                            )?;
                            continue;
                        }
                        ConfirmResponse::Approve => {}
                    }
                } else if !options.auto_approve {
                    return Err(JarvisError::Execution(format!(
                        "step '{}' requires confirmation; use interactive mode",
                        step.id
                    )));
                }
            }

            let ctx = ExecutionContext {
                run_id: run_id.clone(),
                paths: state.paths.clone(),
                allow_paths: playbook.doc.spec.allow_paths.clone(),
                vars: vars.clone(),
                backup_dir: backup_dir.clone(),
            };

            let started = chrono::Utc::now().to_rfc3339();
            let result = Executor::execute_step(&ctx, step).await;
            let finished = chrono::Utc::now().to_rfc3339();

            match result {
                Ok(exec) => {
                    vars.set_step_output(&step.id, "stdout", &exec.output.stdout);
                    files_diff.extend(exec.files_changed.clone());
                    state.vault.insert_run_step(
                        &run_id,
                        &step.id,
                        &trust.to_string(),
                        "completed",
                        None,
                        Some(&serde_json::json!(exec.output)),
                    )?;
                    recorder.record_step(&ReplayStepLog {
                        step_id: step.id.clone(),
                        trust_level: trust.to_string(),
                        status: "completed".into(),
                        input: None,
                        output: Some(serde_json::json!(exec.output)),
                        started_at: started,
                        finished_at: finished,
                    })?;
                }
                Err(e) => {
                    state
                        .vault
                        .update_run_status(&run_id, "failed", Some(&e.to_string()))?;
                    recorder.finish("failed")?;
                    return Err(e);
                }
            }
        }

        if !files_diff.is_empty() {
            recorder.write_files_diff(&files_diff.join("\n"))?;
        }
        state
            .vault
            .update_run_status(&run_id, "completed", None)?;
        recorder.finish("completed")?;
        Ok(run_id)
    }
}

async fn resolve_playbook(state: &AppState, options: &RunOptions) -> jarvis_core::Result<Playbook> {
    if let Some(id) = &options.playbook_id {
        return state.vault.get_playbook(id);
    }
    if let Some(phrase) = &options.phrase {
        if let Some(pb) = state.vault.find_playbook_by_phrase(phrase)? {
            return Ok(pb);
        }
        let ids = state.vault.list_playbook_ids()?;
        if state.config.llm.enabled {
            if let Some(id) = state.llm.match_playbook(phrase, &ids).await? {
                return state.vault.get_playbook(&id);
            }
        }
        return Err(JarvisError::Playbook(format!(
            "no playbook matches phrase: {phrase}"
        )));
    }
    Err(JarvisError::Playbook(
        "playbook_id or phrase required".into(),
    ))
}
