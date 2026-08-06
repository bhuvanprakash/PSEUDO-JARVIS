use std::path::Path;
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
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

pub fn install_builtin_slack_plugin(plugins_dir: &Path) -> jarvis_core::Result<()> {
    let dir = plugins_dir.join("slack");
    std::fs::create_dir_all(&dir)?;
    let manifest = r#"name: slack
version: "1.0.0"
capabilities:
  - network:hooks.slack.com
entrypoint: slack-plugin.sh
"#;
    std::fs::write(dir.join("plugin.yaml"), manifest)?;
    std::fs::write(
        dir.join("slack-plugin.sh"),
        r#"#!/bin/sh
read INPUT
WEBHOOK=$(echo "$INPUT" | jq -r '.config.webhook // empty')
MESSAGE=$(echo "$INPUT" | jq -r '.config.message // "Hello from JARVIS"')
if [ -z "$WEBHOOK" ]; then
  echo '{"success":false,"stderr":"missing webhook"}'
  exit 0
fi
curl -s -X POST -H 'Content-type: application/json' --data "{\"text\":\"$MESSAGE\"}" "$WEBHOOK" >/dev/null
echo '{"success":true,"stdout":"slack message sent"}'
"#,
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.join("slack-plugin.sh"), std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}
