use anyhow::Result;
use axum::{
    extract::{Query, State},
    response::IntoResponse,
    routing::get,
    routing::post,
    Json, Router,
};
use domain::CharacterClass;
use infrastructure::AccountRepository;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    svc: Arc<auth::AuthService<infrastructure::PgRepository>>,
    repo: infrastructure::PgRepository,
    rate_limiter: Arc<RwLock<HashMap<String, RateWindow>>>,
}

#[derive(Debug, Clone)]
struct RateWindow {
    started_at: std::time::Instant,
    attempts: u32,
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    session_id: Option<Uuid>,
    username: String,
    password: String,
    remote_ip: Option<String>,
}

#[derive(Debug, Serialize)]
struct LoginResponse {
    accepted: bool,
    account_id: Option<Uuid>,
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LogoutRequest {
    session_id: Uuid,
}

#[derive(Debug, Serialize)]
struct LogoutResponse {
    ok: bool,
}

#[derive(Debug, Deserialize)]
struct CharacterListQuery {
    session_id: Uuid,
}

#[derive(Debug, Deserialize)]
struct CharacterCreateRequest {
    session_id: Uuid,
    name: String,
    class: String,
    gender: Option<i16>,
    skin_color: Option<i16>,
    hair_style: Option<i16>,
    hair_color: Option<i16>,
    underwear_color: Option<i16>,
    stats: Option<[i16; 6]>,
}

#[derive(Debug, Deserialize)]
struct CharacterDeleteRequest {
    session_id: Uuid,
    character_id: Uuid,
}

#[derive(Debug, Deserialize)]
struct CharacterSelectRequest {
    session_id: Uuid,
    character_id: Uuid,
}

#[derive(Debug, Serialize)]
struct CharacterSummary {
    character_id: Uuid,
    name: String,
    class: String,
    map_id: i32,
    x: i32,
    y: i32,
    level: i32,
}

#[derive(Debug, Serialize)]
struct CharacterListResponse {
    ok: bool,
    characters: Vec<CharacterSummary>,
}

#[derive(Debug, Serialize)]
struct CharacterCreateResponse {
    ok: bool,
    character: Option<CharacterSummary>,
    reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct CharacterDeleteResponse {
    ok: bool,
}

#[derive(Debug, Serialize)]
struct CharacterSelectResponse {
    ok: bool,
    character_id: Option<Uuid>,
    map_id: Option<i32>,
    x: Option<i32>,
    y: Option<i32>,
    reason: Option<String>,
}

async fn healthz() -> &'static str {
    "ok"
}

async fn readyz(State(state): State<AppState>) -> Json<serde_json::Value> {
    let db_ok = state.repo.readiness_check().await.is_ok();
    Json(serde_json::json!({ "ok": db_ok }))
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

async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Json<LoginResponse> {
    let remote_ip = req.remote_ip.unwrap_or_else(|| "0.0.0.0".to_string());
    if is_rate_limited(&state, &remote_ip).await {
        observability::MetricsRegistry::inc(&observability::METRICS.auth_failures_total);
        return Json(LoginResponse {
            accepted: false,
            account_id: None,
            reason: Some("rate_limited".to_string()),
        });
    }

    let session_id = req.session_id.unwrap_or_else(Uuid::new_v4);
    match tokio::time::timeout(
        std::time::Duration::from_secs(3),
        state.svc.validate_credentials(
            session_id,
            req.username.trim(),
            req.password.as_str(),
            &remote_ip,
        ),
    )
    .await
    {
        Err(_) => Json(LoginResponse {
            accepted: false,
            account_id: None,
            reason: Some("timeout".to_string()),
        }),
        Ok(Ok(outcome)) => Json(LoginResponse {
            accepted: outcome.accepted,
            account_id: outcome.account_id,
            reason: outcome.reason,
        }),
        Ok(Err(err)) => Json(LoginResponse {
            accepted: false,
            account_id: None,
            reason: Some(format!("internal_error:{err}")),
        }),
    }
}

async fn logout(
    State(state): State<AppState>,
    Json(req): Json<LogoutRequest>,
) -> Json<LogoutResponse> {
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        state.svc.close_session(req.session_id),
    )
    .await;

    match result {
        Ok(Ok(())) => Json(LogoutResponse { ok: true }),
        Ok(Err(err)) => {
            tracing::warn!(error = ?err, session_id = %req.session_id, "logout close_session failed");
            Json(LogoutResponse { ok: false })
        }
        Err(_) => Json(LogoutResponse { ok: false }),
    }
}

async fn list_characters(
    State(state): State<AppState>,
    Query(req): Query<CharacterListQuery>,
) -> Json<CharacterListResponse> {
    let account_id = match state.repo.get_session_account(req.session_id).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return Json(CharacterListResponse {
                ok: false,
                characters: Vec::new(),
            })
        }
        Err(err) => {
            tracing::warn!(error = ?err, "list_characters: failed loading session account");
            return Json(CharacterListResponse {
                ok: false,
                characters: Vec::new(),
            });
        }
    };

    match state.repo.list_characters_for_account(account_id).await {
        Ok(characters) => Json(CharacterListResponse {
            ok: true,
            characters: characters.into_iter().map(to_character_summary).collect(),
        }),
        Err(err) => {
            tracing::warn!(error = ?err, "list_characters failed");
            Json(CharacterListResponse {
                ok: false,
                characters: Vec::new(),
            })
        }
    }
}

