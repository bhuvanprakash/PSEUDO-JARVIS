use std::fs;
use std::path::Path;

use dialoguer::Confirm;
use jarvis_core::paths::{write_auth_token, VaultPaths};
use jarvis_core::playbook::load_playbook;
use jarvis_core::types::{ConfirmRequest, ConfirmResponse};
use jarvis_core::{Config, Vault};
use jarvis_observer::record_terminal_event;
use jarvis_observer::filesystem::install_shell_hook;
use jarvis_plugin::install_builtin_slack_plugin;
use jarvis_replay::{export_markdown, load_manifest, load_steps, prune_replays};
use jarvis_server::ipc::{ipc_call, IpcRequest};
use jarvis_server::orchestrator::{ConfirmCallback, Orchestrator, RunOptions};
use jarvis_suggester::PatternSuggester;
use jarvis_sync::VaultSync;

use crate::{
    load_state, ObserveCommands, PlaybookCommands, PluginCommands, ReplayCommands, SuggestCommands,
    SyncCommands,
};

pub async fn init(vault: Option<String>) -> anyhow::Result<()> {
    let paths = VaultPaths::from_env_or_default(vault.as_deref());
    paths.ensure_layout()?;
    let config = Config::default();
    config.save(&paths.config())?;
    write_auth_token(&paths.auth_token())?;

    let community_src = Path::new("playbooks/community");
    if community_src.exists() {
        copy_dir_all(community_src, &paths.community_playbooks())?;
    }

    install_builtin_slack_plugin(&paths.plugins())?;

    let tray_src = Path::new("tray-ui");
    if tray_src.exists() {
        copy_dir_all(tray_src, &paths.root.join("tray"))?;
    }

    let vault = Vault::open(paths.clone())?;
    vault.sync_playbooks()?;

    println!("PSEUDO-JARVIS initialized at {}", paths.root.display());
    println!("Next: jarvis playbook list");
    Ok(())
}

pub async fn run(
    playbook: Option<String>,
    phrase: Option<String>,
    auto_approve: bool,
    vault: Option<String>,
) -> anyhow::Result<()> {
    let (state, _paths) = load_state(vault)?;
    let confirm: Option<ConfirmCallback> = if auto_approve {
        None
    } else {
        Some(Box::new(|req: ConfirmRequest| {
            Box::pin(async move {
                println!("\n--- Confirmation required ---");
                println!("Step: {} ({})", req.step_name, req.trust);
                println!("{}", req.summary);
                let approve = Confirm::new()
                    .with_prompt("Approve this step?")
                    .default(false)
                    .interact()
                    .unwrap_or(false);
                if approve {
                    ConfirmResponse::Approve
                } else {
                    ConfirmResponse::Skip
                }
            })
        }))
    };

    let run_id = Orchestrator::run(
        &state,
        RunOptions {
            playbook_id: playbook,
            phrase,
            trigger: "cli".into(),
            schedule_triggered: false,
            auto_approve,
        },
        confirm,
    )
    .await?;

    println!("Run completed: {run_id}");
    println!("Replay: jarvis replay show {run_id}");
    Ok(())
}

pub async fn playbook(cmd: PlaybookCommands, vault: Option<String>) -> anyhow::Result<()> {
    let (state, paths) = load_state(vault)?;
    match cmd {
        PlaybookCommands::List => {
            let ids = state.vault.list_playbook_ids()?;
            if ids.is_empty() {
                println!("No playbooks found. Import one with `jarvis playbook import`.");
            } else {
                for id in ids {
                    println!("- {id}");
                }
            }
        }
        PlaybookCommands::Show { id } => {
            let pb = state.vault.get_playbook(&id)?;
            println!("{}", serde_yaml::to_string(&pb.doc)?);
        }
        PlaybookCommands::Validate { path } => {
            let pb = load_playbook(&path)?;
            println!("Valid: {} ({})", pb.id(), path.display());
        }
        PlaybookCommands::Import { path } => {
            let pb = load_playbook(&path)?;
            let dest = paths.playbooks().join(format!("{}.yaml", pb.id()));
            fs::copy(&path, &dest)?;
            state.vault.sync_playbooks()?;
            println!("Imported {} -> {}", pb.id(), dest.display());
        }
    }
    Ok(())
}

pub async fn observe(cmd: ObserveCommands, vault: Option<String>) -> anyhow::Result<()> {
    let (state, paths) = load_state(vault)?;
    match cmd {
        ObserveCommands::Status => {
            let observer = state.observer.lock().await;
            println!("{}", serde_json::to_string_pretty(&observer.status())?);
        }
        ObserveCommands::Enable { path } => {
            let mut config = state.config.clone();
            config.observer.enabled = true;
            config.observer.filesystem.enabled = true;
            for p in path {
                config.observer.filesystem.watch.push(p);
            }
            config.save(&paths.config())?;
            println!("Observer enabled for paths: {:?}", config.observer.filesystem.watch);
        }
        ObserveCommands::Pause => {
            let mut config = state.config.clone();
            config.observer.paused = true;
            config.save(&paths.config())?;
            println!("Observer paused");
        }
        ObserveCommands::Resume => {
            let mut config = state.config.clone();
            config.observer.paused = false;
            config.save(&paths.config())?;
            println!("Observer resumed");
        }
        ObserveCommands::Event { command, cwd } => {
            record_terminal_event(&state.vault, &command, &cwd, None, None)?;
            println!("Recorded terminal event");
        }
        ObserveCommands::Hook { shell } => {
            let snippet = install_shell_hook(&paths.root, &shell)?;
            println!("Add to your {shell} rc:\n{snippet}");
        }
    }
    Ok(())
}

