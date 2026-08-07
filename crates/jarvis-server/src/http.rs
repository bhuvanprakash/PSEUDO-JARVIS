use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

use crate::orchestrator::{Orchestrator, RunOptions};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct RunBody {
    pub playbook_id: Option<String>,
    pub phrase: Option<String>,
}

#[derive(Deserialize)]
pub struct TrustGrantBody {
    pub playbook_id: String,
    pub step_id: Option<String>,
    pub level: String,
}

#[derive(Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub ok: bool,
    pub data: T,
}

pub fn router(state: Arc<AppState>, tray_dir: Option<std::path::PathBuf>) -> Router {
    let mut router = Router::new()
        .route("/health", get(health))
        .route("/status", get(status))
        .route("/observer/status", get(observer_status))
        .route("/observer/pause", post(observer_pause))
        .route("/observer/resume", post(observer_resume))
        .route("/observer/killswitch", get(killswitch_status))
        .route("/observer/killswitch/enable", post(killswitch_enable))
        .route("/observer/killswitch/disable", post(killswitch_disable))
        .route("/playbooks", get(list_playbooks))
        .route("/runs", get(list_runs))
        .route("/runs/:run_id", get(get_run))
        .route("/trust", get(list_trust))
        .route("/trust/grant", post(grant_trust))
        .route("/run", post(run_playbook))
        .route("/suggest/scan", post(scan_patterns))
        .layer(CorsLayer::permissive())
        .with_state(state);

    if let Some(dir) = tray_dir {
        if dir.exists() {
            let index = dir.join("index.html");
            router = router
                .nest_service("/tray", ServeDir::new(&dir))
                .route_service("/", ServeFile::new(index));
        }
    }

    router
}

async fn health() -> Json<ApiResponse<&'static str>> {
    Json(ApiResponse {
        ok: true,
        data: "ok",
    })
}

async fn status(State(state): State<Arc<AppState>>) -> Json<ApiResponse<serde_json::Value>> {
    Json(ApiResponse {
        ok: true,
        data: serde_json::json!({
            "vault": state.paths.root.display().to_string(),
            "llm_enabled": state.config.llm.enabled,
            "sync_enabled": state.config.sync.enabled,
        }),
    })
}

async fn observer_status(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<serde_json::Value>> {
    let observer = state.observer.lock().await;
    Json(ApiResponse {
        ok: true,
        data: observer.status(),
    })
}

async fn observer_pause(State(state): State<Arc<AppState>>) -> Json<ApiResponse<&'static str>> {
    let mut config = state.config.clone();
    config.observer.paused = true;
    let _ = config.save(&state.paths.config());
    Json(ApiResponse {
        ok: true,
        data: "paused",
    })
}

async fn observer_resume(State(state): State<Arc<AppState>>) -> Json<ApiResponse<&'static str>> {
    let mut config = state.config.clone();
    config.observer.paused = false;
    let _ = config.save(&state.paths.config());
    Json(ApiResponse {
        ok: true,
        data: "resumed",
    })
}

async fn killswitch_status(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<serde_json::Value>> {
    let observer = state.observer.lock().await;
    Json(ApiResponse {
        ok: true,
        data: serde_json::json!({ "active": observer.is_killed() }),
    })
}

async fn killswitch_enable(State(state): State<Arc<AppState>>) -> Json<ApiResponse<&'static str>> {
    let path = jarvis_core::paths::expand_tilde(&state.config.observer.kill_switch_file);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, "stop");
    Json(ApiResponse {
        ok: true,
        data: "enabled",
    })
}

async fn killswitch_disable(State(state): State<Arc<AppState>>) -> Json<ApiResponse<&'static str>> {
    let path = jarvis_core::paths::expand_tilde(&state.config.observer.kill_switch_file);
    if path.exists() {
        let _ = std::fs::remove_file(&path);
    }
    Json(ApiResponse {
        ok: true,
        data: "disabled",
    })
}

