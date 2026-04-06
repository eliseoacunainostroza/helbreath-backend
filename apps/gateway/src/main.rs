use anyhow::Result;
use application::{ClientCommand, InternalCommand, LoginPayload};
use axum::{extract::State, response::IntoResponse, routing::get, Json, Router};
use bytes::BytesMut;
use net::{
    decode_frame, split_frames, translate_packet_for_version, ProtocolVersion, SessionPhase,
    TokenBucketRateLimiter,
};
use observability::{MetricsRegistry, METRICS};
use redis::aio::MultiplexedConnection;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::AsyncReadExt;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    http_client: reqwest::Client,
    auth_base: String,
    world_base: String,
}

#[derive(Debug, Clone)]
struct SessionRuntime {
    remote_ip: String,
    account_id: Option<Uuid>,
    character_id: Option<Uuid>,
    map_id: Option<i32>,
    in_world: bool,
}

#[derive(Clone)]
struct InternalRouterContext {
    http_client: reqwest::Client,
    auth_base: String,
    world_base: String,
    sessions: Arc<RwLock<HashMap<Uuid, SessionRuntime>>>,
}

#[derive(Debug, serde::Serialize)]
struct AuthLoginRequest {
    session_id: Uuid,
    username: String,
    password: String,
    remote_ip: String,
}

#[derive(Debug, serde::Deserialize)]
struct AuthLoginResponse {
    accepted: bool,
    account_id: Option<Uuid>,
    reason: Option<String>,
}

#[derive(Debug, serde::Serialize)]
struct AuthLogoutRequest {
    session_id: Uuid,
}

#[derive(Debug, serde::Deserialize)]
struct AuthCharacterSummary {
    character_id: Uuid,
    name: String,
    class: String,
    map_id: i32,
    x: i32,
    y: i32,
    level: i32,
}

#[derive(Debug, serde::Deserialize)]
struct AuthCharacterListResponse {
    ok: bool,
    characters: Vec<AuthCharacterSummary>,
}

#[derive(Debug, serde::Serialize)]
struct AuthCharacterCreateRequest {
    session_id: Uuid,
    name: String,
    class: String,
    gender: i16,
    skin_color: i16,
    hair_style: i16,
    hair_color: i16,
    underwear_color: i16,
    stats: [i16; 6],
}

#[derive(Debug, serde::Deserialize)]
struct AuthCharacterCreateResponse {
    ok: bool,
    character: Option<AuthCharacterSummary>,
    reason: Option<String>,
}

#[derive(Debug, serde::Serialize)]
struct AuthCharacterDeleteRequest {
    session_id: Uuid,
    character_id: Uuid,
}

#[derive(Debug, serde::Deserialize)]
struct AuthCharacterDeleteResponse {
    ok: bool,
}

#[derive(Debug, serde::Serialize)]
struct AuthCharacterSelectRequest {
    session_id: Uuid,
    character_id: Uuid,
}

#[derive(Debug, serde::Deserialize)]
struct AuthCharacterSelectResponse {
    ok: bool,
    character_id: Option<Uuid>,
    map_id: Option<i32>,
    x: Option<i32>,
    y: Option<i32>,
    reason: Option<String>,
}

#[derive(Debug, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WorldRouteCommandRequest {
    Heartbeat,
    EnterWorld,
    Move {
        x: i32,
        y: i32,
        run: bool,
    },
    Attack {
        target_id: Uuid,
    },
    CastSkill {
        skill_id: i32,
        target_id: Option<Uuid>,
    },
    PickupItem {
        entity_id: Uuid,
    },
    DropItem {
        slot: i32,
        quantity: i32,
    },
    UseItem {
        slot: i32,
    },
    NpcInteraction {
        npc_id: Uuid,
    },
    Chat {
        message: String,
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

#[derive(Debug, serde::Serialize)]
struct WorldRouteEnvelope {
    session_id: Uuid,
    character_id: Option<Uuid>,
    #[serde(flatten)]
    command: WorldRouteCommandRequest,
}

async fn healthz() -> &'static str {
    "ok"
}

