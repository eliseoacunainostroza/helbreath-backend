use anyhow::Result;
use application::ClientCommand;
use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use infrastructure::{AdminRepository, PgRepository};
use map_server::{IncomingCommand, MapConfig, MapInstance, OutboundEvent};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use uuid::Uuid;
use world::{RoutedCommand, WorldCoordinator, WorldHandle, WorldMessage, WorldStats};

#[derive(Clone)]
struct AppState {
    world_handle: WorldHandle,
    repo: PgRepository,
    recent_outbound: Arc<Mutex<VecDeque<WorldOutboundRecord>>>,
    map_manager: Arc<MapRuntimeManager>,
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
    #[serde(default)]
    character_id: Option<Uuid>,
    #[serde(flatten)]
    command: RouteCommandRequest,
}

#[derive(Debug, Clone, serde::Serialize)]
struct WorldOutboundRecord {
    seq: u64,
    kind: &'static str,
    session_id: Option<Uuid>,
    attacker: Option<Uuid>,
    defender: Option<Uuid>,
    x: Option<i32>,
    y: Option<i32>,
    damage: Option<i32>,
    text: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct OutboundQuery {
    limit: Option<usize>,
}

struct MapRuntimeHandle {
    map_task: JoinHandle<()>,
    router_task: JoinHandle<()>,
}

struct MapRuntimeManager {
    world_tx: mpsc::Sender<WorldMessage>,
    outbound_tx: mpsc::Sender<OutboundEvent>,
    persist_tx: mpsc::Sender<map_server::PersistOp>,
    tick_ms: u64,
    command_budget: usize,
    runtimes: Mutex<HashMap<i32, MapRuntimeHandle>>,
}

impl MapRuntimeManager {
    fn new(
        world_tx: mpsc::Sender<WorldMessage>,
        outbound_tx: mpsc::Sender<OutboundEvent>,
        persist_tx: mpsc::Sender<map_server::PersistOp>,
        tick_ms: u64,
        command_budget: usize,
    ) -> Self {
        Self {
            world_tx,
            outbound_tx,
            persist_tx,
            tick_ms,
            command_budget,
            runtimes: Mutex::new(HashMap::new()),
        }
    }

