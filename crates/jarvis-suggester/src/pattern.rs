use std::collections::HashMap;

use jarvis_core::types::PatternRecord;
use jarvis_core::Vault;
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub struct PatternMiner;

impl PatternMiner {
    pub fn mine(vault: &Vault, days: i64) -> jarvis_core::Result<Vec<PatternRecord>> {
        let since = (chrono::Utc::now() - chrono::Duration::days(days))
            .to_rfc3339();
        let events = vault.list_events_since(&since, 10_000)?;
        let mut sequences: HashMap<String, Vec<(i64, String)>> = HashMap::new();

        for (id, event) in events {
            if event.source == "terminal" && event.kind == "command" {
                if let Some(cmd) = event.payload.get("command").and_then(|v| v.as_str()) {
                    if let Some(cwd) = event.payload.get("cwd").and_then(|v| v.as_str()) {
                        let normalized = normalize_command(cmd);
                        let key = format!("{cwd}|{normalized}");
                        sequences.entry(key).or_default().push((id, normalized));
                    }
                }
            }
        }

        let mut patterns = Vec::new();
        let mut grouped: HashMap<String, Vec<(i64, String)>> = HashMap::new();
        for (key, items) in sequences {
            if items.len() >= 3 {
                let cwd = key.split('|').next().unwrap_or("").to_string();
                grouped.entry(cwd).or_default().extend(items);
            }
        }

        for (cwd, items) in grouped {
            let commands: Vec<String> = items.iter().map(|(_, c)| c.clone()).collect();
            let event_ids: Vec<i64> = items.iter().map(|(id, _)| *id).collect();
            let fingerprint = fingerprint(&commands);
            let confidence = (items.len() as f64 / 10.0).min(1.0);
            let suggested_name = suggest_name(&cwd, &commands);
            patterns.push(PatternRecord {
                id: Uuid::new_v4().to_string(),
                fingerprint,
                event_ids,
                suggested_name: Some(suggested_name),
                confidence,
                status: "pending".into(),
                command_sequence: dedupe_sequence(&commands),
            });
        }

        Ok(patterns)
    }
}

fn normalize_command(cmd: &str) -> String {
  let cmd = cmd.trim();
  if cmd.starts_with("git ") {
    let parts: Vec<&str> = cmd.split_whitespace().take(3).collect();
    parts.join(" ")
  } else {
    cmd.to_string()
  }
}

fn fingerprint(commands: &[String]) -> String {
    let joined = commands.join(";");
    let hash = Sha256::digest(joined.as_bytes());
    hex::encode(hash)
}

fn suggest_name(cwd: &str, commands: &[String]) -> String {
    let base = std::path::Path::new(cwd)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("workflow");
    if commands.iter().any(|c| c.contains("git")) {
        format!("{base}-git-routine")
    } else {
        format!("{base}-routine")
    }
}

fn dedupe_sequence(commands: &[String]) -> Vec<String> {
    let mut seen = Vec::new();
    for cmd in commands {
        if seen.last() != Some(cmd) {
            seen.push(cmd.clone());
        }
    }
    seen.truncate(20);
    seen
}
