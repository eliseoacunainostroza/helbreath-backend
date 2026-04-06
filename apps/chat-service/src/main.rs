use anyhow::Result;
use axum::{
    extract::{Query, State},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chat::{ChatCommand, ChatEnvelope, ChatService};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

#[derive(Clone)]
struct AppState {
    cmd_tx: mpsc::Sender<ChatCommand>,
    recent: Arc<RwLock<VecDeque<ChatEnvelope>>>,
}

#[derive(Debug, serde::Deserialize)]
struct SayMapRequest {
    from_character_id: uuid::Uuid,
    map_id: i32,
    message: String,
}

#[derive(Debug, serde::Deserialize)]
struct WhisperRequest {
    from_character_id: uuid::Uuid,
    to_character_name: String,
    message: String,
}

#[derive(Debug, serde::Deserialize)]
struct GuildRequest {
    from_character_id: uuid::Uuid,
    guild_id: uuid::Uuid,
    message: String,
}

#[derive(Debug, serde::Deserialize)]
struct OutboundQuery {
    limit: Option<usize>,
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

async fn say_map(
    State(state): State<AppState>,
    Json(req): Json<SayMapRequest>,
) -> Json<serde_json::Value> {
    if req.message.trim().is_empty() {
        return Json(serde_json::json!({ "ok": false, "error": "empty_message" }));
    }

    let _ = state
        .cmd_tx
        .send(ChatCommand::SayMap {
            from_character_id: req.from_character_id,
            map_id: req.map_id,
            message: req.message,
        })
        .await;

    Json(serde_json::json!({ "ok": true }))
}

async fn whisper(
    State(state): State<AppState>,
    Json(req): Json<WhisperRequest>,
) -> Json<serde_json::Value> {
    if req.message.trim().is_empty() || req.to_character_name.trim().is_empty() {
        return Json(serde_json::json!({ "ok": false, "error": "invalid_payload" }));
    }

    let _ = state
        .cmd_tx
        .send(ChatCommand::Whisper {
            from_character_id: req.from_character_id,
            to_character_name: req.to_character_name,
            message: req.message,
        })
        .await;

    Json(serde_json::json!({ "ok": true }))
}

async fn guild(
    State(state): State<AppState>,
    Json(req): Json<GuildRequest>,
) -> Json<serde_json::Value> {
    if req.message.trim().is_empty() {
        return Json(serde_json::json!({ "ok": false, "error": "empty_message" }));
    }

    let _ = state
        .cmd_tx
        .send(ChatCommand::Guild {
            from_character_id: req.from_character_id,
            guild_id: req.guild_id,
            message: req.message,
        })
        .await;

    Json(serde_json::json!({ "ok": true }))
}

async fn outbound(
    State(state): State<AppState>,
    Query(query): Query<OutboundQuery>,
) -> Json<serde_json::Value> {
    let limit = query.limit.unwrap_or(20).clamp(1, 200);
    let guard = state.recent.read().await;
    let rows: Vec<ChatEnvelope> = guard.iter().rev().take(limit).cloned().collect();
    Json(serde_json::json!({
        "rows": rows,
        "count": rows.len(),
    }))
}

#[tokio::main]
async fn main() -> Result<()> {
    let settings = config::Settings::from_env()?;
    observability::init_tracing("chat-service", &settings.log_level, settings.log_json)?;
    observability::init_opentelemetry(settings.otel_endpoint.as_deref());

    let (cmd_tx, cmd_rx) = mpsc::channel(2048);
    let (out_tx, mut out_rx) = mpsc::channel(2048);

    let svc = ChatService {
        rx: cmd_rx,
        tx: out_tx,
    };
    let svc_task = tokio::spawn(svc.run());

    let recent = Arc::new(RwLock::new(VecDeque::<ChatEnvelope>::with_capacity(256)));
    let recent_sink = recent.clone();
    let sink_task = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            tracing::debug!(?msg, "chat out");
            let mut guard = recent_sink.write().await;
            if guard.len() >= 256 {
                let _ = guard.pop_front();
            }
            guard.push_back(msg);
        }
    });

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics))
        .route("/metrics/prometheus", get(metrics_prometheus))
        .route("/v1/chat/map", post(say_map))
        .route("/v1/chat/whisper", post(whisper))
        .route("/v1/chat/guild", post(guild))
        .route("/v1/chat/outbound", get(outbound))
        .with_state(AppState { cmd_tx, recent });

    let listener = tokio::net::TcpListener::bind(&settings.chat_bind).await?;
    tracing::info!(bind = %settings.chat_bind, "chat-service listening");

    let http_task = tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, app).await {
            tracing::error!(error = ?err, "chat-service http crashed");
        }
    });

    tokio::signal::ctrl_c().await?;
    tracing::info!("chat-service shutdown requested");

    svc_task.abort();
    sink_task.abort();
    http_task.abort();

    Ok(())
}
