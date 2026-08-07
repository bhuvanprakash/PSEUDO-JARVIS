use std::fs;
use std::path::Path;
use std::process::Stdio;

use glob::glob;
use jarvis_core::error::JarvisError;
use jarvis_core::paths::{path_allowed, resolve_path};
use jarvis_core::playbook::{PlaybookStep, StepType};
use jarvis_llm::{CompleteOpts, LlmProvider, OllamaProvider};
use serde_json::json;
use tokio::process::Command;
use tokio::time::timeout;

use crate::executor::{ExecutionContext, ExecutionResult, Executor, StepOutput};
use crate::sandbox::{is_command_blocked, redact_secrets};

pub async fn dispatch(
    ctx: &ExecutionContext,
    step: &PlaybookStep,
) -> jarvis_core::Result<ExecutionResult> {
    match step.step_type {
        StepType::Shell => run_shell(ctx, step).await,
        StepType::FileRead => run_file_read(ctx, step).await,
        StepType::FileWrite => run_file_write(ctx, step).await,
        StepType::FileTransform => run_file_transform(ctx, step).await,
        StepType::DraftEmail => run_draft_email(ctx, step).await,
        StepType::HttpLocal => run_http_local(ctx, step).await,
        StepType::Plugin => run_plugin_step(ctx, step).await,
        StepType::LlmTransform => run_llm_transform(ctx, step).await,
    }
}

pub async fn run_shell(ctx: &ExecutionContext, step: &PlaybookStep) -> jarvis_core::Result<ExecutionResult> {
    let command = step
        .command
        .as_ref()
        .ok_or_else(|| JarvisError::Execution("shell step missing command".into()))?;
    let command = ctx.vars.interpolate(command);

    if let Some(reason) = is_command_blocked(&command) {
        return Err(JarvisError::Execution(format!("blocked command: {reason}")));
    }

    let cwd = step
        .cwd
        .as_ref()
        .map(|p| resolve_path(&ctx.vars.interpolate(p)))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    if !path_allowed(&cwd, &ctx.allow_paths) {
        return Err(JarvisError::Execution(format!(
            "cwd not allowed: {}",
            cwd.display()
        )));
    }

    let duration = Executor::parse_timeout(step);
    let mut cmd = if cfg!(target_os = "windows") {
        let mut c = Command::new("cmd");
        c.args(["/C", &command]);
        c
    } else {
        let mut c = Command::new("sh");
        c.args(["-c", &command]);
        c
    };

    let output = timeout(
        duration,
        cmd.current_dir(&cwd)
            .env("JARVIS_RUN_ID", &ctx.run_id)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output(),
    )
    .await
    .map_err(|_| JarvisError::Execution(format!("step '{}' timed out", step.id)))?
    .map_err(|e| JarvisError::Execution(format!("shell failed: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let success = output.status.success();

    Ok(ExecutionResult {
        output: StepOutput {
            success,
            stdout: redact_secrets(&stdout),
            stderr: redact_secrets(&stderr),
            data: json!({
                "exit_code": output.status.code(),
                "command": command,
                "cwd": cwd.display().to_string(),
            }),
        },
        files_changed: Vec::new(),
    })
}

pub async fn run_file_read(ctx: &ExecutionContext, step: &PlaybookStep) -> jarvis_core::Result<ExecutionResult> {
    let config = ctx.vars.interpolate_json(&step.config);
    let path_str = config
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| JarvisError::Execution("file-read requires config.path".into()))?;
    let path = resolve_path(path_str);
    ensure_allowed(&path, &ctx.allow_paths)?;
    let content = fs::read_to_string(&path)
        .map_err(|e| JarvisError::Execution(format!("read {}: {e}", path.display())))?;
    Ok(ExecutionResult {
        output: StepOutput {
            success: true,
            stdout: content.clone(),
            stderr: String::new(),
            data: json!({ "path": path.display().to_string(), "bytes": content.len() }),
        },
        files_changed: Vec::new(),
    })
}

pub async fn run_file_write(ctx: &ExecutionContext, step: &PlaybookStep) -> jarvis_core::Result<ExecutionResult> {
    let config = ctx.vars.interpolate_json(&step.config);
    let path_str = config
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| JarvisError::Execution("file-write requires config.path".into()))?;
    let content = config
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let path = resolve_path(path_str);
    ensure_allowed(&path, &ctx.allow_paths)?;
    backup_file(&ctx.backup_dir, &path)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, content)?;
    Ok(ExecutionResult {
        output: StepOutput {
            success: true,
            stdout: format!("wrote {}", path.display()),
            stderr: String::new(),
            data: json!({ "path": path.display().to_string() }),
        },
        files_changed: vec![path.display().to_string()],
    })
}

