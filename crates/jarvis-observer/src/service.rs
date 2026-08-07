use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use jarvis_core::paths::expand_tilde;
use jarvis_core::{Config, Vault};

use crate::calendar::CalendarWatcher;
use crate::filesystem::FilesystemWatcher;

pub struct ObserverService {
    vault: Arc<Vault>,
    config: Config,
    filesystem: Option<FilesystemWatcher>,
    calendar: Option<CalendarWatcher>,
    kill_cached: AtomicBool,
    kill_checked_at: std::sync::Mutex<Instant>,
}

impl ObserverService {
    pub fn new(vault: Arc<Vault>, config: Config) -> Self {
        Self {
            vault,
            config,
            filesystem: None,
            calendar: None,
            kill_cached: AtomicBool::new(false),
            kill_checked_at: std::sync::Mutex::new(Instant::now() - Duration::from_secs(60)),
        }
    }

    pub fn is_paused(&self) -> bool {
        self.config.observer.paused
    }

    pub fn is_killed(&self) -> bool {
        let mut checked = self.kill_checked_at.lock().unwrap();
        if checked.elapsed() < Duration::from_secs(2) {
            return self.kill_cached.load(Ordering::Relaxed);
        }
        let kill_path = expand_tilde(&self.config.observer.kill_switch_file);
        let killed = kill_path.exists();
        self.kill_cached.store(killed, Ordering::Relaxed);
        *checked = Instant::now();
        killed
    }

    pub fn set_paused(&mut self, paused: bool) -> jarvis_core::Result<()> {
        self.config.observer.paused = paused;
        if paused {
            self.filesystem = None;
            self.calendar = None;
        } else if self.config.observer.enabled && !self.is_killed() {
            self.start()?;
        }
        Ok(())
    }

    pub fn start(&mut self) -> jarvis_core::Result<()> {
        if !self.config.observer.enabled || self.is_paused() || self.is_killed() {
            return Ok(());
        }
        if self.config.observer.filesystem.enabled
            && !self.config.observer.filesystem.watch.is_empty()
        {
            self.filesystem = Some(FilesystemWatcher::start(
                self.vault.clone(),
                &self.config.observer.filesystem,
            )?);
        }
        if self.config.observer.calendar.enabled && !self.config.observer.calendar.ics_paths.is_empty() {
            self.calendar = Some(CalendarWatcher::start(
                self.vault.clone(),
                self.config.observer.calendar.ics_paths.clone(),
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
            "calendar": self.config.observer.calendar.enabled,
            "calendar_paths": self.config.observer.calendar.ics_paths,
        })
    }

    pub fn prune(&self) -> jarvis_core::Result<usize> {
        self.vault
            .prune_events(self.config.observer.retention_days)
    }
}
