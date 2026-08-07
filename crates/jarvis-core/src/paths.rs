use std::path::{Path, PathBuf};

use crate::error::{JarvisError, Result};

#[derive(Debug, Clone)]
pub struct VaultPaths {
    pub root: PathBuf,
}

impl VaultPaths {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn default_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".jarvis")
    }

    pub fn from_env_or_default(custom: Option<&str>) -> Self {
        let root = custom
            .map(PathBuf::from)
            .or_else(|| std::env::var("JARVIS_HOME").ok().map(PathBuf::from))
            .unwrap_or_else(Self::default_dir);
        Self::new(root)
    }

    pub fn config(&self) -> PathBuf {
        self.root.join("config.toml")
    }

    pub fn auth_token(&self) -> PathBuf {
        self.root.join("auth.token")
    }

    pub fn database(&self) -> PathBuf {
        self.root.join("vault.db")
    }

    pub fn playbooks(&self) -> PathBuf {
        self.root.join("playbooks")
    }

    pub fn community_playbooks(&self) -> PathBuf {
        self.playbooks().join("community")
    }

    pub fn backups(&self) -> PathBuf {
        self.root.join("backups")
    }

    pub fn replays(&self) -> PathBuf {
        self.root.join("replays")
    }

    pub fn observer(&self) -> PathBuf {
        self.root.join("observer")
    }

    pub fn secrets(&self) -> PathBuf {
        self.root.join("secrets")
    }

    pub fn plugins(&self) -> PathBuf {
        self.root.join("plugins")
    }

    pub fn socket(&self) -> PathBuf {
        self.root.join("jarvis.sock")
    }

    pub fn drafts(&self) -> PathBuf {
        self.root.join("drafts")
    }

    pub fn sync_staging(&self) -> PathBuf {
        self.root.join("sync-staging")
    }

    pub fn kill_switch(&self, config_path: &str) -> PathBuf {
        expand_tilde(config_path)
    }

    pub fn ensure_layout(&self) -> Result<()> {
        for dir in [
            &self.root,
            &self.playbooks(),
            &self.community_playbooks(),
            &self.backups(),
            &self.replays(),
            &self.observer(),
            &self.secrets(),
            &self.plugins(),
            &self.drafts(),
        ] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }

    pub fn run_backup_dir(&self, run_id: &str) -> PathBuf {
        self.backups().join(format!("run_{run_id}"))
    }

    pub fn run_replay_dir(&self, run_id: &str) -> PathBuf {
        self.replays().join(format!("run_{run_id}"))
    }
}

pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(rest)
    } else if path == "~" {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
    } else {
        PathBuf::from(path)
    }
}

pub fn resolve_path(path: &str) -> PathBuf {
    let expanded = expand_tilde(path);
    if expanded.is_absolute() {
        expanded
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(expanded)
    }
}

pub fn canonicalize_allow_paths(allowlist: &[String]) -> Vec<PathBuf> {
    allowlist
        .iter()
        .map(|allowed| {
            let path = resolve_path(allowed);
            path.canonicalize().unwrap_or(path)
        })
        .collect()
}

pub fn path_allowed_canonical(path: &Path, allowed: &[PathBuf]) -> bool {
    if allowed.is_empty() {
        return true;
    }
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    allowed.iter().any(|a| canonical.starts_with(a))
}

pub fn path_allowed(path: &Path, allowlist: &[String]) -> bool {
    path_allowed_canonical(path, &canonicalize_allow_paths(allowlist))
}

pub fn generate_auth_token() -> Result<String> {
    use sha2::{Digest, Sha256};
    let seed = format!(
        "{}{}",
        uuid::Uuid::new_v4(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    );
    let hash = Sha256::digest(seed.as_bytes());
    Ok(hex::encode(hash))
}

pub fn write_auth_token(path: &Path) -> Result<String> {
    let token = generate_auth_token()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, &token)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(token)
}

pub fn read_auth_token(path: &Path) -> Result<String> {
    if !path.exists() {
        return Err(JarvisError::Vault(
            "auth token missing; run `jarvis init`".into(),
        ));
    }
    let token = std::fs::read_to_string(path)?.trim().to_string();
    if token.is_empty() {
        return Err(JarvisError::Vault("auth token is empty".into()));
    }
    Ok(token)
}