pub async fn run_file_transform(
    ctx: &ExecutionContext,
    step: &PlaybookStep,
) -> jarvis_core::Result<ExecutionResult> {
    let config = ctx.vars.interpolate_json(&step.config);
    let glob_pattern = config
        .get("glob")
        .and_then(|v| v.as_str())
        .ok_or_else(|| JarvisError::Execution("file-transform requires config.glob".into()))?;
    let glob_pattern = ctx.vars.interpolate(glob_pattern);
    let mut changed = Vec::new();

    if let Some(rename_template) = config.get("rename").and_then(|v| v.as_str()) {
        for entry in glob(&glob_pattern).map_err(|e| JarvisError::Execution(e.to_string()))? {
            let path = entry.map_err(|e| JarvisError::Execution(e.to_string()))?;
            ensure_allowed(&path, &ctx.allow_paths)?;
            let file_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file");
            let new_name = ctx.vars.interpolate(rename_template).replace("{filename}", file_name);
            let new_path = path.with_file_name(new_name);
            backup_file(&ctx.backup_dir, &path)?;
            fs::rename(&path, &new_path)?;
            changed.push(new_path.display().to_string());
        }
    }

    if let Some(move_to) = config.get("move_to").and_then(|v| v.as_str()) {
        let dest_dir = resolve_path(&ctx.vars.interpolate(move_to));
        fs::create_dir_all(&dest_dir)?;
        ensure_allowed(&dest_dir, &ctx.allow_paths)?;
        for entry in glob(&glob_pattern).map_err(|e| JarvisError::Execution(e.to_string()))? {
            let path = entry.map_err(|e| JarvisError::Execution(e.to_string()))?;
            ensure_allowed(&path, &ctx.allow_paths)?;
            let file_name = path.file_name().ok_or_else(|| {
                JarvisError::Execution("path has no file name".into())
            })?;
            let dest = dest_dir.join(file_name);
            backup_file(&ctx.backup_dir, &path)?;
            fs::rename(&path, &dest)?;
            changed.push(dest.display().to_string());
        }
    }

    Ok(ExecutionResult {
        output: StepOutput {
            success: true,
            stdout: format!("transformed {} files", changed.len()),
            stderr: String::new(),
            data: json!({ "files": changed }),
        },
        files_changed: changed,
    })
}

pub async fn run_draft_email(ctx: &ExecutionContext, step: &PlaybookStep) -> jarvis_core::Result<ExecutionResult> {
    let config = ctx.vars.interpolate_json(&step.config);
    let to = config
        .get("to")
        .and_then(|v| v.as_str())
        .ok_or_else(|| JarvisError::Execution("draft-email requires config.to".into()))?;
    let subject = config
        .get("subject")
        .and_then(|v| v.as_str())
        .unwrap_or("(no subject)");
    let body = if let Some(template) = config.get("body_template").and_then(|v| v.as_str()) {
        let template_path = resolve_path(template);
        fs::read_to_string(&template_path).unwrap_or_else(|_| String::new())
    } else {
        config
            .get("body")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    let body = ctx.vars.interpolate(&body);

    let draft_dir = ctx.paths.drafts();
    fs::create_dir_all(&draft_dir)?;
    let draft_path = draft_dir.join(format!("{}_{}.eml", ctx.run_id, step.id));
    let eml = format!(
        "To: {to}\r\nSubject: {subject}\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{body}\r\n"
    );
    fs::write(&draft_path, &eml)?;

    Ok(ExecutionResult {
        output: StepOutput {
            success: true,
            stdout: format!("draft saved to {}", draft_path.display()),
            stderr: String::new(),
            data: json!({
                "to": to,
                "subject": subject,
                "draft_path": draft_path.display().to_string(),
            }),
        },
        files_changed: vec![draft_path.display().to_string()],
    })
}

pub async fn run_http_local(ctx: &ExecutionContext, step: &PlaybookStep) -> jarvis_core::Result<ExecutionResult> {
    let config = ctx.vars.interpolate_json(&step.config);
    let url = config
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| JarvisError::Execution("http-local requires config.url".into()))?;
    if !url.starts_with("http://127.0.0.1") && !url.starts_with("http://localhost") {
        return Err(JarvisError::Execution(
            "http-local only allows localhost URLs".into(),
        ));
    }
    let client = reqwest::Client::new();
    let method = config
        .get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("GET");
    let response = match method.to_uppercase().as_str() {
        "POST" => {
            let body = config.get("body").cloned().unwrap_or(json!({}));
            client.post(url).json(&body).send().await
        }
        _ => client.get(url).send().await,
    }
    .map_err(|e| JarvisError::Execution(format!("http request failed: {e}")))?;
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    Ok(ExecutionResult {
        output: StepOutput {
            success: (200..300).contains(&status),
            stdout: body,
            stderr: String::new(),
            data: json!({ "status": status, "url": url }),
        },
        files_changed: Vec::new(),
    })
}

