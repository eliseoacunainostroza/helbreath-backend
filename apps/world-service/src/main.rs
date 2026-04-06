use anyhow::Result;
use application::ClientCommand;
use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use infrastructure::PgRepository;
use map_server::{IncomingCommand, MapConfig, MapInstance};
use tokio::sync::mpsc;
use uuid::Uuid;
use world::{RoutedCommand, WorldCoordinator, WorldHandle, WorldMessage, WorldStats};

#[derive(Clone)]
struct AppState {
    world_handle: WorldHandle,
    repo: PgRepository,
}

#[derive(Debug, serde::Deserialize)]
struct BroadcastRequest {
    message: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum RouteCommandRequest {
    Heartbeat,
    EnterWorld,
    Chat {
        message: String,
    },
    Move {
        x: i32,
        y: i32,
        run: bool,
    },
    Attack {
        target_id: uuid::Uuid,
    },
    CastSkill {
        skill_id: i32,
        target_id: Option<uuid::Uuid>,
    },
    PickupItem {
        entity_id: uuid::Uuid,
    },
    DropItem {
        slot: i32,
        quantity: i32,
    },
    UseItem {
        slot: i32,
    },
    NpcInteraction {
        npc_id: uuid::Uuid,
    },
    Whisper {
        to_character: String,
        message: String,
    },
    GuildChat {
        message: String,
    },
    Logout,
}

#[derive(Debug, serde::Deserialize)]
struct RouteEnvelope {
    session_id: Uuid,
    #[serde(flatten)]
    command: RouteCommandRequest,
}

async fn healthz() -> &'static str {
    "ok"
}

