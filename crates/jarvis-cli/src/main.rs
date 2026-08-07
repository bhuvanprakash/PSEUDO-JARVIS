mod commands;

use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use jarvis_core::paths::VaultPaths;
use jarvis_core::{Config, Vault};
use jarvis_llm::ollama::create_provider;
use jarvis_server::AppState;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "jarvis",
    about = "PSEUDO-JARVIS — Ambient Playbooks local assistant",
    version
)]
struct Cli {
    #[arg(long, env = "JARVIS_HOME", global = true)]
    vault: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize vault and configuration
    Init {
        #[arg(long)]
        vault: Option<String>,
    },
    /// Run a playbook by id or natural-language phrase
    Run {
        playbook: Option<String>,
        #[arg(long)]
        phrase: Option<String>,
        #[arg(long)]
        yes: bool,
    },
    /// Playbook management
    Playbook {
        #[command(subcommand)]
        cmd: PlaybookCommands,
    },
    /// Observer controls
    Observe {
        #[command(subcommand)]
        cmd: ObserveCommands,
    },
    /// Replay audit logs
    Replay {
        #[command(subcommand)]
        cmd: ReplayCommands,
    },
    /// Pattern suggestions (Phase 2)
    Suggest {
        #[command(subcommand)]
        cmd: SuggestCommands,
    },
    /// Plugin management
    Plugin {
        #[command(subcommand)]
        cmd: PluginCommands,
    },
    /// Vault sync (Phase 5)
    Sync {
        #[command(subcommand)]
        cmd: SyncCommands,
    },
    /// Voice trigger (Phase 3) — text phrase shortcut
    Voice {
        phrase: String,
    },
    /// Daemon status via IPC
    Status,
    /// Start daemon in foreground (dev)
    Daemon,
}

#[derive(Subcommand)]
enum PlaybookCommands {
    List,
    Show { id: String },
    Validate { path: PathBuf },
    Import { path: PathBuf },
}

#[derive(Subcommand)]
enum ObserveCommands {
    Status,
    Enable {
        #[arg(long)]
        path: Vec<String>,
    },
    Pause,
    Resume,
    Event {
        #[arg(long)]
        command: String,
        #[arg(long, default_value = ".")]
        cwd: String,
    },
    Hook {
        #[arg(long, default_value = "bash")]
        shell: String,
    },
}

#[derive(Subcommand)]
enum ReplayCommands {
    List,
    Show { run_id: String },
    Export { run_id: String },
    Prune {
        #[arg(long, default_value = "30")]
        days: u32,
    },
}

#[derive(Subcommand)]
enum SuggestCommands {
    Scan {
        #[arg(long, default_value = "7")]
        days: i64,
    },
    List,
    Accept { id: String },
    Dismiss { id: String },
}

#[derive(Subcommand)]
enum PluginCommands {
    List,
    Enable { name: String },
}

#[derive(Subcommand)]
enum SyncCommands {
    Export,
    Import { archive: PathBuf },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("jarvis=warn".parse()?))
        .init();

    let cli = Cli::parse();
    match cli.command {
        Commands::Init { vault } => commands::init(vault.or(cli.vault)).await,
        Commands::Run { playbook, phrase, yes } => {
            commands::run(playbook, phrase, yes, cli.vault).await
        }
        Commands::Playbook { cmd } => commands::playbook(cmd, cli.vault).await,
        Commands::Observe { cmd } => commands::observe(cmd, cli.vault).await,
        Commands::Replay { cmd } => commands::replay(cmd, cli.vault).await,
        Commands::Suggest { cmd } => commands::suggest(cmd, cli.vault).await,
        Commands::Plugin { cmd } => commands::plugin(cmd, cli.vault).await,
        Commands::Sync { cmd } => commands::sync(cmd, cli.vault).await,
        Commands::Voice { phrase } => commands::run(None, Some(phrase), false, cli.vault).await,
        Commands::Status => commands::status(cli.vault).await,
        Commands::Daemon => commands::daemon(cli.vault).await,
    }
}

pub fn load_state(vault: Option<String>) -> anyhow::Result<(AppState, VaultPaths)> {
    let paths = VaultPaths::from_env_or_default(vault.as_deref());
    paths.ensure_layout()?;
    let config = Config::load(&paths.config())?;
    let vault = Arc::new(Vault::open(paths.clone())?);
    vault.sync_playbooks()?;
    let llm: Arc<dyn jarvis_llm::LlmProvider> =
        Arc::from(create_provider(config.llm.enabled, &config.llm.endpoint));
    Ok((AppState::new(vault, config, paths.clone(), llm), paths))
}