    async fn activate(&self, map_id: i32) -> Result<bool> {
        let mut guard = self.runtimes.lock().await;
        if guard.contains_key(&map_id) {
            return Ok(false);
        }

        let (map_cmd_tx, map_cmd_rx) = mpsc::channel::<IncomingCommand>(8192);
        let (world_map_tx, mut world_map_rx) = mpsc::channel::<RoutedCommand>(8192);

        self.world_tx
            .send(WorldMessage::RegisterMap {
                map_id,
                tx: world_map_tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("world coordinator unavailable"))?;

        let map_instance = MapInstance::new(
            MapConfig {
                map_id,
                tick_ms: self.tick_ms,
                command_budget: self.command_budget,
            },
            map_cmd_rx,
            self.outbound_tx.clone(),
            self.persist_tx.clone(),
        );

        let map_task = tokio::spawn(map_instance.run());
        let router_task = tokio::spawn(async move {
            while let Some(routed) = world_map_rx.recv().await {
                let _ = map_cmd_tx
                    .send(IncomingCommand {
                        session_id: routed.session_id,
                        character_id: routed.character_id,
                        command: routed.command,
                    })
                    .await;
            }
        });

        guard.insert(
            map_id,
            MapRuntimeHandle {
                map_task,
                router_task,
            },
        );
        tracing::info!(map_id, "map runtime activated");
        Ok(true)
    }

    async fn deactivate(&self, map_id: i32) -> Result<bool> {
        let maybe_handle = {
            let mut guard = self.runtimes.lock().await;
            guard.remove(&map_id)
        };

        let Some(handle) = maybe_handle else {
            return Ok(false);
        };

        let _ = self
            .world_tx
            .send(WorldMessage::UnregisterMap { map_id })
            .await;

        handle.map_task.abort();
        handle.router_task.abort();
        tracing::info!(map_id, "map runtime deactivated");
        Ok(true)
    }

    async fn is_active(&self, map_id: i32) -> bool {
        let guard = self.runtimes.lock().await;
        guard.contains_key(&map_id)
    }

    async fn shutdown(&self) {
        let handles = {
            let mut guard = self.runtimes.lock().await;
            std::mem::take(&mut *guard)
        };
        for (map_id, handle) in handles {
            let _ = self
                .world_tx
                .send(WorldMessage::UnregisterMap { map_id })
                .await;
            handle.map_task.abort();
            handle.router_task.abort();
        }
    }
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

fn outbound_to_record(seq: u64, event: OutboundEvent) -> WorldOutboundRecord {
    match event {
        OutboundEvent::Text { session_id, text } => WorldOutboundRecord {
            seq,
            kind: "text",
            session_id: Some(session_id),
            attacker: None,
            defender: None,
            x: None,
            y: None,
            damage: None,
            text: Some(text),
        },
        OutboundEvent::Position { session_id, x, y } => WorldOutboundRecord {
            seq,
            kind: "position",
            session_id: Some(session_id),
            attacker: None,
            defender: None,
            x: Some(x),
            y: Some(y),
            damage: None,
            text: None,
        },
        OutboundEvent::CombatResult {
            attacker,
            defender,
            damage,
        } => WorldOutboundRecord {
            seq,
            kind: "combat_result",
            session_id: None,
            attacker: Some(attacker),
            defender: Some(defender),
            x: None,
            y: None,
            damage: Some(damage),
            text: None,
        },
    }
}

async fn world_map_outbound(
    State(state): State<AppState>,
    Path(map_id): Path<i32>,
    Query(query): Query<OutboundQuery>,
) -> Json<serde_json::Value> {
    let limit = query.limit.unwrap_or(20).clamp(1, 200);
    let rows = {
        let guard = state.recent_outbound.lock().await;
        let mut collected: Vec<_> = guard.iter().rev().take(limit).cloned().collect();
        collected.reverse();
        collected
    };
    let is_active = state.map_manager.is_active(map_id).await;
    Json(serde_json::json!({
        "ok": is_active,
        "map_id": map_id,
        "is_active": is_active,
        "rows": rows,
    }))
}

async fn world_map_activate(
    State(state): State<AppState>,
    Path(map_id): Path<i32>,
) -> Json<serde_json::Value> {
    let maps = match state.repo.list_maps().await {
        Ok(maps) => maps,
        Err(err) => {
            tracing::error!(error = ?err, map_id, "failed to query map catalog");
            return Json(serde_json::json!({
                "ok": false,
                "error": "catalog_unavailable",
            }));
        }
    };
    let map_exists = maps.into_iter().any(|map| map.id == map_id);
    if !map_exists {
        return Json(serde_json::json!({
            "ok": false,
            "error": "map_not_found",
            "map_id": map_id,
        }));
    }

    match state.map_manager.activate(map_id).await {
        Ok(changed) => Json(serde_json::json!({
            "ok": true,
            "map_id": map_id,
            "changed": changed,
            "is_active": true,
        })),
        Err(err) => {
            tracing::error!(error = ?err, map_id, "failed to activate map runtime");
            Json(serde_json::json!({
                "ok": false,
                "error": "activation_failed",
                "map_id": map_id,
            }))
        }
    }
}

async fn world_map_deactivate(
    State(state): State<AppState>,
    Path(map_id): Path<i32>,
) -> Json<serde_json::Value> {
    let stats = state.world_handle.get_stats().await;
    let players_online = stats
        .players_by_map
        .iter()
        .find(|(current_map, _)| *current_map == map_id)
        .map(|(_, count)| *count)
        .unwrap_or(0);
    if players_online > 0 {
        return Json(serde_json::json!({
            "ok": false,
            "error": "map_has_players",
            "map_id": map_id,
            "players_online": players_online,
        }));
    }

    match state.map_manager.deactivate(map_id).await {
        Ok(changed) => Json(serde_json::json!({
            "ok": true,
            "map_id": map_id,
            "changed": changed,
            "is_active": false,
        })),
        Err(err) => {
            tracing::error!(error = ?err, map_id, "failed to deactivate map runtime");
            Json(serde_json::json!({
                "ok": false,
                "error": "deactivation_failed",
                "map_id": map_id,
            }))
        }
    }
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
        .route_to_map(map_id, req.session_id, req.character_id, cmd)
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

    let (outbound_tx, mut outbound_rx) = mpsc::channel::<OutboundEvent>(8192);
    let (persist_tx, mut persist_rx) = mpsc::channel::<map_server::PersistOp>(8192);
    let recent_outbound: Arc<Mutex<VecDeque<WorldOutboundRecord>>> =
        Arc::new(Mutex::new(VecDeque::with_capacity(256)));
    let map_manager = Arc::new(MapRuntimeManager::new(
        world_tx.clone(),
        outbound_tx.clone(),
        persist_tx.clone(),
        settings.map_tick_ms,
        settings.map_command_budget,
    ));

    let coordinator_task = tokio::spawn(WorldCoordinator::new(world_rx).run());

    let mut startup_maps = repo.list_active_map_ids().await.unwrap_or_default();
    if startup_maps.is_empty() {
        startup_maps.push(1);
    }
    startup_maps.sort_unstable();
    startup_maps.dedup();

    for map_id in startup_maps {
        if let Err(err) = map_manager.activate(map_id).await {
            tracing::error!(error = ?err, map_id, "failed to activate map runtime at startup");
        }
    }

    let outbound_store = Arc::clone(&recent_outbound);
    let outbound_task = tokio::spawn(async move {
        let mut seq = 0_u64;
        while let Some(event) = outbound_rx.recv().await {
            seq = seq.wrapping_add(1);
            let record = outbound_to_record(seq, event.clone());
            {
                let mut guard = outbound_store.lock().await;
                guard.push_back(record);
                while guard.len() > 256 {
                    let _ = guard.pop_front();
                }
            }
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
        recent_outbound,
        map_manager: Arc::clone(&map_manager),
    };

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics))
        .route("/metrics/prometheus", get(metrics_prometheus))
        .route("/v1/world/stats", get(world_stats))
        .route("/v1/world/broadcast", post(world_broadcast))
        .route("/v1/world/maps/:map_id/activate", post(world_map_activate))
        .route("/v1/world/maps/:map_id/deactivate", post(world_map_deactivate))
        .route("/v1/world/maps/:map_id/outbound", get(world_map_outbound))
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

    map_manager.shutdown().await;
    coordinator_task.abort();
    outbound_task.abort();
    persist_task.abort();
    http_task.abort();

    Ok(())
}