async fn readyz(State(state): State<AppState>) -> Json<serde_json::Value> {
    let auth_ok = probe_health(&state.http_client, &state.auth_base).await;
    let world_ok = probe_health(&state.http_client, &state.world_base).await;

    Json(serde_json::json!({
        "ok": auth_ok && world_ok,
        "auth": auth_ok,
        "world": world_ok
    }))
}

async fn metrics() -> Json<observability::MetricsSnapshot> {
    Json(METRICS.snapshot())
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

async fn probe_health(client: &reqwest::Client, base: &str) -> bool {
    match client.get(format!("{base}/healthz")).send().await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let settings = config::Settings::from_env()?;
    observability::init_tracing("gateway", &settings.log_level, settings.log_json)?;
    observability::init_opentelemetry(settings.otel_endpoint.as_deref());

    let http_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()?;
    let redis_client = if settings.redis_enabled {
        match redis::Client::open(settings.redis_url.clone()) {
            Ok(client) => {
                tracing::info!("distributed redis rate limiting enabled");
                Some(client)
            }
            Err(err) => {
                tracing::warn!(
                    error = ?err,
                    "invalid redis configuration, falling back to local gateway rate limiting"
                );
                None
            }
        }
    } else {
        None
    };

    let auth_base = format!("http://{}", settings.auth_bind);
    let world_base = format!("http://{}", settings.world_bind);
    let sessions = Arc::new(RwLock::new(HashMap::<Uuid, SessionRuntime>::new()));
    let ip_connection_counts = Arc::new(RwLock::new(HashMap::<String, usize>::new()));

    let state = AppState {
        http_client: http_client.clone(),
        auth_base: auth_base.clone(),
        world_base: world_base.clone(),
    };

    let (internal_tx, internal_rx) = mpsc::channel::<InternalCommand>(4096);

    let http_app = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics))
        .route("/metrics/prometheus", get(metrics_prometheus))
        .with_state(state.clone());

    let http_listener = tokio::net::TcpListener::bind(&settings.gateway_http_bind).await?;
    tracing::info!(bind = %settings.gateway_http_bind, "gateway http listening");

    let tcp_bind = settings.gateway_tcp_bind.clone();
    let tcp_settings = settings.clone();
    let tcp_internal_tx = internal_tx.clone();
    let tcp_sessions = sessions.clone();
    let tcp_ip_connection_counts = ip_connection_counts.clone();
    let tcp_redis_client = redis_client.clone();
    let tcp_task = tokio::spawn(async move {
        if let Err(err) = run_game_tcp(
            tcp_bind,
            tcp_settings,
            tcp_internal_tx,
            tcp_sessions,
            tcp_ip_connection_counts,
            tcp_redis_client,
        )
        .await
        {
            tracing::error!(error = ?err, "gateway tcp loop crashed");
        }
    });

    let router_ctx = InternalRouterContext {
        http_client,
        auth_base,
        world_base,
        sessions: sessions.clone(),
    };
    let router_task = tokio::spawn(run_internal_router(internal_rx, router_ctx));

    let http_task = tokio::spawn(async move {
        if let Err(err) = axum::serve(http_listener, http_app).await {
            tracing::error!(error = ?err, "gateway http crashed");
        }
    });

    tokio::signal::ctrl_c().await?;
    tracing::info!("gateway shutdown requested");

    tcp_task.abort();
    router_task.abort();
    http_task.abort();

    Ok(())
}

async fn run_internal_router(
    mut internal_rx: mpsc::Receiver<InternalCommand>,
    ctx: InternalRouterContext,
) {
    while let Some(cmd) = internal_rx.recv().await {
        match cmd {
            InternalCommand::RouteClientCommand {
                session_id,
                map_id_hint,
                command,
            } => {
                route_client_command(&ctx, session_id, map_id_hint, command).await;
            }
            InternalCommand::DisconnectSession { session_id, reason } => {
                tracing::info!(%session_id, %reason, "disconnect session command");
                route_auth_logout(&ctx, session_id).await;
                ctx.sessions.write().await.remove(&session_id);
            }
            InternalCommand::Broadcast { message, .. } => {
                tracing::info!(%message, "admin broadcast via gateway");
            }
            InternalCommand::MoveCharacter {
                character_id,
                target_map_id,
                x,
                y,
            } => {
                tracing::info!(%character_id, target_map_id, x, y, "move character internal command");
            }
        }
    }
}