async fn create_character(
    State(state): State<AppState>,
    Json(req): Json<CharacterCreateRequest>,
) -> Json<CharacterCreateResponse> {
    let account_id = match state.repo.get_session_account(req.session_id).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return Json(CharacterCreateResponse {
                ok: false,
                character: None,
                reason: Some("invalid_session".to_string()),
            })
        }
        Err(err) => {
            tracing::warn!(error = ?err, "create_character: failed loading session account");
            return Json(CharacterCreateResponse {
                ok: false,
                character: None,
                reason: Some("internal_error".to_string()),
            });
        }
    };

    let class = parse_class(&req.class).unwrap_or(CharacterClass::Warrior);
    let params = infrastructure::NewCharacterParams {
        name: req.name.trim().to_string(),
        class,
        gender: req.gender.unwrap_or(0),
        skin_color: req.skin_color.unwrap_or(0),
        hair_style: req.hair_style.unwrap_or(0),
        hair_color: req.hair_color.unwrap_or(0),
        underwear_color: req.underwear_color.unwrap_or(0),
        stats: req.stats.unwrap_or([10, 10, 10, 10, 10, 10]),
    };

    match state.repo.create_character(account_id, params).await {
        Ok(character) => Json(CharacterCreateResponse {
            ok: true,
            character: Some(to_character_summary(character)),
            reason: None,
        }),
        Err(err) => {
            tracing::warn!(error = ?err, "create_character failed");
            Json(CharacterCreateResponse {
                ok: false,
                character: None,
                reason: Some(err.to_string()),
            })
        }
    }
}

async fn delete_character(
    State(state): State<AppState>,
    Json(req): Json<CharacterDeleteRequest>,
) -> Json<CharacterDeleteResponse> {
    let account_id = match state.repo.get_session_account(req.session_id).await {
        Ok(Some(id)) => id,
        Ok(None) => return Json(CharacterDeleteResponse { ok: false }),
        Err(err) => {
            tracing::warn!(error = ?err, "delete_character: failed loading session account");
            return Json(CharacterDeleteResponse { ok: false });
        }
    };

    match state
        .repo
        .delete_character(account_id, req.character_id)
        .await
    {
        Ok(ok) => Json(CharacterDeleteResponse { ok }),
        Err(err) => {
            tracing::warn!(error = ?err, "delete_character failed");
            Json(CharacterDeleteResponse { ok: false })
        }
    }
}