pub async fn replay(cmd: ReplayCommands, vault: Option<String>) -> anyhow::Result<()> {
    let (state, paths) = load_state(vault)?;
    match cmd {
        ReplayCommands::List => {
            for run in state.vault.list_runs(20)? {
                println!(
                    "{}  {}  {}  {}",
                    run.id,
                    run.playbook_id,
                    run.status,
                    run.started_at.unwrap_or_default()
                );
            }
        }
        ReplayCommands::Show { run_id } => {
            let dir = paths.run_replay_dir(&run_id);
            let manifest = load_manifest(&dir)?;
            let steps = load_steps(&dir)?;
            println!("{}", serde_json::to_string_pretty(&manifest)?);
            for step in steps {
                println!("  {} [{}] {}", step.step_id, step.status, step.trust_level);
            }
        }
        ReplayCommands::Export { run_id } => {
            let dir = paths.run_replay_dir(&run_id);
            println!("{}", export_markdown(&dir)?);
        }
        ReplayCommands::Prune { days } => {
            let n = prune_replays(&paths, days)?;
            println!("Pruned {n} replays older than {days} days");
        }
    }
    Ok(())
}

pub async fn suggest(cmd: SuggestCommands, vault: Option<String>) -> anyhow::Result<()> {
    let (state, _paths) = load_state(vault)?;
    let suggester = PatternSuggester::new(&state.vault, state.llm.as_ref());
    match cmd {
        SuggestCommands::Scan { days } => {
            let patterns = suggester.scan(days).await?;
            if patterns.is_empty() {
                println!("No patterns found. Enable observer and use your terminal.");
            } else {
                for p in patterns {
                    println!(
                        "- {} (confidence {:.0}%) {:?}",
                        p.id,
                        p.confidence * 100.0,
                        p.suggested_name
                    );
                }
            }
        }
        SuggestCommands::List => {
            for p in state.vault.list_patterns(Some("pending"))? {
                println!("{} {:?} ({:.0}%)", p.id, p.suggested_name, p.confidence * 100.0);
            }
        }
        SuggestCommands::Accept { id } => {
            let pb = suggester.accept_pattern(&id).await?;
            println!("Created playbook: {}", pb.id());
        }
        SuggestCommands::Dismiss { id } => {
            suggester.dismiss_pattern(&id)?;
            println!("Dismissed pattern {id}");
        }
    }
    Ok(())
}

pub async fn plugin(cmd: PluginCommands, vault: Option<String>) -> anyhow::Result<()> {
    let (_state, paths) = load_state(vault)?;
    match cmd {
        PluginCommands::List => {
            if paths.plugins().exists() {
                for entry in fs::read_dir(paths.plugins())? {
                    let entry = entry?;
                    if entry.file_type()?.is_dir() {
                        println!("- {}", entry.file_name().to_string_lossy());
                    }
                }
            }
        }
        PluginCommands::Enable { name } => {
            install_builtin_slack_plugin(&paths.plugins())?;
            println!("Plugin '{name}' available at {}", paths.plugins().join(&name).display());
        }
    }
    Ok(())
}

pub async fn sync(cmd: SyncCommands, vault: Option<String>) -> anyhow::Result<()> {
    let (state, paths) = load_state(vault)?;
    let sync_dir = state
        .config
        .sync
        .sync_dir
        .clone()
        .map(PathBuf::from)
        .unwrap_or_else(|| paths.root.join("sync"));
    let sync = VaultSync::new(paths.clone(), sync_dir);
    match cmd {
        SyncCommands::Export => {
            let archive = sync.export_snapshot()?;
            println!("Exported vault snapshot to {}", archive.display());
        }
        SyncCommands::Import { archive } => {
            sync.import_snapshot(&archive)?;
            println!("Imported vault snapshot from {}", archive.display());
        }
    }
    Ok(())
}

pub async fn status(vault: Option<String>) -> anyhow::Result<()> {
    let (_state, paths) = load_state(vault)?;
    if paths.socket().exists() {
        let resp = ipc_call(&paths.socket(), &IpcRequest::Status).await?;
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!("Daemon not running. Start with: jarvis daemon");
    }
    Ok(())
}

pub async fn daemon(vault: Option<String>) -> anyhow::Result<()> {
    let paths = VaultPaths::from_env_or_default(vault.as_deref());
    let exe = std::env::current_exe()?;
    let jarvisd = exe
        .parent()
        .map(|p| p.join("jarvisd"))
        .filter(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from("jarvisd"));
    let status = std::process::Command::new(jarvisd)
        .arg("--foreground")
        .env("JARVIS_HOME", paths.root.display().to_string())
        .status()?;
    if !status.success() {
        anyhow::bail!("jarvisd exited with {status}");
    }
    Ok(())
}

use std::path::PathBuf;

fn copy_dir_all(src: &Path, dst: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dest = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dest)?;
        } else {
            fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}