async fn route_client_command(
    ctx: &InternalRouterContext,
    session_id: Uuid,
    map_id_hint: Option<i32>,
    command: ClientCommand,
) {
    let map_id = resolve_map_id(ctx, session_id, map_id_hint).await;
    match command {
        ClientCommand::Login(payload) => {
            route_auth_login(ctx, session_id, payload).await;
        }
        ClientCommand::CharacterList => {
            if !is_authenticated(ctx, session_id).await {
                tracing::warn!(%session_id, "character list rejected: unauthenticated");
                return;
            }
            route_auth_character_list(ctx, session_id).await;
        }
        ClientCommand::CharacterCreate(payload) => {
            if !is_authenticated(ctx, session_id).await {
                tracing::warn!(%session_id, "character create rejected: unauthenticated");
                return;
            }
            route_auth_character_create(ctx, session_id, payload).await;
        }
        ClientCommand::CharacterDelete { character_id } => {
            if !is_authenticated(ctx, session_id).await {
                tracing::warn!(%session_id, "character delete rejected: unauthenticated");
                return;
            }
            route_auth_character_delete(ctx, session_id, character_id).await;
        }
        ClientCommand::CharacterSelect { character_id } => {
            if !is_authenticated(ctx, session_id).await {
                tracing::warn!(%session_id, "character select rejected: unauthenticated");
                return;
            }
            route_auth_character_select(ctx, session_id, character_id).await;
        }
        ClientCommand::EnterWorld => {
            if !is_character_selected(ctx, session_id).await {
                tracing::warn!(%session_id, "enter world rejected: no character selected");
                return;
            }
            if route_world_command(
                ctx,
                session_id,
                map_id,
                WorldRouteCommandRequest::EnterWorld,
            )
            .await
            {
                let mut guard = ctx.sessions.write().await;
                if let Some(sess) = guard.get_mut(&session_id) {
                    sess.in_world = true;
                }
            }
        }
        ClientCommand::Move { x, y, run } => {
            if !is_in_world(ctx, session_id).await {
                tracing::warn!(%session_id, "move rejected: not in world");
                return;
            }
            route_world_command(
                ctx,
                session_id,
                map_id,
                WorldRouteCommandRequest::Move { x, y, run },
            )
            .await;
        }
        ClientCommand::Attack { target_id } => {
            if !is_in_world(ctx, session_id).await {
                tracing::warn!(%session_id, "attack rejected: not in world");
                return;
            }
            route_world_command(
                ctx,
                session_id,
                map_id,
                WorldRouteCommandRequest::Attack { target_id },
            )
            .await;
        }
        ClientCommand::CastSkill {
            skill_id,
            target_id,
        } => {
            if !is_in_world(ctx, session_id).await {
                tracing::warn!(%session_id, "cast skill rejected: not in world");
                return;
            }
            route_world_command(
                ctx,
                session_id,
                map_id,
                WorldRouteCommandRequest::CastSkill {
                    skill_id,
                    target_id,
                },
            )
            .await;
        }
        ClientCommand::PickupItem { entity_id } => {
            if !is_in_world(ctx, session_id).await {
                tracing::warn!(%session_id, "pickup rejected: not in world");
                return;
            }
            route_world_command(
                ctx,
                session_id,
                map_id,
                WorldRouteCommandRequest::PickupItem { entity_id },
            )
            .await;
        }
        ClientCommand::DropItem { slot, quantity } => {
            if !is_in_world(ctx, session_id).await {
                tracing::warn!(%session_id, "drop item rejected: not in world");
                return;
            }
            route_world_command(
                ctx,
                session_id,
                map_id,
                WorldRouteCommandRequest::DropItem { slot, quantity },
            )
            .await;
        }
        ClientCommand::UseItem { slot } => {
            if !is_in_world(ctx, session_id).await {
                tracing::warn!(%session_id, "use item rejected: not in world");
                return;
            }
            route_world_command(
                ctx,
                session_id,
                map_id,
                WorldRouteCommandRequest::UseItem { slot },
            )
            .await;
        }
        ClientCommand::NpcInteraction { npc_id } => {
            if !is_in_world(ctx, session_id).await {
                tracing::warn!(%session_id, "npc interaction rejected: not in world");
                return;
            }
            route_world_command(
                ctx,
                session_id,
                map_id,
                WorldRouteCommandRequest::NpcInteraction { npc_id },
            )
            .await;
        }
        ClientCommand::Chat { message } => {
            if !is_in_world(ctx, session_id).await {
                tracing::warn!(%session_id, "chat rejected: not in world");
                return;
            }
            route_world_command(
                ctx,
                session_id,
                map_id,
                WorldRouteCommandRequest::Chat { message },
            )
            .await;
        }
        ClientCommand::Whisper {
            to_character,
            message,
        } => {
            if !is_in_world(ctx, session_id).await {
                tracing::warn!(%session_id, "whisper rejected: not in world");
                return;
            }
            route_world_command(
                ctx,
                session_id,
                map_id,
                WorldRouteCommandRequest::Whisper {
                    to_character,
                    message,
                },
            )
            .await;
        }
        ClientCommand::GuildChat { message } => {
            if !is_in_world(ctx, session_id).await {
                tracing::warn!(%session_id, "guild chat rejected: not in world");
                return;
            }
            route_world_command(
                ctx,
                session_id,
                map_id,
                WorldRouteCommandRequest::GuildChat { message },
            )
            .await;
        }
        ClientCommand::Heartbeat => {
            route_world_command(ctx, session_id, map_id, WorldRouteCommandRequest::Heartbeat).await;
        }
        ClientCommand::Logout => {
            route_world_command(ctx, session_id, map_id, WorldRouteCommandRequest::Logout).await;
            route_auth_logout(ctx, session_id).await;
            ctx.sessions.write().await.remove(&session_id);
        }
    }
}

