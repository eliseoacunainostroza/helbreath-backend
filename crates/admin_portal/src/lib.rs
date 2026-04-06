use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashSet};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum Role {
    SuperAdmin,
    Admin,
    Gm,
    Support,
    ReadOnly,
}

impl Role {
    pub fn code(self) -> &'static str {
        match self {
            Role::SuperAdmin => "superadmin",
            Role::Admin => "admin",
            Role::Gm => "gm",
            Role::Support => "support",
            Role::ReadOnly => "readonly",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum Permission {
    AccountsRead,
    AccountsWrite,
    AccountsBan,
    CharactersRead,
    CharactersWrite,
    CharactersDisconnect,
    WorldRead,
    WorldWrite,
    ModerationWrite,
    BroadcastSend,
    AuditRead,
    MetricsRead,
}

impl Permission {
    pub fn code(self) -> &'static str {
        match self {
            Permission::AccountsRead => "accounts.read",
            Permission::AccountsWrite => "accounts.write",
            Permission::AccountsBan => "accounts.ban",
            Permission::CharactersRead => "characters.read",
            Permission::CharactersWrite => "characters.write",
            Permission::CharactersDisconnect => "characters.disconnect",
            Permission::WorldRead => "world.read",
            Permission::WorldWrite => "world.write",
            Permission::ModerationWrite => "moderation.write",
            Permission::BroadcastSend => "broadcast.send",
            Permission::AuditRead => "audit.read",
            Permission::MetricsRead => "metrics.read",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminPrincipal {
    pub admin_user_id: Uuid,
    pub email: String,
    pub roles: BTreeSet<Role>,
}

impl AdminPrincipal {
    pub fn effective_permissions(&self) -> HashSet<Permission> {
        self.roles
            .iter()
            .flat_map(|role| permissions_for_role(*role).iter().copied())
            .collect()
    }

    pub fn has_permission(&self, needed: Permission) -> bool {
        self.effective_permissions().contains(&needed)
    }
}

pub fn permissions_for_role(role: Role) -> &'static [Permission] {
    use Permission::*;
    match role {
        Role::SuperAdmin => &[
            AccountsRead,
            AccountsWrite,
            AccountsBan,
            CharactersRead,
            CharactersWrite,
            CharactersDisconnect,
            WorldRead,
            WorldWrite,
            ModerationWrite,
            BroadcastSend,
            AuditRead,
            MetricsRead,
        ],
        Role::Admin => &[
            AccountsRead,
            AccountsWrite,
            AccountsBan,
            CharactersRead,
            CharactersWrite,
            WorldRead,
            WorldWrite,
            BroadcastSend,
            AuditRead,
            MetricsRead,
        ],
        Role::Gm => &[
            CharactersRead,
            CharactersWrite,
            CharactersDisconnect,
            WorldRead,
            WorldWrite,
            ModerationWrite,
            BroadcastSend,
            MetricsRead,
        ],
        Role::Support => &[AccountsRead, CharactersRead, ModerationWrite],
        Role::ReadOnly => &[
            AccountsRead,
            CharactersRead,
            WorldRead,
            AuditRead,
            MetricsRead,
        ],
    }
}

pub fn require_permission(
    principal: &AdminPrincipal,
    permission: Permission,
) -> Result<(), String> {
    if principal.has_permission(permission) {
        Ok(())
    } else {
        Err(format!(
            "missing permission {} for user {}",
            permission.code(),
            principal.email
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminLoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminLoginResponse {
    pub ok: bool,
    pub token: Option<String>,
    pub expires_at_unix: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardSummary {
    pub active_connections: u64,
    pub online_players: u64,
    pub players_by_map: Vec<MapPlayerCount>,
    pub avg_tick_ms: f64,
    pub tick_overruns: u64,
    pub recent_errors: Vec<String>,
    pub service_status: Vec<ServiceStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapPlayerCount {
    pub map_id: i32,
    pub players: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub service: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSummary {
    pub account_id: Uuid,
    pub username: String,
    pub status: String,
    pub failed_login_count: i32,
    pub last_login_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterSummary {
    pub character_id: Uuid,
    pub account_id: Uuid,
    pub name: String,
    pub map_id: i32,
    pub level: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModerationActionRequest {
    pub scope: String,
    pub account_id: Option<Uuid>,
    pub character_id: Option<Uuid>,
    pub reason: String,
    pub minutes: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BroadcastRequest {
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readonly_cannot_write_accounts() {
        let principal = AdminPrincipal {
            admin_user_id: Uuid::new_v4(),
            email: "ro@test".to_string(),
            roles: [Role::ReadOnly].into_iter().collect(),
        };

        assert!(require_permission(&principal, Permission::AccountsRead).is_ok());
        assert!(require_permission(&principal, Permission::AccountsWrite).is_err());
    }
}
