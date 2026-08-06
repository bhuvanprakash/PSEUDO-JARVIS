use jarvis_core::types::ObservedEvent;
use jarvis_core::Vault;
use serde_json::json;

pub fn record_terminal_event(
    vault: &Vault,
    command: &str,
    cwd: &str,
    session_id: Option<&str>,
    exit_code: Option<i32>,
) -> jarvis_core::Result<i64> {
    let event = ObservedEvent {
        ts: chrono::Utc::now().to_rfc3339(),
        source: "terminal".into(),
        kind: "command".into(),
        payload: json!({
            "command": command,
            "cwd": cwd,
            "exit_code": exit_code,
        }),
        session_id: session_id.map(str::to_string),
        redacted: false,
    };
    vault.insert_event(&event)
}

pub fn record_filesystem_event(
    vault: &Vault,
    kind: &str,
    path: &str,
) -> jarvis_core::Result<i64> {
    let event = ObservedEvent {
        ts: chrono::Utc::now().to_rfc3339(),
        source: "filesystem".into(),
        kind: kind.into(),
        payload: json!({ "path": path }),
        session_id: None,
        redacted: false,
    };
    vault.insert_event(&event)
}
