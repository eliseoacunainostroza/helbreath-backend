use chrono::{DateTime, Utc};
use domain::{CharacterClass, CharacterId, EntityId, MapId, SessionState};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginPayload {
    pub username: String,
    pub password: String,
    pub client_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterCreatePayload {
    pub name: String,
    pub class: CharacterClass,
    pub gender: u8,
    pub skin_color: u8,
    pub hair_style: u8,
    pub hair_color: u8,
    pub underwear_color: u8,
    pub stats: [i16; 6],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientCommand {
    Login(LoginPayload),
    CharacterList,
    CharacterCreate(CharacterCreatePayload),
    CharacterDelete {
        character_id: CharacterId,
    },
    CharacterSelect {
        character_id: CharacterId,
    },
    EnterWorld,
    Move {
        x: i32,
        y: i32,
        run: bool,
    },
    Attack {
        target_id: EntityId,
    },
    CastSkill {
        skill_id: i32,
        target_id: Option<EntityId>,
    },
    PickupItem {
        entity_id: EntityId,
    },
    DropItem {
        slot: i32,
        quantity: i32,
    },
    UseItem {
        slot: i32,
    },
    NpcInteraction {
        npc_id: EntityId,
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
    Heartbeat,
    Logout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InternalCommand {
    RouteClientCommand {
        session_id: Uuid,
        map_id_hint: Option<MapId>,
        command: ClientCommand,
    },
    DisconnectSession {
        session_id: Uuid,
        reason: String,
    },
    Broadcast {
        from_admin: Option<String>,
        message: String,
    },
    MoveCharacter {
        character_id: CharacterId,
        target_map_id: MapId,
        x: i32,
        y: i32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DomainEvent {
    SessionAuthenticated {
        session_id: Uuid,
        account_id: Uuid,
        at: DateTime<Utc>,
    },
    CharacterLoaded {
        session_id: Uuid,
        character_id: CharacterId,
        map_id: MapId,
        at: DateTime<Utc>,
    },
    CharacterMoved {
        character_id: CharacterId,
        map_id: MapId,
        x: i32,
        y: i32,
        at: DateTime<Utc>,
    },
    CombatResolved {
        attacker: EntityId,
        defender: EntityId,
        damage: i32,
        at: DateTime<Utc>,
    },
    AdminActionLogged {
        admin_user_id: Uuid,
        action_type: String,
        target_type: String,
        target_id: String,
        at: DateTime<Utc>,
    },
}

#[derive(Debug, Error)]
pub enum CommandValidationError {
    #[error("command {command} is not allowed in state {state:?}")]
    InvalidSessionState {
        state: SessionState,
        command: &'static str,
    },
    #[error("invalid payload: {0}")]
    InvalidPayload(&'static str),
}

pub fn validate_client_command(
    state: SessionState,
    command: &ClientCommand,
) -> Result<(), CommandValidationError> {
    use ClientCommand::*;
    use SessionState::*;

    let command_name = command_name(command);

    let allowed = match state {
        Connecting => matches!(command, Login(_) | Heartbeat),
        Authenticated => matches!(
            command,
            CharacterList
                | CharacterCreate(_)
                | CharacterDelete { .. }
                | CharacterSelect { .. }
                | Logout
                | Heartbeat
        ),
        InCharacterList => matches!(
            command,
            CharacterList
                | CharacterCreate(_)
                | CharacterDelete { .. }
                | CharacterSelect { .. }
                | EnterWorld
                | Logout
                | Heartbeat
        ),
        InWorld => matches!(
            command,
            EnterWorld
                | Move { .. }
                | Attack { .. }
                | CastSkill { .. }
                | PickupItem { .. }
                | DropItem { .. }
                | UseItem { .. }
                | NpcInteraction { .. }
                | Chat { .. }
                | Whisper { .. }
                | GuildChat { .. }
                | Heartbeat
                | Logout
        ),
        Closed => false,
    };

    if !allowed {
        return Err(CommandValidationError::InvalidSessionState {
            state,
            command: command_name,
        });
    }

    if let Login(payload) = command {
        if payload.username.trim().is_empty() || payload.password.is_empty() {
            return Err(CommandValidationError::InvalidPayload(
                "username/password cannot be empty",
            ));
        }
    }

    if let Chat { message } | GuildChat { message } = command {
        if message.trim().is_empty() {
            return Err(CommandValidationError::InvalidPayload(
                "chat message cannot be empty",
            ));
        }
    }

    Ok(())
}

fn command_name(command: &ClientCommand) -> &'static str {
    match command {
        ClientCommand::Login(_) => "login",
        ClientCommand::CharacterList => "character_list",
        ClientCommand::CharacterCreate(_) => "character_create",
        ClientCommand::CharacterDelete { .. } => "character_delete",
        ClientCommand::CharacterSelect { .. } => "character_select",
        ClientCommand::EnterWorld => "enter_world",
        ClientCommand::Move { .. } => "move",
        ClientCommand::Attack { .. } => "attack",
        ClientCommand::CastSkill { .. } => "cast_skill",
        ClientCommand::PickupItem { .. } => "pickup_item",
        ClientCommand::DropItem { .. } => "drop_item",
        ClientCommand::UseItem { .. } => "use_item",
        ClientCommand::NpcInteraction { .. } => "npc_interaction",
        ClientCommand::Chat { .. } => "chat",
        ClientCommand::Whisper { .. } => "whisper",
        ClientCommand::GuildChat { .. } => "guild_chat",
        ClientCommand::Heartbeat => "heartbeat",
        ClientCommand::Logout => "logout",
    }
}

#[async_trait::async_trait]
pub trait AuthUseCase: Send + Sync {
    async fn login(&self, session_id: Uuid, payload: LoginPayload) -> anyhow::Result<Option<Uuid>>;
}

#[async_trait::async_trait]
pub trait CharacterUseCase: Send + Sync {
    async fn list_characters(&self, account_id: Uuid) -> anyhow::Result<Vec<CharacterId>>;
    async fn select_character(
        &self,
        account_id: Uuid,
        character_id: CharacterId,
    ) -> anyhow::Result<MapId>;
}

#[async_trait::async_trait]
pub trait AdminUseCase: Send + Sync {
    async fn broadcast(&self, admin_user_id: Uuid, message: String) -> anyhow::Result<()>;
    async fn move_character(
        &self,
        admin_user_id: Uuid,
        character_id: CharacterId,
        map_id: MapId,
        x: i32,
        y: i32,
    ) -> anyhow::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::SessionState;

    #[test]
    fn enter_world_is_allowed_from_character_list() {
        let result =
            validate_client_command(SessionState::InCharacterList, &ClientCommand::EnterWorld);
        assert!(result.is_ok());
    }

    #[test]
    fn move_is_rejected_from_character_list() {
        let result = validate_client_command(
            SessionState::InCharacterList,
            &ClientCommand::Move {
                x: 10,
                y: 20,
                run: false,
            },
        );
        assert!(result.is_err());
    }
}