async fn route_auth_login(ctx: &InternalRouterContext, session_id: Uuid, payload: LoginPayload) {
    let remote_ip = ctx
        .sessions
        .read()
        .await
        .get(&session_id)
        .map(|s| s.remote_ip.clone())
        .unwrap_or_else(|| "0.0.0.0".to_string());

    let req = AuthLoginRequest {
        session_id,
        username: payload.username,
        password: payload.password,
        remote_ip,
    };

    let response = ctx
        .http_client
        .post(format!("{}/v1/auth/login", ctx.auth_base))
        .json(&req)
        .send()
        .await;

    match response {
        Ok(resp) if resp.status().is_success() => {
            let parsed = resp.json::<AuthLoginResponse>().await;
            match parsed {
                Ok(body) => {
                    if body.accepted {
                        let mut guard = ctx.sessions.write().await;
                        if let Some(sess) = guard.get_mut(&session_id) {
                            sess.account_id = body.account_id;
                            sess.character_id = None;
                            sess.map_id = None;
                            sess.in_world = false;
                        }
                        tracing::info!(%session_id, account_id = ?body.account_id, "auth login accepted");
                    } else {
                        tracing::warn!(
                            %session_id,
                            reason = ?body.reason,
                            "auth login rejected"
                        );
                    }
                }
                Err(err) => {
                    tracing::warn!(%session_id, error = ?err, "failed parsing auth response")
                }
            }
        }
        Ok(resp) => tracing::warn!(%session_id, status = %resp.status(), "auth login http error"),
        Err(err) => tracing::warn!(%session_id, error = ?err, "auth login request failed"),
    }
}