async fn select_character(
    State(state): State<AppState>,
    Json(req): Json<CharacterSelectRequest>,
) -> Json<CharacterSelectResponse> {
    let account_id = match state.repo.get_session_account(req.session_id).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return Json(CharacterSelectResponse {
                ok: false,
                character_id: None,
                map_id: None,
                x: None,
                y: None,
                reason: Some("invalid_session".to_string()),
            })
        }
        Err(err) => {
            tracing::warn!(error = ?err, "select_character: failed loading session account");
            return Json(CharacterSelectResponse {
                ok: false,
                character_id: None,
                map_id: None,
                x: None,
                y: None,
                reason: Some("internal_error".to_string()),
            });
        }
    };

    match state
        .repo
        .bind_session_character(req.session_id, account_id, req.character_id)
        .await
    {
        Ok(Some(character)) => Json(CharacterSelectResponse {
            ok: true,
            character_id: Some(character.id),
            map_id: Some(character.map_id),
            x: Some(character.x),
            y: Some(character.y),
            reason: None,
        }),
        Ok(None) => Json(CharacterSelectResponse {
            ok: false,
            character_id: None,
            map_id: None,
            x: None,
            y: None,
            reason: Some("character_not_found_or_session_closed".to_string()),
        }),
        Err(err) => {
            tracing::warn!(error = ?err, "select_character failed");
            Json(CharacterSelectResponse {
                ok: false,
                character_id: None,
                map_id: None,
                x: None,
                y: None,
                reason: Some("internal_error".to_string()),
            })
        }
    }
}

async fn is_rate_limited(state: &AppState, key: &str) -> bool {
    const WINDOW: std::time::Duration = std::time::Duration::from_secs(60);
    const MAX_ATTEMPTS: u32 = 30;

    let now = std::time::Instant::now();
    let mut guard = state.rate_limiter.write().await;
    let entry = guard.entry(key.to_string()).or_insert(RateWindow {
        started_at: now,
        attempts: 0,
    });

    if now.duration_since(entry.started_at) > WINDOW {
        entry.started_at = now;
        entry.attempts = 0;
    }

    entry.attempts = entry.attempts.saturating_add(1);
    entry.attempts > MAX_ATTEMPTS
}

fn parse_class(raw: &str) -> Option<CharacterClass> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "warrior" | "w" => Some(CharacterClass::Warrior),
        "mage" | "magician" | "m" => Some(CharacterClass::Mage),
        "archer" | "a" => Some(CharacterClass::Archer),
        _ => None,
    }
}

fn to_character_summary(c: domain::Character) -> CharacterSummary {
    CharacterSummary {
        character_id: c.id,
        name: c.name,
        class: match c.class {
            CharacterClass::Warrior => "warrior",
            CharacterClass::Mage => "mage",
            CharacterClass::Archer => "archer",
        }
        .to_string(),
        map_id: c.map_id,
        x: c.x,
        y: c.y,
        level: c.level,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let settings = config::Settings::from_env()?;
    observability::init_tracing("auth-service", &settings.log_level, settings.log_json)?;
    observability::init_opentelemetry(settings.otel_endpoint.as_deref());

    let repo = infrastructure::PgRepository::new(&settings).await?;
    let svc = Arc::new(auth::AuthService::new(
        repo.clone(),
        settings.session_ttl_seconds,
        "auth-service",
    ));

    let state = AppState {
        svc,
        repo,
        rate_limiter: Arc::new(RwLock::new(HashMap::new())),
    };

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics))
        .route("/metrics/prometheus", get(metrics_prometheus))
        .route("/v1/auth/login", post(login))
        .route("/v1/auth/logout", post(logout))
        .route("/v1/auth/characters", get(list_characters))
        .route("/v1/auth/characters/create", post(create_character))
        .route("/v1/auth/characters/delete", post(delete_character))
        .route("/v1/auth/characters/select", post(select_character))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&settings.auth_bind).await?;
    tracing::info!(bind = %settings.auth_bind, "auth-service listening");
    axum::serve(listener, app).await?;

    Ok(())
}
