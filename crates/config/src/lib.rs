use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Environment {
    Development,
    Staging,
    Production,
}

impl std::str::FromStr for Environment {
    type Err = anyhow::Error;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "dev" | "development" => Ok(Self::Development),
            "staging" => Ok(Self::Staging),
            "prod" | "production" => Ok(Self::Production),
            other => Err(anyhow!("invalid HB_ENV: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub env: Environment,
    pub log_level: String,
    pub log_json: bool,
    pub otel_endpoint: Option<String>,

    pub gateway_http_bind: String,
    pub gateway_tcp_bind: String,
    pub auth_bind: String,
    pub world_bind: String,
    pub map_bind: String,
    pub chat_bind: String,
    pub admin_bind: String,
    pub jobs_bind: String,

    pub database_url: String,
    pub database_max_conn: u32,

    pub redis_enabled: bool,
    pub redis_url: String,

    pub map_tick_ms: u64,
    pub map_command_budget: usize,

    pub session_ttl_seconds: u64,
    pub admin_session_ttl_seconds: u64,

    pub packet_max_payload: usize,
    pub rate_limit_per_sec: u32,
    pub rate_limit_burst: u32,
    pub gateway_idle_timeout_seconds: u64,
    pub gateway_max_connections_per_ip: usize,

    pub bootstrap_admin_email: String,
    pub bootstrap_admin_password: String,
}

impl Settings {
    pub fn from_env() -> Result<Self> {
        let _ = dotenvy::dotenv();

        Ok(Self {
            env: env("HB_ENV", "development").parse::<Environment>()?,
            log_level: env("HB_LOG_LEVEL", "info"),
            log_json: env_bool("HB_LOG_JSON", true),
            otel_endpoint: env_opt("HB_OTEL_ENDPOINT"),

            gateway_http_bind: env("HB_GATEWAY_HTTP_BIND", "127.0.0.1:7080"),
            gateway_tcp_bind: env("HB_GATEWAY_TCP_BIND", "0.0.0.0:2848"),
            auth_bind: env("HB_AUTH_BIND", "127.0.0.1:7101"),
            world_bind: env("HB_WORLD_BIND", "127.0.0.1:7201"),
            map_bind: env("HB_MAP_BIND", "127.0.0.1:7301"),
            chat_bind: env("HB_CHAT_BIND", "127.0.0.1:7401"),
            admin_bind: env("HB_ADMIN_BIND", "127.0.0.1:8080"),
            jobs_bind: env("HB_JOBS_BIND", "127.0.0.1:7501"),

            database_url: env(
                "HB_DATABASE_URL",
                "postgres://hb:hbpass@127.0.0.1:5432/helbreath",
            ),
            database_max_conn: env_parse("HB_DATABASE_MAX_CONN", 20)?,

            redis_enabled: env_bool("HB_REDIS_ENABLED", false),
            redis_url: env("HB_REDIS_URL", "redis://127.0.0.1:6379"),

            map_tick_ms: env_parse("HB_MAP_TICK_MS", 50)?,
            map_command_budget: env_parse("HB_MAP_COMMAND_BUDGET", 2048)?,

            session_ttl_seconds: env_parse("HB_SESSION_TTL_SECONDS", 3600)?,
            admin_session_ttl_seconds: env_parse("HB_ADMIN_SESSION_TTL_SECONDS", 1800)?,

            packet_max_payload: env_parse("HB_PACKET_MAX_PAYLOAD", 65535)?,
            rate_limit_per_sec: env_parse("HB_RATE_LIMIT_PER_SEC", 40)?,
            rate_limit_burst: env_parse("HB_RATE_LIMIT_BURST", 80)?,
            gateway_idle_timeout_seconds: env_parse("HB_GATEWAY_IDLE_TIMEOUT_SECONDS", 45)?,
            gateway_max_connections_per_ip: env_parse("HB_GATEWAY_MAX_CONNECTIONS_PER_IP", 32)?,

            bootstrap_admin_email: env("HB_BOOTSTRAP_ADMIN_EMAIL", "admin@localhost"),
            bootstrap_admin_password: env("HB_BOOTSTRAP_ADMIN_PASSWORD", "change_me_now"),
        })
    }
}

fn env(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_opt(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .and_then(|v| if v.trim().is_empty() { None } else { Some(v) })
}

fn env_bool(key: &str, default: bool) -> bool {
    match std::env::var(key) {
        Ok(value) => matches!(
            value.to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => default,
    }
}

fn env_parse<T>(key: &str, default: T) -> Result<T>
where
    T: std::str::FromStr,
    <T as std::str::FromStr>::Err: std::fmt::Display,
{
    match std::env::var(key) {
        Ok(raw) => raw
            .parse::<T>()
            .map_err(|e| anyhow!("invalid value for {key}: {e}")),
        Err(_) => Ok(default),
    }
}
