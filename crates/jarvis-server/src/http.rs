use std::sync::Arc;

use axum::extract::State;
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
        .route("/playbooks", get(list_playbooks))
        .route("/runs", get(list_runs))
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