async fn readyz(State(state): State<AppState>) -> Json<serde_json::Value> {
    let db_ok = state.repo.readiness_check().await.is_ok();
    Json(serde_json::json!({ "ok": db_ok, "db": db_ok }))
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

async fn world_stats(State(state): State<AppState>) -> Json<WorldStats> {
    Json(state.world_handle.get_stats().await)
}

async fn world_broadcast(
    State(state): State<AppState>,
    Json(req): Json<BroadcastRequest>,
) -> Json<serde_json::Value> {
    if req.message.trim().is_empty() {
        return Json(serde_json::json!({ "ok": false, "error": "message_empty" }));
    }
    state.world_handle.broadcast(req.message).await;
    Json(serde_json::json!({ "ok": true }))
}

async fn route_command(
    State(state): State<AppState>,
    Path(map_id): Path<i32>,
    Json(req): Json<RouteEnvelope>,
) -> Json<serde_json::Value> {
    let cmd = match req.command {
        RouteCommandRequest::Heartbeat => ClientCommand::Heartbeat,
        RouteCommandRequest::EnterWorld => ClientCommand::EnterWorld,
        RouteCommandRequest::Chat { message } => ClientCommand::Chat { message },
        RouteCommandRequest::Move { x, y, run } => ClientCommand::Move { x, y, run },
        RouteCommandRequest::Attack { target_id } => ClientCommand::Attack { target_id },
        RouteCommandRequest::CastSkill {
            skill_id,
            target_id,
        } => ClientCommand::CastSkill {
            skill_id,
            target_id,
        },
        RouteCommandRequest::PickupItem { entity_id } => ClientCommand::PickupItem { entity_id },
        RouteCommandRequest::DropItem { slot, quantity } => {
            ClientCommand::DropItem { slot, quantity }
        }
        RouteCommandRequest::UseItem { slot } => ClientCommand::UseItem { slot },
        RouteCommandRequest::NpcInteraction { npc_id } => ClientCommand::NpcInteraction { npc_id },
        RouteCommandRequest::Whisper {
            to_character,
            message,
        } => ClientCommand::Whisper {
            to_character,
            message,
        },
        RouteCommandRequest::GuildChat { message } => ClientCommand::GuildChat { message },
        RouteCommandRequest::Logout => ClientCommand::Logout,
    };

    state
        .world_handle
        .route_to_map(map_id, req.session_id, cmd)
        .await;
    Json(serde_json::json!({ "ok": true }))
}

async fn persist_map_op(repo: &PgRepository, op: map_server::PersistOp) -> Result<()> {
    match op {
        map_server::PersistOp::SaveCharacterPosition {
            character_id,
            map_id,
            x,
            y,
        } => {
            repo.persist_position_snapshot(character_id, map_id, x, y)
                .await?
        }
        map_server::PersistOp::SaveInventoryChange {
            character_id,
            map_id,
            reason,
        } => {
            repo.persist_inventory_event(character_id, map_id, &reason)
                .await?
        }
        map_server::PersistOp::SaveCombatLog {
            attacker,
            defender,
            map_id,
            damage,
        } => {
            repo.persist_combat_event(attacker, defender, map_id, damage)
                .await?
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let settings = config::Settings::from_env()?;
    observability::init_tracing("world-service", &settings.log_level, settings.log_json)?;
    observability::init_opentelemetry(settings.otel_endpoint.as_deref());
    let repo = PgRepository::new(&settings).await?;

    let (world_tx, world_rx) = mpsc::channel::<WorldMessage>(4096);
    let world_handle = WorldHandle::new(world_tx.clone());

    let (map_cmd_tx, map_cmd_rx) = mpsc::channel::<IncomingCommand>(8192);
    let (outbound_tx, mut outbound_rx) = mpsc::channel(8192);
    let (persist_tx, mut persist_rx) = mpsc::channel(8192);

    let (world_map_tx, mut world_map_rx) = mpsc::channel::<RoutedCommand>(8192);
    world_tx
        .send(WorldMessage::RegisterMap {
            map_id: 1,
            tx: world_map_tx,
        })
        .await?;

    let map_instance = MapInstance::new(
        MapConfig {
            map_id: 1,
            tick_ms: settings.map_tick_ms,
            command_budget: settings.map_command_budget,
        },
        map_cmd_rx,
        outbound_tx,
        persist_tx,
    );

    let coordinator_task = tokio::spawn(WorldCoordinator::new(world_rx).run());
    let map_task = tokio::spawn(map_instance.run());

    let router_task = tokio::spawn(async move {
        while let Some(routed) = world_map_rx.recv().await {
            let _ = map_cmd_tx
                .send(IncomingCommand {
                    session_id: routed.session_id,
                    command: routed.command,
                })
                .await;
        }
    });

    let outbound_task = tokio::spawn(async move {
        while let Some(event) = outbound_rx.recv().await {
            tracing::debug!(?event, "outbound event");
        }
    });

    let persist_repo = repo.clone();
    let persist_task = tokio::spawn(async move {
        while let Some(op) = persist_rx.recv().await {
            if let Err(err) = persist_map_op(&persist_repo, op).await {
                observability::MetricsRegistry::inc(
                    &observability::METRICS.persistence_errors_total,
                );
                tracing::error!(error = ?err, "persist op failed");
            }
        }
    });

    let state = AppState {
        world_handle,
        repo: repo.clone(),
    };

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics))
        .route("/metrics/prometheus", get(metrics_prometheus))
        .route("/v1/world/stats", get(world_stats))
        .route("/v1/world/broadcast", post(world_broadcast))
        .route("/v1/world/maps/:map_id/route", post(route_command))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&settings.world_bind).await?;
    tracing::info!(bind = %settings.world_bind, "world-service listening");

    let http_task = tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, app).await {
            tracing::error!(error = ?err, "world-service http crashed");
        }
    });

    tokio::signal::ctrl_c().await?;
    tracing::info!("world-service shutdown requested");

    coordinator_task.abort();
    map_task.abort();
    router_task.abort();
    outbound_task.abort();
    persist_task.abort();
    http_task.abort();

    Ok(())
}
