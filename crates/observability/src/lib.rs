use anyhow::Result;
use once_cell::sync::Lazy;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing_subscriber::{fmt, EnvFilter};

pub static METRICS: Lazy<MetricsRegistry> = Lazy::new(MetricsRegistry::default);

#[derive(Debug, Default)]
pub struct MetricsRegistry {
    pub active_connections: AtomicU64,
    pub logins_total: AtomicU64,
    pub auth_failures_total: AtomicU64,
    pub packet_decode_errors_total: AtomicU64,
    pub command_queue_depth: AtomicU64,
    pub tick_duration_ms_last: AtomicU64,
    pub tick_overruns_total: AtomicU64,
    pub players_online_total: AtomicU64,
    pub db_latency_ms_last: AtomicU64,
    pub persistence_errors_total: AtomicU64,
    pub admin_actions_total: AtomicU64,
}

impl MetricsRegistry {
    pub fn inc(counter: &AtomicU64) {
        counter.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set(gauge: &AtomicU64, value: u64) {
        gauge.store(value, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            active_connections: self.active_connections.load(Ordering::Relaxed),
            logins_total: self.logins_total.load(Ordering::Relaxed),
            auth_failures_total: self.auth_failures_total.load(Ordering::Relaxed),
            packet_decode_errors_total: self.packet_decode_errors_total.load(Ordering::Relaxed),
            command_queue_depth: self.command_queue_depth.load(Ordering::Relaxed),
            tick_duration_ms_last: self.tick_duration_ms_last.load(Ordering::Relaxed),
            tick_overruns_total: self.tick_overruns_total.load(Ordering::Relaxed),
            players_online_total: self.players_online_total.load(Ordering::Relaxed),
            db_latency_ms_last: self.db_latency_ms_last.load(Ordering::Relaxed),
            persistence_errors_total: self.persistence_errors_total.load(Ordering::Relaxed),
            admin_actions_total: self.admin_actions_total.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MetricsSnapshot {
    pub active_connections: u64,
    pub logins_total: u64,
    pub auth_failures_total: u64,
    pub packet_decode_errors_total: u64,
    pub command_queue_depth: u64,
    pub tick_duration_ms_last: u64,
    pub tick_overruns_total: u64,
    pub players_online_total: u64,
    pub db_latency_ms_last: u64,
    pub persistence_errors_total: u64,
    pub admin_actions_total: u64,
}

impl MetricsSnapshot {
    pub fn to_prometheus_text(&self) -> String {
        let mut out = String::with_capacity(1024);
        let _ = writeln!(
            out,
            "# HELP hb_active_connections Active TCP connections in gateway."
        );
        let _ = writeln!(out, "# TYPE hb_active_connections gauge");
        let _ = writeln!(out, "hb_active_connections {}", self.active_connections);
        let _ = writeln!(out, "# HELP hb_logins_total Total successful auth logins.");
        let _ = writeln!(out, "# TYPE hb_logins_total counter");
        let _ = writeln!(out, "hb_logins_total {}", self.logins_total);
        let _ = writeln!(
            out,
            "# HELP hb_auth_failures_total Total failed auth attempts."
        );
        let _ = writeln!(out, "# TYPE hb_auth_failures_total counter");
        let _ = writeln!(out, "hb_auth_failures_total {}", self.auth_failures_total);
        let _ = writeln!(
            out,
            "# HELP hb_packet_decode_errors_total Total packet decode/translation errors."
        );
        let _ = writeln!(out, "# TYPE hb_packet_decode_errors_total counter");
        let _ = writeln!(
            out,
            "hb_packet_decode_errors_total {}",
            self.packet_decode_errors_total
        );
        let _ = writeln!(
            out,
            "# HELP hb_command_queue_depth Pending commands in map tick queue."
        );
        let _ = writeln!(out, "# TYPE hb_command_queue_depth gauge");
        let _ = writeln!(out, "hb_command_queue_depth {}", self.command_queue_depth);
        let _ = writeln!(
            out,
            "# HELP hb_tick_duration_ms_last Last map tick duration in milliseconds."
        );
        let _ = writeln!(out, "# TYPE hb_tick_duration_ms_last gauge");
        let _ = writeln!(
            out,
            "hb_tick_duration_ms_last {}",
            self.tick_duration_ms_last
        );
        let _ = writeln!(
            out,
            "# HELP hb_tick_overruns_total Total ticks that exceeded configured budget."
        );
        let _ = writeln!(out, "# TYPE hb_tick_overruns_total counter");
        let _ = writeln!(out, "hb_tick_overruns_total {}", self.tick_overruns_total);
        let _ = writeln!(
            out,
            "# HELP hb_players_online_total Players currently tracked as online."
        );
        let _ = writeln!(out, "# TYPE hb_players_online_total gauge");
        let _ = writeln!(out, "hb_players_online_total {}", self.players_online_total);
        let _ = writeln!(
            out,
            "# HELP hb_db_latency_ms_last Last observed DB operation latency in milliseconds."
        );
        let _ = writeln!(out, "# TYPE hb_db_latency_ms_last gauge");
        let _ = writeln!(out, "hb_db_latency_ms_last {}", self.db_latency_ms_last);
        let _ = writeln!(
            out,
            "# HELP hb_persistence_errors_total Total persistence failures in async workers."
        );
        let _ = writeln!(out, "# TYPE hb_persistence_errors_total counter");
        let _ = writeln!(
            out,
            "hb_persistence_errors_total {}",
            self.persistence_errors_total
        );
        let _ = writeln!(
            out,
            "# HELP hb_admin_actions_total Total audited admin actions."
        );
        let _ = writeln!(out, "# TYPE hb_admin_actions_total counter");
        let _ = writeln!(out, "hb_admin_actions_total {}", self.admin_actions_total);
        out
    }
}

pub fn init_tracing(service: &str, log_level: &str, json: bool) -> Result<()> {
    let filter = EnvFilter::try_new(log_level).unwrap_or_else(|_| EnvFilter::new("info"));

    let subscriber = fmt().with_env_filter(filter).with_target(true);
    if json {
        let _ = subscriber.json().try_init();
    } else {
        let _ = subscriber.try_init();
    }

    tracing::info!(service = service, "tracing initialized");
    Ok(())
}

pub fn init_opentelemetry(endpoint: Option<&str>) {
    if let Some(endpoint) = endpoint {
        if !endpoint.trim().is_empty() {
            tracing::info!(
                endpoint = endpoint,
                "otel endpoint configured (export wiring pending)"
            );
        }
    }
}

pub fn correlation_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn prometheus_text() -> String {
    METRICS.snapshot().to_prometheus_text()
}