async fn route_auth_logout(ctx: &InternalRouterContext, session_id: Uuid) {
    let req = AuthLogoutRequest { session_id };
    if let Err(err) = ctx
        .http_client
        .post(format!("{}/v1/auth/logout", ctx.auth_base))
        .json(&req)
        .send()
        .await
    {
        tracing::debug!(%session_id, error = ?err, "auth logout request failed");
    }
}

async fn route_auth_character_list(ctx: &InternalRouterContext, session_id: Uuid) {
    let response = ctx
        .http_client
        .get(format!("{}/v1/auth/characters", ctx.auth_base))
        .query(&[("session_id", session_id.to_string())])
        .send()
        .await;

    match response {
        Ok(resp) if resp.status().is_success() => match resp
            .json::<AuthCharacterListResponse>()
            .await
        {
            Ok(body) => {
                if body.ok {
                    tracing::info!(%session_id, count = body.characters.len(), "character list loaded");
                } else {
                    tracing::warn!(%session_id, "character list rejected by auth");
                }
            }
            Err(err) => tracing::warn!(%session_id, error = ?err, "character list parse failed"),
        },
        Ok(resp) => {
            tracing::warn!(%session_id, status = %resp.status(), "character list http error")
        }
        Err(err) => tracing::warn!(%session_id, error = ?err, "character list request failed"),
    }
}

async fn route_auth_character_create(
    ctx: &InternalRouterContext,
    session_id: Uuid,
    payload: application::CharacterCreatePayload,
) {
    let req = AuthCharacterCreateRequest {
        session_id,
        name: payload.name,
        class: format!("{:?}", payload.class).to_ascii_lowercase(),
        gender: payload.gender as i16,
        skin_color: payload.skin_color as i16,
        hair_style: payload.hair_style as i16,
        hair_color: payload.hair_color as i16,
        underwear_color: payload.underwear_color as i16,
        stats: payload.stats,
    };

    let response = ctx
        .http_client
        .post(format!("{}/v1/auth/characters/create", ctx.auth_base))
        .json(&req)
        .send()
        .await;

    match response {
        Ok(resp) if resp.status().is_success() => match resp
            .json::<AuthCharacterCreateResponse>()
            .await
        {
            Ok(body) => {
                if body.ok {
                    if let Some(character) = body.character {
                        tracing::info!(
                            %session_id,
                            character_id = %character.character_id,
                            name = %character.name,
                            map_id = character.map_id,
                            class = %character.class,
                            x = character.x,
                            y = character.y,
                            level = character.level,
                            "character created"
                        );
                    }
                } else {
                    tracing::warn!(%session_id, reason = ?body.reason, "character create rejected");
                }
            }
            Err(err) => tracing::warn!(%session_id, error = ?err, "character create parse failed"),
        },
        Ok(resp) => {
            tracing::warn!(%session_id, status = %resp.status(), "character create http error")
        }
        Err(err) => tracing::warn!(%session_id, error = ?err, "character create request failed"),
    }
}

async fn route_auth_character_delete(
    ctx: &InternalRouterContext,
    session_id: Uuid,
    character_id: Uuid,
) {
    let req = AuthCharacterDeleteRequest {
        session_id,
        character_id,
    };

    let response = ctx
        .http_client
        .post(format!("{}/v1/auth/characters/delete", ctx.auth_base))
        .json(&req)
        .send()
        .await;

    match response {
        Ok(resp) if resp.status().is_success() => match resp
            .json::<AuthCharacterDeleteResponse>()
            .await
        {
            Ok(body) => {
                if body.ok {
                    let mut guard = ctx.sessions.write().await;
                    if let Some(sess) = guard.get_mut(&session_id) {
                        if sess.character_id == Some(character_id) {
                            sess.character_id = None;
                            sess.map_id = None;
                            sess.in_world = false;
                        }
                    }
                    tracing::info!(%session_id, %character_id, "character deleted");
                } else {
                    tracing::warn!(%session_id, %character_id, "character delete rejected");
                }
            }
            Err(err) => tracing::warn!(%session_id, error = ?err, "character delete parse failed"),
        },
        Ok(resp) => {
            tracing::warn!(%session_id, status = %resp.status(), "character delete http error")
        }
        Err(err) => tracing::warn!(%session_id, error = ?err, "character delete request failed"),
    }
}

