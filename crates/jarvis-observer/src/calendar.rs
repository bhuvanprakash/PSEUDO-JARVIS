use std::fs;
use std::path::Path;
use std::sync::Arc;

use jarvis_core::types::ObservedEvent;
use jarvis_core::Vault;
use serde_json::json;

/// Watch calendar `.ics` files for upcoming events matching playbook triggers.
pub struct CalendarWatcher {
    _handle: tokio::task::JoinHandle<()>,
}

impl CalendarWatcher {
    pub fn start(vault: Arc<Vault>, ics_paths: Vec<String>) -> jarvis_core::Result<Self> {
        let handle = tokio::spawn(async move {
            loop {
                for path_str in &ics_paths {
                    let path = jarvis_core::paths::resolve_path(path_str);
                    if path.exists() {
                        if let Ok(content) = fs::read_to_string(&path) {
                            scan_ics(&vault, &content);
                        }
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            }
        });
        Ok(Self { _handle: handle })
    }
}

fn scan_ics(vault: &Vault, content: &str) {
    for line in content.lines() {
        if let Some(summary) = line.strip_prefix("SUMMARY:") {
            let event = ObservedEvent {
                ts: chrono::Utc::now().to_rfc3339(),
                source: "calendar".into(),
                kind: "event".into(),
                payload: json!({ "title": summary.trim() }),
                session_id: None,
                redacted: false,
            };
            let _ = vault.insert_event(&event);
        }
    }
}

pub fn list_upcoming_titles(ics_path: &Path, hours: i64) -> jarvis_core::Result<Vec<String>> {
    let content = fs::read_to_string(ics_path)?;
    let mut titles = Vec::new();
    let cutoff = chrono::Utc::now() + chrono::Duration::hours(hours);
    let mut current_title: Option<String> = None;
    for line in content.lines() {
        if let Some(t) = line.strip_prefix("SUMMARY:") {
            current_title = Some(t.trim().to_string());
        }
        if let Some(dt) = line.strip_prefix("DTSTART:") {
            if let Some(title) = current_title.take() {
                if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(dt.trim()) {
                    if parsed.with_timezone(&chrono::Utc) <= cutoff {
                        titles.push(title);
                    }
                } else if dt.len() >= 8 {
                    titles.push(title);
                }
            }
        }
    }
    Ok(titles)
}
