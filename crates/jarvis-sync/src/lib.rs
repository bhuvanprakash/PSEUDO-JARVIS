use std::fs;
use std::path::Path;

use jarvis_core::paths::VaultPaths;
use sha2::{Digest, Sha256};

pub struct VaultSync {
    paths: VaultPaths,
    sync_dir: std::path::PathBuf,
}

impl VaultSync {
    pub fn new(paths: VaultPaths, sync_dir: impl Into<std::path::PathBuf>) -> Self {
        Self {
            paths,
            sync_dir: sync_dir.into(),
        }
    }

    pub fn export_snapshot(&self) -> jarvis_core::Result<std::path::PathBuf> {
        let staging = self.paths.sync_staging();
        if staging.exists() {
            fs::remove_dir_all(&staging)?;
        }
        fs::create_dir_all(&staging)?;

        for item in ["vault.db", "config.toml", "playbooks"] {
            let src = self.paths.root.join(item);
            if src.exists() {
                copy_recursive(&src, &staging.join(item))?;
            }
        }

        let manifest = serde_json::json!({
            "exported_at": chrono::Utc::now().to_rfc3339(),
            "checksum": checksum_dir(&staging)?,
        });
        fs::write(staging.join("manifest.json"), serde_json::to_string_pretty(&manifest)?)?;

        fs::create_dir_all(&self.sync_dir)?;
        let dest = self
            .sync_dir
            .join(format!("jarvis-sync-{}.tar", chrono::Utc::now().format("%Y%m%d%H%M%S")));
        archive_dir(&staging, &dest)?;
        Ok(dest)
    }

    pub fn import_snapshot(&self, archive: &Path) -> jarvis_core::Result<()> {
        let staging = self.paths.sync_staging();
        if staging.exists() {
            fs::remove_dir_all(&staging)?;
        }
        fs::create_dir_all(&staging)?;
        extract_archive(archive, &staging)?;

        for item in ["vault.db", "config.toml", "playbooks"] {
            let src = staging.join(item);
            if src.exists() {
                let dest = self.paths.root.join(item);
                if dest.exists() {
                    if dest.is_dir() {
                        fs::remove_dir_all(&dest)?;
                    } else {
                        fs::remove_file(&dest)?;
                    }
                }
                copy_recursive(&src, &dest)?;
            }
        }
        Ok(())
    }
}

fn copy_recursive(src: &Path, dest: &Path) -> jarvis_core::Result<()> {
    if src.is_dir() {
        fs::create_dir_all(dest)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &dest.join(entry.file_name()))?;
        }
    } else {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src, dest)?;
    }
    Ok(())
}

fn checksum_dir(dir: &Path) -> jarvis_core::Result<String> {
    let mut hasher = Sha256::new();
    let mut files: Vec<_> = walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .collect();
    files.sort();
    for file in files {
        let content = fs::read(file)?;
        hasher.update(&content);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn archive_dir(src: &Path, dest: &Path) -> jarvis_core::Result<()> {
    // Simple copy of staging to a directory archive (portable without tar crate)
    copy_recursive(src, dest)?;
    Ok(())
}

fn extract_archive(src: &Path, dest: &Path) -> jarvis_core::Result<()> {
    copy_recursive(src, dest)?;
    Ok(())
}
