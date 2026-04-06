use anyhow::Result;
use axum::{extract::State, response::IntoResponse, routing::get, Json, Router};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, serde::Serialize, Default)]
struct JobsStatus {
    ticks_total: u64,
    last_tick_at_unix: Option<i64>,
    last_closed_sessions: u64,
    last_expired_sanctions: u64,
    last_error: Option<String>,
}

#[derive(Clone)]
struct AppState {
    repo: infrastructure::PgRepository,
    status: Arc<RwLock<JobsStatus>>,
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

async fn jobs_status(State(state): State<AppState>) -> Json<JobsStatus> {
    Json(state.status.read().await.clone())
}

#[tokio::main]
async fn main() -> Result<()> {
    let settings = config::Settings::from_env()?;
    observability::init_tracing("jobs-runner", &settings.log_level, settings.log_json)?;
    observability::init_opentelemetry(settings.otel_endpoint.as_deref());

    let repo = infrastructure::PgRepository::new(&settings).await?;
    let status = Arc::new(RwLock::new(JobsStatus::default()));
    let state = AppState {
        repo: repo.clone(),
        status: status.clone(),
    };

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics))
        .route("/metrics/prometheus", get(metrics_prometheus))
        .route("/v1/jobs/status", get(jobs_status))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&settings.jobs_bind).await?;
    tracing::info!(bind = %settings.jobs_bind, "jobs-runner http listening");

    let http_task = tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, app).await {
            tracing::error!(error = ?err, "jobs-runner http crashed");
        }
    });

    let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
    tracing::info!("jobs-runner started");

    loop {
        tokio::select! {
            _ = interval.tick() => {
                match run_tick(&repo).await {
                    Ok((closed_sessions, expired_sanctions)) => {
                        let mut st = status.write().await;
                        st.ticks_total = st.ticks_total.saturating_add(1);
                        st.last_tick_at_unix = Some(chrono::Utc::now().timestamp());
                        st.last_closed_sessions = closed_sessions;
                        st.last_expired_sanctions = expired_sanctions;
                        st.last_error = None;
                    }
                    Err(err) => {
                        observability::MetricsRegistry::inc(&observability::METRICS.persistence_errors_total);
                        let mut st = status.write().await;
                        st.last_error = Some(err.to_string());
                        tracing::error!(error = ?err, "jobs tick failed");
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("jobs-runner shutdown requested");
                break;
            }
        }
    }

    http_task.abort();
    Ok(())
}

async fn run_tick(repo: &infrastructure::PgRepository) -> Result<(u64, u64)> {
    let closed_sessions = sqlx::query(
        "UPDATE sessions SET state='closed', closed_at=now() WHERE expires_at < now() AND closed_at IS NULL",
    )
    .execute(repo.pool())
    .await?
    .rows_affected();

    let expired_sanctions = sqlx::query(
        "UPDATE sanctions SET status='expired' WHERE ends_at IS NOT NULL AND ends_at < now() AND status='active'",
    )
    .execute(repo.pool())
    .await?
    .rows_affected();

    tracing::info!(closed_sessions, expired_sanctions, "jobs tick done");
    Ok((closed_sessions, expired_sanctions))
}
