use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum IpcRequest {
    Ping,
    Status,
    Run { playbook_id: Option<String>, phrase: Option<String> },
    ObserverStatus,
    ObserverPause,
    ObserverResume,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IpcResponse {
    pub ok: bool,
    pub data: serde_json::Value,
    pub error: Option<String>,
}

pub async fn serve_ipc(state: Arc<AppState>, socket_path: &Path) -> jarvis_core::Result<()> {
    if socket_path.exists() {
        std::fs::remove_file(socket_path)?;
    }
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let listener = UnixListener::bind(socket_path)
        .map_err(|e| jarvis_core::error::JarvisError::Other(format!("ipc bind: {e}")))?;

    loop {
        let (stream, _) = listener
            .accept()
            .await
            .map_err(|e| jarvis_core::error::JarvisError::Other(format!("ipc accept: {e}")))?;
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(state, stream).await {
                tracing::error!("ipc connection error: {e}");
            }
        });
    }
}

async fn handle_connection(state: Arc<AppState>, stream: UnixStream) -> jarvis_core::Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    if let Some(line) = lines.next_line().await? {
        let request: IpcRequest = serde_json::from_str(&line)?;
        let response = dispatch(state, request).await;
        writer.write_all(format!("{}\n", serde_json::to_string(&response)?).as_bytes()).await?;
    }
    Ok(())
}

async fn dispatch(state: Arc<AppState>, request: IpcRequest) -> IpcResponse {
    match request {
        IpcRequest::Ping => IpcResponse {
            ok: true,
            data: serde_json::json!({ "pong": true }),
            error: None,
        },
        IpcRequest::Status => {
            let playbooks = state.vault.list_playbook_ids().unwrap_or_default();
            IpcResponse {
                ok: true,
                data: serde_json::json!({
                    "playbooks": playbooks,
                    "vault": state.paths.root.display().to_string(),
                }),
                error: None,
            }
        }
        IpcRequest::ObserverStatus => {
            let observer = state.observer.lock().await;
            IpcResponse {
                ok: true,
                data: observer.status(),
                error: None,
            }
        }
        IpcRequest::ObserverPause => {
            let mut config = state.config.clone();
            config.observer.paused = true;
            let _ = config.save(&state.paths.config());
            IpcResponse {
                ok: true,
                data: serde_json::json!({ "paused": true }),
                error: None,
            }
        }
        IpcRequest::ObserverResume => {
            let mut config = state.config.clone();
            config.observer.paused = false;
            let _ = config.save(&state.paths.config());
            IpcResponse {
                ok: true,
                data: serde_json::json!({ "paused": false }),
                error: None,
            }
        }
        IpcRequest::Run { playbook_id, phrase } => {
            match crate::orchestrator::Orchestrator::run(
                &state,
                crate::orchestrator::RunOptions {
                    playbook_id,
                    phrase,
                    trigger: "ipc".into(),
                    schedule_triggered: false,
                    auto_approve: true,
                },
                None,
            )
            .await
            {
                Ok(run_id) => IpcResponse {
                    ok: true,
                    data: serde_json::json!({ "run_id": run_id }),
                    error: None,
                },
                Err(e) => IpcResponse {
                    ok: false,
                    data: serde_json::json!({}),
                    error: Some(e.to_string()),
                },
            }
        }
    }
}

pub async fn ipc_call(socket_path: &Path, request: &IpcRequest) -> jarvis_core::Result<IpcResponse> {
    let mut stream = UnixStream::connect(socket_path)
        .await
        .map_err(|e| jarvis_core::error::JarvisError::Other(format!("ipc connect: {e}")))?;
    stream
        .write_all(format!("{}\n", serde_json::to_string(request)?).as_bytes())
        .await?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).await?;
    Ok(serde_json::from_str(&line)?)
}
