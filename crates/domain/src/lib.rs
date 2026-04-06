use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub type AccountId = Uuid;
pub type SessionId = Uuid;
pub type CharacterId = Uuid;
pub type MapId = i32;
pub type EntityId = Uuid;
pub type GuildId = Uuid;
pub type AdminUserId = Uuid;
pub type AuditId = Uuid;

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("{field} cannot be empty")]
    EmptyField { field: &'static str },
    #[error("{field} exceeds max length {max}")]
    FieldTooLong { field: &'static str, max: usize },
    #[error("invalid stat value for {field}")]
    InvalidStatValue { field: &'static str },
    #[error("invalid slot {slot}")]
    InvalidSlot { slot: i32 },
    #[error("invalid quantity {quantity}")]
    InvalidQuantity { quantity: i32 },
}

fn normalized_text(value: &str, field: &'static str, max: usize) -> Result<String, DomainError> {
    let cleaned = value.trim();
    if cleaned.is_empty() {
        return Err(DomainError::EmptyField { field });
    }
    if cleaned.len() > max {
        return Err(DomainError::FieldTooLong { field, max });
    }
    Ok(cleaned.to_string())
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AccountStatus {
    Active,
    Suspended,
    Banned,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: AccountId,
    pub username: String,
    pub email: Option<String>,
    pub status: AccountStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Account {
    pub fn new(username: &str, email: Option<&str>) -> Result<Self, DomainError> {
        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            username: normalized_text(username, "username", 32)?,
            email: match email {
                Some(v) => Some(normalized_text(v, "email", 190)?),
                None => None,
            },
            status: AccountStatus::Active,
            created_at: now,
            updated_at: now,
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionState {
    Connecting,
    Authenticated,
    InCharacterList,
    InWorld,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub account_id: Option<AccountId>,
    pub character_id: Option<CharacterId>,
    pub remote_ip: String,
    pub gateway_node: String,
    pub state: SessionState,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CharacterClass {
    Warrior,
    Mage,
    Archer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterStats {
    pub strength: i16,
    pub vitality: i16,
    pub dexterity: i16,
    pub intelligence: i16,
    pub magic: i16,
    pub charisma: i16,
}

impl CharacterStats {
    pub fn validate(&self) -> Result<(), DomainError> {
        for (field, value) in [
            ("strength", self.strength),
            ("vitality", self.vitality),
            ("dexterity", self.dexterity),
            ("intelligence", self.intelligence),
            ("magic", self.magic),
            ("charisma", self.charisma),
        ] {
            if !(1..=255).contains(&value) {
                return Err(DomainError::InvalidStatValue { field });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character {
    pub id: CharacterId,
    pub account_id: AccountId,
    pub name: String,
    pub class: CharacterClass,
    pub map_id: MapId,
    pub x: i32,
    pub y: i32,
    pub level: i32,
    pub exp: i64,
    pub hp: i32,
    pub mp: i32,
    pub sp: i32,
    pub stats: CharacterStats,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Character {
    pub fn new(
        account_id: AccountId,
        name: &str,
        class: CharacterClass,
        map_id: MapId,
    ) -> Result<Self, DomainError> {
        let stats = CharacterStats {
            strength: 10,
            vitality: 10,
            dexterity: 10,
            intelligence: 10,
            magic: 10,
            charisma: 10,
        };
        stats.validate()?;
        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            account_id,
            name: normalized_text(name, "character_name", 16)?,
            class,
            map_id,
            x: 0,
            y: 0,
            level: 1,
            exp: 0,
            hp: 100,
            mp: 50,
            sp: 50,
            stats,
            created_at: now,
            updated_at: now,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapInfo {
    pub id: MapId,
    pub code: String,
    pub name: String,
    pub width: i32,
    pub height: i32,
    pub tick_ms: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum EntityKind {
    Player,
    Npc,
    Monster,
    ItemDrop,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: EntityId,
    pub map_id: MapId,
    pub kind: EntityKind,
    pub x: i32,
    pub y: i32,
    pub hp: i32,
    pub alive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Npc {
    pub entity: Entity,
    pub npc_code: String,
    pub behavior_tree: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemDefinition {
    pub id: i64,
    pub code: String,
    pub item_type: String,
    pub max_stack: i32,
    pub attributes: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventorySlot {
    pub slot: i32,
    pub item_id: i64,
    pub quantity: i32,
    pub metadata: serde_json::Value,
}

impl InventorySlot {
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.slot < 0 {
            return Err(DomainError::InvalidSlot { slot: self.slot });
        }
        if self.quantity <= 0 {
            return Err(DomainError::InvalidQuantity {
                quantity: self.quantity,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inventory {
    pub character_id: CharacterId,
    pub gold: i64,
    pub version: i64,
    pub slots: Vec<InventorySlot>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EquipmentSlotKind {
    Weapon,
    Shield,
    Helmet,
    Armor,
    Gloves,
    Boots,
    Accessory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquipmentPiece {
    pub slot: EquipmentSlotKind,
    pub item_id: i64,
    pub durability: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Equipment {
    pub character_id: CharacterId,
    pub pieces: Vec<EquipmentPiece>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub id: i32,
    pub code: String,
    pub display_name: String,
    pub mana_cost: i32,
    pub cooldown_ms: i32,
    pub config_json: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterSkill {
    pub character_id: CharacterId,
    pub skill_id: i32,
    pub level: i16,
    pub exp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatState {
    pub attacker: EntityId,
    pub defender: EntityId,
    pub damage: i32,
    pub was_critical: bool,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Guild {
    pub id: GuildId,
    pub name: String,
    pub notice: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub sender_character_id: CharacterId,
    pub scope: ChatScope,
    pub body: String,
    pub sent_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChatScope {
    Global,
    Map,
    Whisper,
    Guild,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailMessage {
    pub id: Uuid,
    pub from_character_id: Option<CharacterId>,
    pub to_character_id: CharacterId,
    pub subject: String,
    pub body: String,
    pub attached_item: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameEvent {
    pub id: Uuid,
    pub event_type: String,
    pub actor_character_id: Option<CharacterId>,
    pub map_id: Option<MapId>,
    pub payload: serde_json::Value,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum AdminRole {
    SuperAdmin,
    Admin,
    Gm,
    Support,
    ReadOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminAction {
    pub action_type: String,
    pub target_type: String,
    pub target_id: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditTrailEntry {
    pub id: AuditId,
    pub admin_user_id: Option<AdminUserId>,
    pub action: AdminAction,
    pub request_id: Option<String>,
    pub ip_address: Option<String>,
    pub created_at: DateTime<Utc>,
}
