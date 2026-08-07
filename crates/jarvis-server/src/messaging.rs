use std::sync::Arc;

use tracing::{error, info};

use crate::state::AppState;

pub async fn start_messaging_bots(state: Arc<AppState>) -> jarvis_core::Result<()> {
    start_telegram_bot(state.clone()).await?;
    start_discord_bot(state).await?;
    Ok(())
}

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
                                    if let Some(phrase) = extract_run_phrase(&text, &["/run "]) {
                                        let state = state.clone();
                                        let token = token.clone();
                                        let chat_id = msg
                                            .get("chat")
                                            .and_then(|c| c.get("id"))
                                            .and_then(|v| v.as_i64())
                                            .unwrap_or(0);
                                        tokio::spawn(async move {
                                            let reply = run_phrase(&state, phrase).await;
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

pub async fn start_discord_bot(state: Arc<AppState>) -> jarvis_core::Result<()> {
    let token = match &state.config.messaging.discord_token {
        Some(t) if !t.is_empty() => t.clone(),
        _ => return Ok(()),
    };
    info!("Discord bot enabled");

    tokio::spawn(async move {
        let client = reqwest::Client::new();
        let mut last_message_id: Option<String> = None;
        loop {
            let mut url = "https://discord.com/api/v10/users/@me/channels".to_string();
            match client
                .get(&url)
                .header("Authorization", format!("Bot {token}"))
                .send()
                .await
            {
                Ok(_) => {}
                Err(e) => {
                    error!("discord channel list error: {e}");
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    continue;
                }
            }

            // Poll gateway via REST: fetch recent DMs the bot can see
            url = "https://discord.com/api/v10/users/@me/channels".to_string();
            let channels = match client
                .get(&url)
                .header("Authorization", format!("Bot {token}"))
                .send()
                .await
            {
                Ok(resp) => resp.json::<Vec<serde_json::Value>>().await.unwrap_or_default(),
                Err(e) => {
                    error!("discord poll error: {e}");
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    continue;
                }
            };

            for channel in channels {
                let channel_id = channel
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let messages_url = format!(
                    "https://discord.com/api/v10/channels/{channel_id}/messages?limit=5"
                );
                let messages = match client
                    .get(&messages_url)
                    .header("Authorization", format!("Bot {token}"))
                    .send()
                    .await
                {
                    Ok(resp) => resp.json::<Vec<serde_json::Value>>().await.unwrap_or_default(),
                    Err(_) => continue,
                };

                for msg in messages.iter().rev() {
                    let msg_id = msg.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    if last_message_id.as_deref() == Some(msg_id) {
                        continue;
                    }
                    last_message_id = Some(msg_id.to_string());

                    let author_bot = msg
                        .pointer("/author/bot")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    if author_bot {
                        continue;
                    }

                    let text = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
                    if let Some(phrase) =
                        extract_run_phrase(text, &["!jarvis ", "/jarvis ", "!run "])
                    {
                        let state = state.clone();
                        let token = token.clone();
                        let reply = run_phrase(&state, phrase).await;
                        let _ = send_discord(&token, &channel_id, &reply).await;
                    }
                }
            }

            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }
    });
    Ok(())
}

fn extract_run_phrase(text: &str, prefixes: &[&str]) -> Option<String> {
    let trimmed = text.trim();
    for prefix in prefixes {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            let phrase = rest.trim();
            if !phrase.is_empty() {
                return Some(phrase.to_string());
            }
        }
    }
    None
}

async fn run_phrase(state: &AppState, phrase: String) -> String {
    let result = crate::orchestrator::Orchestrator::run(
        state,
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
    match result {
        Ok(id) => format!("Run completed: {id}"),
        Err(e) => format!("Run failed: {e}"),
    }
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

async fn send_discord(token: &str, channel_id: &str, text: &str) -> jarvis_core::Result<()> {
    let client = reqwest::Client::new();
    let url = format!("https://discord.com/api/v10/channels/{channel_id}/messages");
    let _ = client
        .post(&url)
        .header("Authorization", format!("Bot {token}"))
        .json(&serde_json::json!({ "content": text }))
        .send()
        .await
        .map_err(|e| jarvis_core::error::JarvisError::Other(e.to_string()))?;
    Ok(())
}
