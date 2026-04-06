use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEnvelope {
    pub session_id: String,
    pub sequence: u64,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteToMap {
    pub session_id: String,
    pub map_id: i32,
    pub command_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapTickReport {
    pub map_id: i32,
    pub tick_ms: u64,
    pub overrun_count: u64,
    pub players_online: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResult {
    pub session_id: String,
    pub account_id: Option<String>,
    pub accepted: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminBroadcast {
    pub admin_user_id: String,
    pub message: String,
}