async fn route_auth_character_select(
    ctx: &InternalRouterContext,
    session_id: Uuid,
    character_id: Uuid,
) {
    let req = AuthCharacterSelectRequest {
        session_id,
        character_id,
    };

    let response = ctx
        .http_client
        .post(format!("{}/v1/auth/characters/select", ctx.auth_base))
        .json(&req)
        .send()
        .await;

    match response {
        Ok(resp) if resp.status().is_success() => match resp
            .json::<AuthCharacterSelectResponse>()
            .await
        {
            Ok(body) => {
                if body.ok {
                    let mut guard = ctx.sessions.write().await;
                    if let Some(sess) = guard.get_mut(&session_id) {
                        sess.character_id = body.character_id;
                        sess.map_id = body.map_id;
                        sess.in_world = false;
                    }
                    tracing::info!(
                        %session_id,
                        character_id = ?body.character_id,
                        map_id = ?body.map_id,
                        x = ?body.x,
                        y = ?body.y,
                        "character selected"
                    );
                } else {
                    tracing::warn!(
                        %session_id,
                        reason = ?body.reason,
                        "character select rejected"
                    );
                }
            }
            Err(err) => tracing::warn!(%session_id, error = ?err, "character select parse failed"),
        },
        Ok(resp) => {
            tracing::warn!(%session_id, status = %resp.status(), "character select http error")
        }
        Err(err) => tracing::warn!(%session_id, error = ?err, "character select request failed"),
    }
}

async fn resolve_map_id(
    ctx: &InternalRouterContext,
    session_id: Uuid,
    map_id_hint: Option<i32>,
) -> i32 {
    if let Some(map_id) = map_id_hint {
        return map_id;
    }
    ctx.sessions
        .read()
        .await
        .get(&session_id)
        .and_then(|s| s.map_id)
        .unwrap_or(1)
}

async fn route_world_command(
    ctx: &InternalRouterContext,
    session_id: Uuid,
    map_id: i32,
    request: WorldRouteCommandRequest,
) -> bool {
    let character_id = ctx
        .sessions
        .read()
        .await
        .get(&session_id)
        .and_then(|s| s.character_id);
    let url = format!("{}/v1/world/maps/{map_id}/route", ctx.world_base);
    let envelope = WorldRouteEnvelope {
        session_id,
        character_id,
        command: request,
    };
    match ctx.http_client.post(url).json(&envelope).send().await {
        Ok(resp) if resp.status().is_success() => true,
        Ok(resp) => {
            tracing::warn!(map_id, status = %resp.status(), "world route request rejected");
            false
        }
        Err(err) => {
            tracing::warn!(map_id, error = ?err, "world route request failed");
            false
        }
    }
}

async fn is_authenticated(ctx: &InternalRouterContext, session_id: Uuid) -> bool {
    ctx.sessions
        .read()
        .await
        .get(&session_id)
        .and_then(|s| s.account_id)
        .is_some()
}

async fn is_in_world(ctx: &InternalRouterContext, session_id: Uuid) -> bool {
    ctx.sessions
        .read()
        .await
        .get(&session_id)
        .map(|s| s.in_world && s.account_id.is_some())
        .unwrap_or(false)
}

async fn is_character_selected(ctx: &InternalRouterContext, session_id: Uuid) -> bool {
    ctx.sessions
        .read()
        .await
        .get(&session_id)
        .map(|s| s.character_id.is_some() && s.account_id.is_some())
        .unwrap_or(false)
}

