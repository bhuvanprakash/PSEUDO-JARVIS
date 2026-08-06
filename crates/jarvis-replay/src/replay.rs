use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use jarvis_core::VaultPaths;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayManifest {
    pub run_id: String,
    pub playbook: PlaybookRef,
    pub trigger: String,
    pub phrase: Option<String>,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub steps_total: usize,
    pub steps_failed: usize,
    pub trust_prompts: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookRef {
    pub id: String,
    pub version: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayStepLog {
    pub step_id: String,
    pub trust_level: String,
    pub status: String,
    pub input: Option<serde_json::Value>,
    pub output: Option<serde_json::Value>,
    pub started_at: String,
    pub finished_at: String,
}

pub struct ReplayRecorder {
    dir: std::path::PathBuf,
    steps_path: std::path::PathBuf,
    manifest: ReplayManifest,
}

impl ReplayRecorder {
    pub fn new(
        paths: &VaultPaths,
        run_id: &str,
        playbook_id: &str,
        version: i64,
        trigger: &str,
        phrase: Option<&str>,
        steps_total: usize,
    ) -> jarvis_core::Result<Self> {
        let dir = paths.run_replay_dir(run_id);
        fs::create_dir_all(&dir)?;
        let manifest = ReplayManifest {
            run_id: run_id.to_string(),
            playbook: PlaybookRef {
                id: playbook_id.to_string(),
                version,
            },
            trigger: trigger.to_string(),
            phrase: phrase.map(str::to_string),
            status: "running".into(),
            started_at: chrono::Utc::now().to_rfc3339(),
            finished_at: None,
            steps_total,
            steps_failed: 0,
            trust_prompts: 0,
        };
        let steps_path = dir.join("steps.jsonl");
        OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&steps_path)?;
        Ok(Self {
            dir,
            steps_path,
            manifest,
        })
    }

    pub fn record_step(&mut self, log: &ReplayStepLog) -> jarvis_core::Result<()> {
        if log.status == "failed" {
            self.manifest.steps_failed += 1;
        }
        if log.status == "awaiting_confirm" {
            self.manifest.trust_prompts += 1;
        }
        let mut file = OpenOptions::new().append(true).open(&self.steps_path)?;
        writeln!(file, "{}", serde_json::to_string(log)?)?;
        Ok(())
    }

    pub fn finish(&mut self, status: &str) -> jarvis_core::Result<PathBuf> {
        self.manifest.status = status.to_string();
        self.manifest.finished_at = Some(chrono::Utc::now().to_rfc3339());
        let manifest_path = self.dir.join("manifest.json");
        fs::write(&manifest_path, serde_json::to_string_pretty(&self.manifest)?)?;
        Ok(self.dir.clone())
    }

    pub fn write_files_diff(&self, diff: &str) -> jarvis_core::Result<()> {
        fs::write(self.dir.join("files.diff"), diff)?;
        Ok(())
    }

    pub fn replay_dir(&self) -> &Path {
        &self.dir
    }
}

use std::path::PathBuf;

pub fn load_manifest(replay_dir: &Path) -> jarvis_core::Result<ReplayManifest> {
    let content = fs::read_to_string(replay_dir.join("manifest.json"))?;
    Ok(serde_json::from_str(&content)?)
}

pub fn load_steps(replay_dir: &Path) -> jarvis_core::Result<Vec<ReplayStepLog>> {
    let path = replay_dir.join("steps.jsonl");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path)?;
    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| Ok(serde_json::from_str(line)?))
        .collect()
}

pub fn export_markdown(replay_dir: &Path) -> jarvis_core::Result<String> {
    let manifest = load_manifest(replay_dir)?;
    let steps = load_steps(replay_dir)?;
    let mut md = format!(
        "# Replay {}\n\n- Playbook: {} v{}\n- Trigger: {}\n- Status: {}\n- Started: {}\n\n## Steps\n\n",
        manifest.run_id,
        manifest.playbook.id,
        manifest.playbook.version,
        manifest.trigger,
        manifest.status,
        manifest.started_at,
    );
    for step in steps {
        md.push_str(&format!(
            "### {}\n- Trust: {}\n- Status: {}\n\n",
            step.step_id, step.trust_level, step.status
        ));
    }
    Ok(md)
}

pub fn prune_replays(paths: &VaultPaths, older_than_days: u32) -> jarvis_core::Result<usize> {
    let cutoff = chrono::Utc::now() - chrono::Duration::days(older_than_days as i64);
    let mut removed = 0;
    if !paths.replays().exists() {
        return Ok(0);
    }
    for entry in fs::read_dir(paths.replays())? {
        let entry = entry?;
        let manifest_path = entry.path().join("manifest.json");
        if manifest_path.exists() {
            if let Ok(manifest) = load_manifest(&entry.path()) {
                if let Ok(started) = chrono::DateTime::parse_from_rfc3339(&manifest.started_at) {
                    if started < cutoff {
                        fs::remove_dir_all(entry.path())?;
                        removed += 1;
                    }
                }
            }
        }
    }
    Ok(removed)
}
