use std::path::Path;
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub capabilities: Vec<String>,
    pub entrypoint: String,
}

pub fn load_manifest(plugin_dir: &Path) -> jarvis_core::Result<PluginManifest> {
    let content = std::fs::read_to_string(plugin_dir.join("plugin.yaml"))?;
    Ok(serde_yaml::from_str(&content)?)
}

pub async fn run_plugin(
    plugin_dir: &Path,
    input: &serde_json::Value,
) -> jarvis_core::Result<serde_json::Value> {
    let manifest = load_manifest(plugin_dir)?;
    let entry = plugin_dir.join(&manifest.entrypoint);
    let mut child = Command::new(&entry)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| jarvis_core::error::JarvisError::Execution(format!("plugin spawn: {e}")))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(serde_json::to_string(input)?.as_bytes())
            .await
            .map_err(|e| jarvis_core::error::JarvisError::Execution(format!("plugin stdin: {e}")))?;
    }

    let stdout = child
        .wait_with_output()
        .await
        .map_err(|e| jarvis_core::error::JarvisError::Execution(format!("plugin wait: {e}")))?;

    let output = String::from_utf8_lossy(&stdout.stdout);
    serde_json::from_str(&output).map_err(|e| {
        jarvis_core::error::JarvisError::Execution(format!("plugin output parse: {e}"))
    })
}

pub fn install_builtin_plugins(plugins_dir: &Path) -> jarvis_core::Result<()> {
    install_plugin_script(
        plugins_dir,
        "slack",
        "slack-plugin.sh",
        r#"#!/bin/sh
read INPUT
WEBHOOK=$(echo "$INPUT" | jq -r '.config.webhook // empty')
MESSAGE=$(echo "$INPUT" | jq -r '.config.message // "Hello from JARVIS"')
if [ -z "$WEBHOOK" ]; then echo '{"success":false,"stderr":"missing webhook"}'; exit 0; fi
curl -s -X POST -H 'Content-type: application/json' --data "{\"text\":\"$MESSAGE\"}" "$WEBHOOK" >/dev/null
echo '{"success":true,"stdout":"slack message sent"}'
"#,
        &["network:hooks.slack.com"],
    )?;
    install_plugin_script(
        plugins_dir,
        "homeassistant",
        "ha-plugin.sh",
        r#"#!/bin/sh
read INPUT
URL=$(echo "$INPUT" | jq -r '.config.url // empty')
TOKEN=$(echo "$INPUT" | jq -r '.config.token // empty')
SERVICE=$(echo "$INPUT" | jq -r '.config.service // "notify.notify"')
MESSAGE=$(echo "$INPUT" | jq -r '.config.message // "JARVIS notification"')
if [ -z "$URL" ] || [ -z "$TOKEN" ]; then echo '{"success":false,"stderr":"missing url or token"}'; exit 0; fi
curl -s -X POST "$URL/api/services/$SERVICE" -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d "{\"message\":\"$MESSAGE\"}" >/dev/null
echo '{"success":true,"stdout":"home assistant notified"}'
"#,
        &["network:homeassistant"],
    )?;
    install_plugin_script(
        plugins_dir,
        "notion",
        "notion-plugin.sh",
        r#"#!/bin/sh
read INPUT
TOKEN=$(echo "$INPUT" | jq -r '.config.token // empty')
PAGE=$(echo "$INPUT" | jq -r '.config.page_id // empty')
BODY=$(echo "$INPUT" | jq -r '.config.body // "JARVIS update"')
if [ -z "$TOKEN" ] || [ -z "$PAGE" ]; then echo '{"success":false,"stderr":"missing token or page_id"}'; exit 0; fi
curl -s -X PATCH "https://api.notion.com/v1/blocks/$PAGE/children" \
  -H "Authorization: Bearer $TOKEN" -H "Notion-Version: 2022-06-28" -H "Content-Type: application/json" \
  -d "{\"children\":[{\"object\":\"block\",\"type\":\"paragraph\",\"paragraph\":{\"rich_text\":[{\"text\":{\"content\":\"$BODY\"}}]}}]}" >/dev/null
echo '{"success":true,"stdout":"notion page updated"}'
"#,
        &["network:api.notion.com"],
    )?;
    install_plugin_script(
        plugins_dir,
        "discord",
        "discord-plugin.sh",
        r#"#!/bin/sh
read INPUT
WEBHOOK=$(echo "$INPUT" | jq -r '.config.webhook // empty')
MESSAGE=$(echo "$INPUT" | jq -r '.config.message // "Hello from JARVIS"')
if [ -z "$WEBHOOK" ]; then echo '{"success":false,"stderr":"missing webhook"}'; exit 0; fi
curl -s -X POST -H 'Content-Type: application/json' --data "{\"content\":\"$MESSAGE\"}" "$WEBHOOK" >/dev/null
echo '{"success":true,"stdout":"discord message sent"}'
"#,
        &["network:discord.com"],
    )?;
    Ok(())
}

pub fn install_builtin_slack_plugin(plugins_dir: &Path) -> jarvis_core::Result<()> {
    install_builtin_plugins(plugins_dir)
}

fn install_plugin_script(
    plugins_dir: &Path,
    name: &str,
    script_name: &str,
    script: &str,
    capabilities: &[&str],
) -> jarvis_core::Result<()> {
    let dir = plugins_dir.join(name);
    std::fs::create_dir_all(&dir)?;
    let caps: String = capabilities
        .iter()
        .map(|c| format!("  - {c}\n"))
        .collect();
    let manifest = format!(
        "name: {name}\nversion: \"1.0.0\"\ncapabilities:\n{caps}entrypoint: {script_name}\n"
    );
    std::fs::write(dir.join("plugin.yaml"), manifest)?;
    std::fs::write(dir.join(script_name), script)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            dir.join(script_name),
            std::fs::Permissions::from_mode(0o755),
        )?;
    }
    Ok(())
}