async fn run_game_tcp(
    bind: String,
    settings: config::Settings,
    internal_tx: mpsc::Sender<InternalCommand>,
    sessions: Arc<RwLock<HashMap<Uuid, SessionRuntime>>>,
    ip_connection_counts: Arc<RwLock<HashMap<String, usize>>>,
    redis_client: Option<redis::Client>,
) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(bind = %bind, "gateway tcp listening");

    loop {
        let (socket, remote_addr) = listener.accept().await?;
        let remote_ip = remote_addr.ip().to_string();

        let accepted = {
            let mut guard = ip_connection_counts.write().await;
            let current = guard.get(&remote_ip).copied().unwrap_or(0);
            if current >= settings.gateway_max_connections_per_ip {
                false
            } else {
                guard.insert(remote_ip.clone(), current + 1);
                true
            }
        };
        if !accepted {
            tracing::warn!(
                %remote_ip,
                limit = settings.gateway_max_connections_per_ip,
                "rejecting tcp session: per-ip connection limit exceeded"
            );
            continue;
        }

        MetricsRegistry::inc(&METRICS.active_connections);

        let session_id = Uuid::new_v4();
        sessions.write().await.insert(
            session_id,
            SessionRuntime {
                remote_ip: remote_ip.clone(),
                account_id: None,
                character_id: None,
                map_id: None,
                in_world: false,
            },
        );

        let internal_tx = internal_tx.clone();
        let settings = settings.clone();
        let sessions = sessions.clone();
        let ip_connection_counts = ip_connection_counts.clone();
        let redis_client = redis_client.clone();
        tokio::spawn(async move {
            if let Err(err) = handle_session(
                session_id,
                socket,
                remote_ip.clone(),
                settings,
                internal_tx,
                sessions,
                redis_client,
            )
            .await
            {
                tracing::warn!(%session_id, error = ?err, "session ended with error");
            }
            METRICS
                .active_connections
                .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            let mut guard = ip_connection_counts.write().await;
            let should_remove = if let Some(count) = guard.get_mut(&remote_ip) {
                if *count <= 1 {
                    true
                } else {
                    *count -= 1;
                    false
                }
            } else {
                false
            };
            if should_remove {
                guard.remove(&remote_ip);
            }
        });
    }
}

async fn handle_session(
    session_id: Uuid,
    mut socket: tokio::net::TcpStream,
    remote_ip: String,
    settings: config::Settings,
    internal_tx: mpsc::Sender<InternalCommand>,
    sessions: Arc<RwLock<HashMap<Uuid, SessionRuntime>>>,
    redis_client: Option<redis::Client>,
) -> Result<()> {
    let mut local_limiter =
        TokenBucketRateLimiter::new(settings.rate_limit_per_sec, settings.rate_limit_burst);
    let mut redis_limiter = if let Some(client) = redis_client {
        match client.get_multiplexed_async_connection().await {
            Ok(connection) => Some(connection),
            Err(err) => {
                tracing::warn!(
                    %session_id,
                    error = ?err,
                    "redis unavailable for gateway limiter; using local limiter for this session"
                );
                None
            }
        }
    } else {
        None
    };
    let mut redis_fallback_logged = false;
    let distributed_limit = settings
        .rate_limit_per_sec
        .saturating_add(settings.rate_limit_burst)
        .max(1);
    let mut phase = SessionPhase::PreAuth;
    let mut protocol_version = ProtocolVersion::LegacyV382;
    let mut read_buf = BytesMut::with_capacity(8192);

    tracing::info!(%session_id, %remote_ip, "session connected");

    loop {
        let read = match tokio::time::timeout(
            Duration::from_secs(settings.gateway_idle_timeout_seconds),
            socket.read_buf(&mut read_buf),
        )
        .await
        {
            Ok(Ok(size)) => size,
            Ok(Err(err)) => return Err(err.into()),
            Err(_) => {
                tracing::info!(
                    %session_id,
                    %remote_ip,
                    idle_timeout_seconds = settings.gateway_idle_timeout_seconds,
                    "session closed by idle timeout"
                );
                break;
            }
        };
        if read == 0 {
            break;
        }

        for frame in split_frames(&mut read_buf) {
            let allowed = if let Some(connection) = redis_limiter.as_mut() {
                match try_acquire_distributed_rate_limit(connection, &remote_ip, distributed_limit)
                    .await
                {
                    Ok(result) => result,
                    Err(err) => {
                        if !redis_fallback_logged {
                            tracing::warn!(
                                %session_id,
                                error = ?err,
                                "redis limiter failed, falling back to local limiter"
                            );
                            redis_fallback_logged = true;
                        }
                        local_limiter.try_acquire(1)
                    }
                }
            } else {
                local_limiter.try_acquire(1)
            };
            if !allowed {
                tracing::warn!(%session_id, "session rate limited");
                continue;
            }

            let packet = match decode_frame(&frame, settings.packet_max_payload) {
                Ok(packet) => packet,
                Err(err) => {
                    MetricsRegistry::inc(&METRICS.packet_decode_errors_total);
                    tracing::warn!(%session_id, error = ?err, "invalid packet frame");
                    continue;
                }
            };

            match translate_packet_for_version(&packet, phase, protocol_version) {
                Ok(command) => {
                    if let ClientCommand::Login(payload) = &command {
                        protocol_version = detect_protocol_version(payload);
                    }
                    phase = next_phase(phase, &command);
                    let _ = internal_tx
                        .send(InternalCommand::RouteClientCommand {
                            session_id,
                            map_id_hint: None,
                            command,
                        })
                        .await;
                }
                Err(err) => {
                    MetricsRegistry::inc(&METRICS.packet_decode_errors_total);
                    tracing::warn!(%session_id, error = ?err, "packet translation rejected");
                }
            }
        }
    }

    let _ = internal_tx
        .send(InternalCommand::DisconnectSession {
            session_id,
            reason: "socket_closed".to_string(),
        })
        .await;
    sessions.write().await.remove(&session_id);

    tracing::info!(%session_id, "session disconnected");
    Ok(())
}