async fn list_playbooks(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<Vec<String>>> {
    let ids = state.vault.list_playbook_ids().unwrap_or_default();
    Json(ApiResponse { ok: true, data: ids })
}

async fn list_runs(State(state): State<Arc<AppState>>) -> Json<ApiResponse<serde_json::Value>> {
    let runs = state.vault.list_runs(50).unwrap_or_default();
    Json(ApiResponse {
        ok: true,
        data: serde_json::to_value(runs).unwrap_or_default(),
    })
}

async fn get_run(
    State(state): State<Arc<AppState>>,
    Path(run_id): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    match state.vault.get_run(&run_id) {
        Ok(run) => Json(ApiResponse {
            ok: true,
            data: serde_json::to_value(run).unwrap_or_default(),
        }),
        Err(e) => Json(ApiResponse {
            ok: false,
            data: serde_json::json!({ "error": e.to_string() }),
        }),
    }
}

async fn list_trust(State(state): State<Arc<AppState>>) -> Json<ApiResponse<serde_json::Value>> {
    let grants = state.vault.list_trust_grants().unwrap_or_default();
    Json(ApiResponse {
        ok: true,
        data: serde_json::to_value(grants).unwrap_or_default(),
    })
}

async fn grant_trust(
    State(state): State<Arc<AppState>>,
    Json(body): Json<TrustGrantBody>,
) -> Json<ApiResponse<serde_json::Value>> {
    let level: jarvis_core::types::TrustLevel = match body.level.parse() {
        Ok(l) => l,
        Err(e) => {
            return Json(ApiResponse {
                ok: false,
                data: serde_json::json!({ "error": e }),
            });
        }
    };
    let grant = jarvis_core::types::TrustGrant {
        playbook_id: body.playbook_id.clone(),
        step_id: body.step_id.unwrap_or_else(|| "*".to_string()),
        level,
        granted_at: chrono::Utc::now().to_rfc3339(),
    };
    match state.vault.set_trust_grant(&grant) {
        Ok(()) => Json(ApiResponse {
            ok: true,
            data: serde_json::to_value(grant).unwrap_or_default(),
        }),
        Err(e) => Json(ApiResponse {
            ok: false,
            data: serde_json::json!({ "error": e.to_string() }),
        }),
    }
}

async fn run_playbook(
    State(state): State<Arc<AppState>>,
    Json(body): Json<RunBody>,
) -> Json<ApiResponse<serde_json::Value>> {
    let result = Orchestrator::run(
        &state,
        RunOptions {
            playbook_id: body.playbook_id,
            phrase: body.phrase,
            trigger: "http".into(),
            schedule_triggered: false,
            auto_approve: true,
        },
        None,
    )
    .await;

    match result {
        Ok(run_id) => Json(ApiResponse {
            ok: true,
            data: serde_json::json!({ "run_id": run_id }),
        }),
        Err(e) => Json(ApiResponse {
            ok: false,
            data: serde_json::json!({ "error": e.to_string() }),
        }),
    }
}

async fn scan_patterns(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<serde_json::Value>> {
    let suggester = jarvis_suggester::PatternSuggester::new(&state.vault, state.llm.as_ref());
    match suggester.scan(7).await {
        Ok(patterns) => Json(ApiResponse {
            ok: true,
            data: serde_json::to_value(patterns).unwrap_or_default(),
        }),
        Err(e) => Json(ApiResponse {
            ok: false,
            data: serde_json::json!({ "error": e.to_string() }),
        }),
    }
}

pub async fn serve(
    state: Arc<AppState>,
    port: u16,
    tray_dir: Option<std::path::PathBuf>,
) -> jarvis_core::Result<()> {
    let app = router(state, tray_dir);
    let addr = format!("127.0.0.1:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| jarvis_core::error::JarvisError::Other(format!("http bind: {e}")))?;
    tracing::info!("HTTP API listening on http://{addr}");
    axum::serve(listener, app)
        .await
        .map_err(|e| jarvis_core::error::JarvisError::Other(format!("http serve: {e}")))?;
    Ok(())
}
