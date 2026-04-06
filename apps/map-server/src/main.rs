use anyhow::Result;
use application::ClientCommand;
use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use map_server::{IncomingCommand, MapConfig, MapInstance};
use tokio::sync::mpsc;

#[derive(Clone)]
struct AppState {
    map_id: i32,
    cmd_tx: mpsc::Sender<IncomingCommand>,
}

#[derive(Debug, serde::Deserialize)]
struct MoveRequest {
    x: i32,
    y: i32,
    run: Option<bool>,
}

async fn healthz() -> &'static str {
    "ok"
}

async fn readyz() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true }))
}

async fn metrics() -> Json<observability::MetricsSnapshot> {
    Json(observability::METRICS.snapshot())
}

async fn metrics_prometheus() -> impl IntoResponse {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        observability::prometheus_text(),
    )
}

async fn map_ping(
    State(state): State<AppState>,
    Path(map_id): Path<i32>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "ok": map_id == state.map_id,
        "map_id": state.map_id
    }))
}

async fn map_enter_world(
    State(state): State<AppState>,
    Path(map_id): Path<i32>,
) -> Json<serde_json::Value> {
    if map_id != state.map_id {
        return Json(serde_json::json!({ "ok": false, "error": "unknown_map" }));
    }
    let _ = state
        .cmd_tx
        .send(IncomingCommand {
            session_id: uuid::Uuid::new_v4(),
            command: ClientCommand::EnterWorld,
        })
        .await;
    Json(serde_json::json!({ "ok": true }))
}

async fn map_move(
    State(state): State<AppState>,
    Path(map_id): Path<i32>,
    Json(req): Json<MoveRequest>,
) -> Json<serde_json::Value> {
    if map_id != state.map_id {
        return Json(serde_json::json!({ "ok": false, "error": "unknown_map" }));
    }
    let _ = state
        .cmd_tx
        .send(IncomingCommand {
            session_id: uuid::Uuid::new_v4(),
            command: ClientCommand::Move {
                x: req.x,
                y: req.y,
                run: req.run.unwrap_or(false),
            },
        })
        .await;
    Json(serde_json::json!({ "ok": true }))
}

#[tokio::main]
async fn main() -> Result<()> {
    let settings = config::Settings::from_env()?;
    observability::init_tracing("map-server", &settings.log_level, settings.log_json)?;
    observability::init_opentelemetry(settings.otel_endpoint.as_deref());

    let (in_tx, in_rx) = mpsc::channel::<IncomingCommand>(8192);
    let (outbound_tx, mut outbound_rx) = mpsc::channel(8192);
    let (persist_tx, mut persist_rx) = mpsc::channel(8192);

    let map_id = 1_i32;

    let instance = MapInstance::new(
        MapConfig {
            map_id,
            tick_ms: settings.map_tick_ms,
            command_budget: settings.map_command_budget,
        },
        in_rx,
        outbound_tx,
        persist_tx,
    );

    let map_task = tokio::spawn(instance.run());

    let outbound_task = tokio::spawn(async move {
        while let Some(evt) = outbound_rx.recv().await {
            tracing::debug!(?evt, "map outbound");
        }
    });

    let persist_task = tokio::spawn(async move {
        while let Some(op) = persist_rx.recv().await {
            tracing::debug!(?op, "map persist");
        }
    });

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics))
        .route("/metrics/prometheus", get(metrics_prometheus))
        .route("/v1/maps/:map_id/ping", get(map_ping))
        .route("/v1/maps/:map_id/enter-world", post(map_enter_world))
        .route("/v1/maps/:map_id/move", post(map_move))
        .with_state(AppState {
            map_id,
            cmd_tx: in_tx,
        });

    let listener = tokio::net::TcpListener::bind(&settings.map_bind).await?;
    tracing::info!(bind = %settings.map_bind, map_id, "map-server listening");

    let http_task = tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, app).await {
            tracing::error!(error = ?err, "map-server http crashed");
        }
    });

    tokio::signal::ctrl_c().await?;
    tracing::info!("map-server shutdown requested");

    map_task.abort();
    outbound_task.abort();
    persist_task.abort();
    http_task.abort();

    Ok(())
}
