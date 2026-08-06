use std::sync::Arc;

use jarvis_core::paths::expand_tilde;
use jarvis_core::{Config, Vault};

use crate::filesystem::FilesystemWatcher;

pub struct ObserverService {
  vault: Arc<Vault>,
  config: Config,
  filesystem: Option<FilesystemWatcher>,
}

impl ObserverService {
    pub fn new(vault: Arc<Vault>, config: Config) -> Self {
        Self {
            vault,
            config,
            filesystem: None,
        }
    }

    pub fn is_paused(&self) -> bool {
        self.config.observer.paused
    }

    pub fn is_killed(&self) -> bool {
        let kill_path = expand_tilde(&self.config.observer.kill_switch_file);
        kill_path.exists()
    }

    pub fn start(&mut self) -> jarvis_core::Result<()> {
        if !self.config.observer.enabled || self.is_paused() || self.is_killed() {
            return Ok(());
        }
        if self.config.observer.filesystem.enabled {
            self.filesystem = Some(FilesystemWatcher::start(
                self.vault.clone(),
                &self.config.observer.filesystem,
            )?);
        }
        Ok(())
    }

    pub fn status(&self) -> serde_json::Value {
        serde_json::json!({
            "enabled": self.config.observer.enabled,
            "paused": self.config.observer.paused,
            "killed": self.is_killed(),
            "terminal": self.config.observer.terminal.enabled,
            "filesystem": self.config.observer.filesystem.enabled,
            "watch_paths": self.config.observer.filesystem.watch,
            "screen": self.config.observer.screen.enabled,
        })
    }

    pub fn prune(&self) -> jarvis_core::Result<usize> {
        self.vault
            .prune_events(self.config.observer.retention_days)
    }
}
