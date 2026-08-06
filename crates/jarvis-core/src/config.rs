use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{JarvisError, Result};
use crate::types::TrustLevel;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub vault_dir: Option<String>,
    #[serde(default)]
    pub network_enabled: bool,
    #[serde(default)]
    pub llm: LlmConfig,
    #[serde(default)]
    pub observer: ObserverConfig,
    #[serde(default)]
    pub scheduler: SchedulerConfig,
    #[serde(default)]
    pub sync: SyncConfig,
    #[serde(default)]
    pub http: HttpConfig,
    #[serde(default)]
    pub messaging: MessagingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    #[serde(default = "default_ollama_url")]
    pub endpoint: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default)]
    pub enabled: bool,
}

fn default_ollama_url() -> String {
    "http://127.0.0.1:11434".to_string()
}

fn default_model() -> String {
    "llama3.2:3b".to_string()
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            endpoint: default_ollama_url(),
            model: default_model(),
            enabled: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObserverConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_kill_switch")]
    pub kill_switch_file: String,
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub terminal: TerminalObserverConfig,
    #[serde(default)]
    pub filesystem: FilesystemObserverConfig,
    #[serde(default)]
    pub screen: ScreenObserverConfig,
    #[serde(default = "default_retention")]
    pub retention_days: u32,
}

fn default_true() -> bool {
    true
}

fn default_kill_switch() -> String {
    "~/.jarvis/STOP".to_string()
}

fn default_retention() -> u32 {
    30
}

impl Default for ObserverConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            kill_switch_file: default_kill_switch(),
            paused: false,
            terminal: TerminalObserverConfig::default(),
            filesystem: FilesystemObserverConfig::default(),
            screen: ScreenObserverConfig::default(),
            retention_days: default_retention(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalObserverConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_shells")]
    pub shells: Vec<String>,
}

fn default_shells() -> Vec<String> {
    vec!["bash".into(), "zsh".into(), "fish".into()]
}

impl Default for TerminalObserverConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            shells: default_shells(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemObserverConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub watch: Vec<String>,
    #[serde(default = "default_ignore")]
    pub ignore: Vec<String>,
}

fn default_ignore() -> Vec<String> {
    vec![
        "**/.git/**".into(),
        "**/node_modules/**".into(),
        "**/target/**".into(),
    ]
}

impl Default for FilesystemObserverConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            watch: Vec::new(),
            ignore: default_ignore(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenObserverConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub apps: Vec<String>,
}

impl Default for ScreenObserverConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            apps: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub sync_dir: Option<String>,
    #[serde(default)]
    pub passphrase: Option<String>,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            sync_dir: None,
            passphrase: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfig {
    #[serde(default = "default_http_port")]
    pub port: u16,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_http_port() -> u16 {
    9477
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            port: default_http_port(),
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagingConfig {
    #[serde(default)]
    pub telegram_token: Option<String>,
    #[serde(default)]
    pub telegram_allowed_users: Vec<i64>,
    #[serde(default)]
    pub discord_token: Option<String>,
}

impl Default for MessagingConfig {
    fn default() -> Self {
        Self {
            telegram_token: None,
            telegram_allowed_users: Vec::new(),
            discord_token: None,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            vault_dir: None,
            network_enabled: false,
            llm: LlmConfig::default(),
            observer: ObserverConfig::default(),
            scheduler: SchedulerConfig::default(),
            sync: SyncConfig::default(),
            http: HttpConfig::default(),
            messaging: MessagingConfig::default(),
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)
            .map_err(|e| JarvisError::Config(format!("read {}: {e}", path.display())))?;
        toml::from_str(&content)
            .map_err(|e| JarvisError::Config(format!("parse {}: {e}", path.display())))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)
            .map_err(|e| JarvisError::Config(format!("serialize config: {e}")))?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn default_trust(&self) -> TrustLevel {
        TrustLevel::Confirm
    }
}
