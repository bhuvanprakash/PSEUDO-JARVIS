use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use jarvis_core::paths::{read_auth_token, write_auth_token, VaultPaths};
use jarvis_core::{Config, Vault};
use jarvis_llm::ollama::create_provider;
use jarvis_plugin::install_builtin_plugins;
use jarvis_server::{http, ipc, messaging, scheduler, AppState};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "jarvisd", about = "PSEUDO-JARVIS daemon")]
struct Args {
    #[arg(long, env = "JARVIS_HOME")]
    vault: Option<String>,
    #[arg(long)]
    foreground: bool,
}

fn resolve_tray_dir(paths: &VaultPaths) -> Option<PathBuf> {
    let vault_tray = paths.root.join("tray");
    if vault_tray.join("index.html").exists() {
        return Some(vault_tray);
    }
    let dev = PathBuf::from("tray-ui");
    if dev.join("index.html").exists() {
        return Some(dev);
    }
    None
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("jarvis=info".parse()?))
        .init();

    let args = Args::parse();
    let paths = VaultPaths::from_env_or_default(args.vault.as_deref());
    paths.ensure_layout()?;

    let config_path = paths.config();
    let mut config = Config::load(&config_path)?;
    if config.vault_dir.is_none() {
        config.vault_dir = Some(paths.root.display().to_string());
    }
    config.save(&config_path)?;

    if !paths.auth_token().exists() {
        write_auth_token(&paths.auth_token())?;
    }
    let _ = read_auth_token(&paths.auth_token())?;

    let vault = Arc::new(Vault::open(paths.clone())?);
    vault.sync_playbooks()?;
    install_builtin_plugins(&paths.plugins())?;

    let llm: Arc<dyn jarvis_llm::LlmProvider> = Arc::from(create_provider(
        config.llm.enabled,
        &config.llm.endpoint,
    ));

    let state = Arc::new(AppState::new(vault.clone(), config.clone(), paths.clone(), llm));

    {
        let mut observer = state.observer.lock().await;
        observer.start()?;
        observer.prune()?;
    }

    if config.http.enabled {
        let http_state = state.clone();
        let port = config.http.port;
        let tray_dir = resolve_tray_dir(&paths);
        tokio::spawn(async move {
            if let Err(e) = http::serve(http_state, port, tray_dir).await {
                tracing::error!("http server error: {e}");
            }
        });
    }

    if config.scheduler.enabled {
        let sched_state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = scheduler::start_scheduler(sched_state).await {
                tracing::error!("scheduler error: {e}");
            }
        });
    }

    if config.messaging.telegram_token.is_some() || config.messaging.discord_token.is_some() {
        messaging::start_messaging_bots(state.clone()).await?;
    }

    if config.sync.enabled {
        if let Some(sync_dir) = &config.sync.sync_dir {
            tracing::info!("vault sync enabled: {sync_dir}");
        }
    }

    tracing::info!("jarvisd running; vault at {}", paths.root.display());
    ipc::serve_ipc(state, &paths.socket()).await?;
    Ok(())
}