pub async fn run_plugin_step(ctx: &ExecutionContext, step: &PlaybookStep) -> jarvis_core::Result<ExecutionResult> {
    let config = ctx.vars.interpolate_json(&step.config);
    let plugin_name = config
        .get("plugin")
        .and_then(|v| v.as_str())
        .ok_or_else(|| JarvisError::Execution("plugin step requires config.plugin".into()))?;
    let plugin_dir = ctx.paths.plugins().join(plugin_name);
    let manifest_path = plugin_dir.join("plugin.yaml");
    if !manifest_path.exists() {
        return Err(JarvisError::Execution(format!(
            "plugin '{}' not installed",
            plugin_name
        )));
    }
    let input = json!({
        "run_id": ctx.run_id,
        "step_id": step.id,
        "config": config,
    });
    let output = jarvis_plugin::run_plugin(&plugin_dir, &input).await?;
    Ok(ExecutionResult {
        output: StepOutput {
            success: output.get("success").and_then(|v| v.as_bool()).unwrap_or(false),
            stdout: output
                .get("stdout")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            stderr: output
                .get("stderr")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            data: output,
        },
        files_changed: Vec::new(),
    })
}

pub async fn run_llm_transform(
    ctx: &ExecutionContext,
    step: &PlaybookStep,
) -> jarvis_core::Result<ExecutionResult> {
    let config = ctx.vars.interpolate_json(&step.config);
    let prompt = config
        .get("prompt")
        .and_then(|v| v.as_str())
        .ok_or_else(|| JarvisError::Execution("llm-transform requires config.prompt".into()))?;
    let input = config
        .get("input")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let full_prompt = format!("{prompt}\n\nInput:\n{input}");

    let result_text = if ctx.llm_enabled {
        let provider = OllamaProvider::new(&ctx.llm_endpoint);
        provider
            .complete(
                &full_prompt,
                CompleteOpts {
                    model: ctx.llm_model.clone(),
                    temperature: 0.3,
                },
            )
            .await
            .unwrap_or_else(|e| format!("LLM error: {e}"))
    } else {
        format!("[LLM disabled] would process: {}", full_prompt.len())
    };

    if let Some(out_path) = config.get("output_path").and_then(|v| v.as_str()) {
        let path = resolve_path(&ctx.vars.interpolate(out_path));
        ensure_allowed(&path, &ctx.allow_paths)?;
        backup_file(&ctx.backup_dir, &path)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, &result_text)?;
        return Ok(ExecutionResult {
            output: StepOutput {
                success: true,
                stdout: result_text.clone(),
                stderr: String::new(),
                data: json!({ "output_path": path.display().to_string() }),
            },
            files_changed: vec![path.display().to_string()],
        });
    }

    Ok(ExecutionResult {
        output: StepOutput {
            success: true,
            stdout: result_text.clone(),
            stderr: String::new(),
            data: json!({ "result": result_text }),
        },
        files_changed: Vec::new(),
    })
}

fn ensure_allowed(path: &Path, allowlist: &[String]) -> jarvis_core::Result<()> {
    if !path_allowed(path, allowlist) {
        return Err(JarvisError::Execution(format!(
            "path not allowed: {}",
            path.display()
        )));
    }
    Ok(())
}

fn backup_file(backup_dir: &Path, path: &Path) -> jarvis_core::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    fs::create_dir_all(backup_dir)?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("backup");
    let backup_path = backup_dir.join(file_name);
    fs::copy(path, backup_path)?;
    Ok(())
}
