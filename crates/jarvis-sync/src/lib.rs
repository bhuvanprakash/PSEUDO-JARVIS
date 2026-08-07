use std::fs;
use std::path::{Path, PathBuf};

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use jarvis_core::paths::VaultPaths;
use rand::RngCore;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

pub struct VaultSync {
    paths: VaultPaths,
    sync_dir: PathBuf,
    passphrase: Option<String>,
}

impl VaultSync {
    pub fn new(
        paths: VaultPaths,
        sync_dir: impl Into<PathBuf>,
        passphrase: Option<String>,
    ) -> Self {
        Self {
            paths,
            sync_dir: sync_dir.into(),
            passphrase,
        }
    }

    pub fn export_snapshot(&self) -> jarvis_core::Result<PathBuf> {
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

        let checksum = checksum_dir(&staging)?;
        let manifest = serde_json::json!({
            "exported_at": chrono::Utc::now().to_rfc3339(),
            "checksum": checksum,
            "encrypted": self.passphrase.is_some(),
            "version": 1,
        });
        fs::write(staging.join("manifest.json"), serde_json::to_string_pretty(&manifest)?)?;

        fs::create_dir_all(&self.sync_dir)?;
        let dest = self.sync_dir.join(format!(
            "jarvis-sync-{}",
            chrono::Utc::now().format("%Y%m%d%H%M%S")
        ));

        if let Some(pass) = &self.passphrase {
            encrypt_dir(&staging, &dest, pass)?;
        } else {
            copy_recursive(&staging, &dest)?;
        }
        Ok(dest)
    }

    pub fn import_snapshot(&self, archive: &Path) -> jarvis_core::Result<()> {
        let staging = self.paths.sync_staging();
        if staging.exists() {
            fs::remove_dir_all(&staging)?;
        }
        fs::create_dir_all(&staging)?;

        if archive.join("manifest.json").exists() {
            copy_recursive(archive, &staging)?;
        } else if let Some(pass) = &self.passphrase {
            decrypt_dir(archive, &staging, pass)?;
        } else {
            copy_recursive(archive, &staging)?;
        }

        let manifest_path = staging.join("manifest.json");
        if manifest_path.exists() {
            let manifest: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(&manifest_path)?)?;
            if let Some(expected) = manifest.get("checksum").and_then(|v| v.as_str()) {
                let actual = checksum_dir(&staging)?;
                if actual != expected {
                    return Err(jarvis_core::error::JarvisError::Vault(
                        "sync checksum mismatch — archive may be corrupted".into(),
                    ));
                }
            }
        }

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

fn derive_key(passphrase: &str) -> [u8; 32] {
    let hash = Sha256::digest(passphrase.as_bytes());
    let mut key = [0u8; 32];
    key.copy_from_slice(&hash);
    key
}

fn encrypt_dir(src: &Path, dest: &Path, passphrase: &str) -> jarvis_core::Result<()> {
    fs::create_dir_all(dest)?;
    let key = derive_key(passphrase);
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| jarvis_core::error::JarvisError::Vault(e.to_string()))?;

    for entry in WalkDir::new(src).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry.path().strip_prefix(src).unwrap();
        let out_path = dest.join(rel);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let plaintext = fs::read(entry.path())?;
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_ref())
            .map_err(|e| jarvis_core::error::JarvisError::Vault(e.to_string()))?;
        let mut blob = Vec::with_capacity(12 + ciphertext.len());
        blob.extend_from_slice(&nonce_bytes);
        blob.extend_from_slice(&ciphertext);
        fs::write(out_path.with_extension("enc"), blob)?;
    }
    fs::write(
        dest.join("manifest.json"),
        serde_json::json!({"encrypted": true, "version": 1}).to_string(),
    )?;
    Ok(())
}

fn decrypt_dir(src: &Path, dest: &Path, passphrase: &str) -> jarvis_core::Result<()> {
    fs::create_dir_all(dest)?;
    let key = derive_key(passphrase);
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| jarvis_core::error::JarvisError::Vault(e.to_string()))?;

    for entry in WalkDir::new(src).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("enc") {
            continue;
        }
        let rel = path.strip_prefix(src).unwrap();
        let rel_str = rel.to_string_lossy().trim_end_matches(".enc").to_string();
        let out_path = dest.join(rel_str);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let blob = fs::read(path)?;
        if blob.len() < 12 {
            continue;
        }
        let (nonce_bytes, ciphertext) = blob.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| jarvis_core::error::JarvisError::Vault(e.to_string()))?;
        fs::write(out_path, plaintext)?;
    }
    Ok(())
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
    let mut files: Vec<_> = WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .collect();
    files.sort();
    for file in files {
        if file.file_name().and_then(|n| n.to_str()) == Some("manifest.json") {
            continue;
        }
        hasher.update(fs::read(file)?);
    }
    Ok(hex::encode(hasher.finalize()))
}
