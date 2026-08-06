use std::sync::Arc;

use tracing::{error, info};

use crate::state::AppState;

pub async fn start_telegram_bot(state: Arc<AppState>) -> jarvis_core::Result<()> {
    let token = match &state.config.messaging.telegram_token {
        Some(t) if !t.is_empty() => t.clone(),
        _ => return Ok(()),
    };
    let allowed = state.config.messaging.telegram_allowed_users.clone();
    info!("Telegram bot enabled");

    tokio::spawn(async move {
        let client = reqwest::Client::new();
        let mut offset: i64 = 0;
        loop {
            let url = format!(
                "https://api.telegram.org/bot{token}/getUpdates?offset={offset}&timeout=30"
            );
            match client.get(&url).send().await {
                Ok(resp) => {
                    if let Ok(json) = resp.json::<serde_json::Value>().await {
                        if let Some(results) = json.get("result").and_then(|r| r.as_array()) {
                            for update in results {
                                if let Some(id) = update.get("update_id").and_then(|v| v.as_i64()) {
                                    offset = id + 1;
                                }
                                if let Some(msg) = update.get("message") {
                                    let user_id = msg
                                        .pointer("/from/id")
                                        .and_then(|v| v.as_i64())
                                        .unwrap_or(0);
                                    if !allowed.is_empty() && !allowed.contains(&user_id) {
                                        continue;
                                    }
                                    let text = msg
                                        .get("text")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    if text.starts_with("/run ") {
                                        let phrase = text.trim_start_matches("/run ").trim().to_string();
                                        let state = state.clone();
                                        let token = token.clone();
                                        let chat_id = msg
                                            .get("chat")
                                            .and_then(|c| c.get("id"))
                                            .and_then(|v| v.as_i64())
                                            .unwrap_or(0);
                                        tokio::spawn(async move {
                                            let result = crate::orchestrator::Orchestrator::run(
                                                &state,
                                                crate::orchestrator::RunOptions {
                                                    playbook_id: None,
                                                    phrase: Some(phrase),
                                                    trigger: "messaging".into(),
                                                    schedule_triggered: false,
                                                    auto_approve: true,
                                                },
                                                None,
                                            )
                                            .await;
                                            let reply = match result {
                                                Ok(id) => format!("Run completed: {id}"),
                                                Err(e) => format!("Run failed: {e}"),
                                            };
                                            let _ = send_telegram(&token, chat_id, &reply).await;
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => error!("telegram poll error: {e}"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    });
    Ok(())
}

async fn send_telegram(token: &str, chat_id: i64, text: &str) -> jarvis_core::Result<()> {
    let client = reqwest::Client::new();
    let url = format!("https://api.telegram.org/bot{token}/sendMessage");
    let _ = client
        .post(&url)
        .json(&serde_json::json!({
            "chat_id": chat_id,
            "text": text,
        }))
        .send()
        .await
        .map_err(|e| jarvis_core::error::JarvisError::Other(e.to_string()))?;
    Ok(())
}