async fn try_acquire_distributed_rate_limit(
    connection: &mut MultiplexedConnection,
    remote_ip: &str,
    per_second_limit: u32,
) -> std::result::Result<bool, redis::RedisError> {
    let unix_second = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(_) => 0,
    };
    let key = format!("hb:gateway:rate:{remote_ip}:{unix_second}");
    let (count, _): (u32, i32) = redis::pipe()
        .cmd("INCR")
        .arg(&key)
        .cmd("EXPIRE")
        .arg(&key)
        .arg(2)
        .query_async(connection)
        .await?;
    Ok(count <= per_second_limit)
}

fn next_phase(phase: SessionPhase, command: &ClientCommand) -> SessionPhase {
    match (phase, command) {
        (SessionPhase::PreAuth, ClientCommand::Login(_)) => SessionPhase::PostAuth,
        (SessionPhase::PostAuth, ClientCommand::CharacterList)
        | (SessionPhase::PostAuth, ClientCommand::CharacterCreate(_))
        | (SessionPhase::PostAuth, ClientCommand::CharacterDelete { .. })
        | (SessionPhase::PostAuth, ClientCommand::CharacterSelect { .. }) => {
            SessionPhase::InCharacterList
        }
        (SessionPhase::InCharacterList, ClientCommand::EnterWorld) => SessionPhase::InWorld,
        (_, ClientCommand::Logout) => SessionPhase::Closed,
        _ => phase,
    }
}

fn detect_protocol_version(payload: &LoginPayload) -> ProtocolVersion {
    let v = payload.client_version.trim().to_ascii_lowercase();
    if v.contains("modern") || v.contains("xtreme") {
        ProtocolVersion::ModernV400
    } else {
        ProtocolVersion::LegacyV382
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_protocol_version_prefers_modern_tokens() {
        let payload = LoginPayload {
            username: "u".to_string(),
            password: "p".to_string(),
            client_version: "Xtreme-Modern-0.0.1".to_string(),
        };
        assert_eq!(
            detect_protocol_version(&payload),
            ProtocolVersion::ModernV400
        );
    }
}
