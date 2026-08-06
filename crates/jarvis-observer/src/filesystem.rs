use std::path::Path;
use std::sync::Arc;

use jarvis_core::paths::{expand_tilde, resolve_path};
use jarvis_core::Config;
use jarvis_core::Vault;
use notify::{Config as NotifyConfig, Event, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

use crate::events::record_filesystem_event;

pub struct FilesystemWatcher {
    _watcher: RecommendedWatcher,
}

impl FilesystemWatcher {
    pub fn start(
        vault: Arc<Vault>,
        config: &jarvis_core::config::FilesystemObserverConfig,
    ) -> jarvis_core::Result<Self> {
        let (tx, mut rx) = mpsc::channel(256);
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    let _ = tx.blocking_send(event);
                }
            },
            NotifyConfig::default(),
        )
        .map_err(|e| jarvis_core::error::JarvisError::Observer(e.to_string()))?;

        for watch_path in &config.watch {
            let path = resolve_path(watch_path);
            if path.exists() {
                watcher
                    .watch(&path, RecursiveMode::Recursive)
                    .map_err(|e| jarvis_core::error::JarvisError::Observer(e.to_string()))?;
            }
        }

        let ignore = config.ignore.clone();
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                for path in event.paths {
                    let path_str = path.display().to_string();
                    if should_ignore(&path_str, &ignore) {
                        continue;
                    }
                    let kind = match event.kind {
                        notify::EventKind::Create(_) => "file_create",
                        notify::EventKind::Modify(_) => "file_modify",
                        notify::EventKind::Remove(_) => "file_remove",
                        _ => "file_event",
                    };
                    let _ = record_filesystem_event(&vault, kind, &path_str);
                }
            }
        });

        Ok(Self { _watcher: watcher })
    }
}

fn should_ignore(path: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|pattern| {
        if pattern.contains("**") {
            path.contains(".git") || path.contains("node_modules") || path.contains("/target/")
        } else {
            path.contains(pattern)
        }
    })
}

pub fn install_shell_hook(vault_root: &Path, shell: &str) -> jarvis_core::Result<String> {
    let hook_script = vault_root.join("bin/jarvis-hook");
    if let Some(parent) = hook_script.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = format!(
        r#"#!/bin/sh
jarvis observe event --command "$1" --cwd "$PWD" 2>/dev/null || true
"#
    );
    std::fs::write(&hook_script, content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook_script, std::fs::Permissions::from_mode(0o755))?;
    }

    let snippet = match shell {
        "zsh" => format!(
            r#"
# PSEUDO-JARVIS observer
_jarvis_preexec() {{
  {hook} "$1"
}}
preexec_functions+=(_jarvis_preexec)
"#,
            hook = hook_script.display()
        ),
        "fish" => format!(
            r#"
# PSEUDO-JARVIS observer
function __jarvis_preexec --on-event fish_preexec
  {hook} $argv[1]
end
"#,
            hook = hook_script.display()
        ),
        _ => format!(
            r#"
# PSEUDO-JARVIS observer
_jarvis_preexec() {{
  {hook} "$1"
}}
trap '_jarvis_preexec "$BASH_COMMAND"' DEBUG
"#,
            hook = hook_script.display()
        ),
    };
    Ok(snippet)
}
