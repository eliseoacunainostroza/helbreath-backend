use application::{
    validate_client_command, CharacterCreatePayload, ClientCommand, CommandValidationError,
    LoginPayload,
};
use bytes::BytesMut;
use domain::{CharacterClass, SessionState};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const OPCODE_LOGIN: u16 = 0x0001;
pub const OPCODE_CHARACTER_LIST: u16 = 0x0002;
pub const OPCODE_CHARACTER_CREATE: u16 = 0x0003;
pub const OPCODE_CHARACTER_SELECT: u16 = 0x0004;
pub const OPCODE_ENTER_WORLD: u16 = 0x0005;
pub const OPCODE_CHARACTER_DELETE: u16 = 0x0006;
pub const OPCODE_MOVE: u16 = 0x0100;
pub const OPCODE_ATTACK: u16 = 0x0101;
pub const OPCODE_CAST_SKILL: u16 = 0x0102;
pub const OPCODE_PICKUP_ITEM: u16 = 0x0103;
pub const OPCODE_DROP_ITEM: u16 = 0x0104;
pub const OPCODE_USE_ITEM: u16 = 0x0105;
pub const OPCODE_NPC_INTERACTION: u16 = 0x0106;
pub const OPCODE_CHAT: u16 = 0x0200;
pub const OPCODE_WHISPER: u16 = 0x0201;
pub const OPCODE_GUILD_CHAT: u16 = 0x0202;
pub const OPCODE_HEARTBEAT: u16 = 0x02FE;
pub const OPCODE_LOGOUT: u16 = 0x02FF;

pub const S_OPCODE_LOGIN_RESULT: u16 = 0x8001;
pub const S_OPCODE_CHARACTER_LIST_RESULT: u16 = 0x8002;
pub const S_OPCODE_CHARACTER_CREATE_RESULT: u16 = 0x8003;
pub const S_OPCODE_CHARACTER_SELECT_RESULT: u16 = 0x8004;
pub const S_OPCODE_ENTER_WORLD_RESULT: u16 = 0x8005;
pub const S_OPCODE_CHARACTER_DELETE_RESULT: u16 = 0x8006;
pub const S_OPCODE_MOVE_RESULT: u16 = 0x8100;
pub const S_OPCODE_ATTACK_RESULT: u16 = 0x8101;
pub const S_OPCODE_CAST_SKILL_RESULT: u16 = 0x8102;
pub const S_OPCODE_PICKUP_ITEM_RESULT: u16 = 0x8103;
pub const S_OPCODE_DROP_ITEM_RESULT: u16 = 0x8104;
pub const S_OPCODE_USE_ITEM_RESULT: u16 = 0x8105;
pub const S_OPCODE_NPC_INTERACTION_RESULT: u16 = 0x8106;
pub const S_OPCODE_CHAT_BROADCAST: u16 = 0x8200;
pub const S_OPCODE_WHISPER_BROADCAST: u16 = 0x8201;
pub const S_OPCODE_GUILD_CHAT_BROADCAST: u16 = 0x8202;
pub const S_OPCODE_HEARTBEAT_ACK: u16 = 0x82FE;
pub const S_OPCODE_LOGOUT_ACK: u16 = 0x82FF;
pub const S_OPCODE_ERROR: u16 = 0x8FFF;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u16)]
pub enum WireErrorCode {
    Ok = 0,
    InvalidCredentials = 1,
    SessionExpired = 2,
    CharacterNotFound = 3,
    InventoryFull = 4,
    TargetNotFound = 5,
    PermissionDenied = 6,
    RateLimited = 7,
    InvalidPayload = 8,
    InvalidSessionState = 9,
    InternalServerError = 500,
    Unknown = 0xFFFF,
}

impl WireErrorCode {
    pub const fn as_u16(self) -> u16 {
        self as u16
    }

    pub const fn from_u16(raw: u16) -> Self {
        match raw {
            0 => Self::Ok,
            1 => Self::InvalidCredentials,
            2 => Self::SessionExpired,
            3 => Self::CharacterNotFound,
            4 => Self::InventoryFull,
            5 => Self::TargetNotFound,
            6 => Self::PermissionDenied,
            7 => Self::RateLimited,
            8 => Self::InvalidPayload,
            9 => Self::InvalidSessionState,
            500 => Self::InternalServerError,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerOpcodeMatrix {
    pub login_result: u16,
    pub character_list_result: u16,
    pub character_create_result: u16,
    pub character_delete_result: u16,
    pub character_select_result: u16,
    pub enter_world_result: u16,
    pub move_result: u16,
    pub attack_result: u16,
    pub cast_skill_result: u16,
    pub pickup_item_result: u16,
    pub drop_item_result: u16,
    pub use_item_result: u16,
    pub npc_interaction_result: u16,
    pub chat_broadcast: u16,
    pub whisper_broadcast: u16,
    pub guild_chat_broadcast: u16,
    pub heartbeat_ack: u16,
    pub logout_ack: u16,
    pub error: u16,
}

impl ServerOpcodeMatrix {
    pub const fn legacy_v382() -> Self {
        Self {
            login_result: S_OPCODE_LOGIN_RESULT,
            character_list_result: S_OPCODE_CHARACTER_LIST_RESULT,
            character_create_result: S_OPCODE_CHARACTER_CREATE_RESULT,
            character_delete_result: S_OPCODE_CHARACTER_DELETE_RESULT,
            character_select_result: S_OPCODE_CHARACTER_SELECT_RESULT,
            enter_world_result: S_OPCODE_ENTER_WORLD_RESULT,
            move_result: S_OPCODE_MOVE_RESULT,
            attack_result: S_OPCODE_ATTACK_RESULT,
            cast_skill_result: S_OPCODE_CAST_SKILL_RESULT,
            pickup_item_result: S_OPCODE_PICKUP_ITEM_RESULT,
            drop_item_result: S_OPCODE_DROP_ITEM_RESULT,
            use_item_result: S_OPCODE_USE_ITEM_RESULT,
            npc_interaction_result: S_OPCODE_NPC_INTERACTION_RESULT,
            chat_broadcast: S_OPCODE_CHAT_BROADCAST,
            whisper_broadcast: S_OPCODE_WHISPER_BROADCAST,
            guild_chat_broadcast: S_OPCODE_GUILD_CHAT_BROADCAST,
            heartbeat_ack: S_OPCODE_HEARTBEAT_ACK,
            logout_ack: S_OPCODE_LOGOUT_ACK,
            error: S_OPCODE_ERROR,
        }
    }

    pub const fn modern_v400() -> Self {
        Self::legacy_v382()
    }

    pub const fn for_version(version: ProtocolVersion) -> Self {
        match version {
            ProtocolVersion::LegacyV382 => Self::legacy_v382(),
            ProtocolVersion::ModernV400 => Self::modern_v400(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ServerMessage {
    LoginResult {
        accepted: bool,
        code: WireErrorCode,
        message: Option<String>,
    },
    CharacterListResult {
        count: u16,
    },
    CharacterCreateResult {
        code: WireErrorCode,
        character_id: Option<Uuid>,
    },
    CharacterDeleteResult {
        code: WireErrorCode,
        character_id: Option<Uuid>,
    },
    CharacterSelectResult {
        code: WireErrorCode,
        character_id: Option<Uuid>,
        map_id: Option<i32>,
        x: Option<i32>,
        y: Option<i32>,
    },
    EnterWorldResult {
        code: WireErrorCode,
        map_id: Option<i32>,
        x: Option<i32>,
        y: Option<i32>,
    },
    MoveResult {
        x: i32,
        y: i32,
    },
    AttackResult {
        target_id: Uuid,
        damage: i32,
        defeated: bool,
    },
    CastSkillResult {
        skill_id: i32,
        target_id: Option<Uuid>,
        ok: bool,
    },
    PickupItemResult {
        entity_id: Uuid,
        ok: bool,
    },
    DropItemResult {
        slot: i32,
        quantity: i32,
        ok: bool,
    },
    UseItemResult {
        slot: i32,
        ok: bool,
    },
    NpcInteractionResult {
        npc_id: Uuid,
        payload: Option<String>,
    },
    ChatBroadcast {
        from: String,
        message: String,
    },
    WhisperBroadcast {
        from: String,
        message: String,
    },
    GuildChatBroadcast {
        from: String,
        message: String,
    },
    HeartbeatAck,
    LogoutAck,
    Error {
        code: WireErrorCode,
        message: Option<String>,
    },
}

#[derive(Debug, Error)]
pub enum ServerTranslateError {
    #[error("unknown server opcode 0x{0:04X}")]
    UnknownOpcode(u16),
    #[error("invalid server payload: {0}")]
    InvalidPayload(&'static str),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct WireCodecOptions {
    pub compression: bool,
    pub obfuscation_seed: Option<u8>,
}

#[derive(Debug, Error)]
pub enum WireCodecError {
    #[error("compression stream is malformed")]
    MalformedCompressionStream,
    #[error("decompressed payload too large")]
    DecompressedPayloadTooLarge,
}

#[derive(Debug, Error)]
pub enum WireDecodeError {
    #[error(transparent)]
    Frame(#[from] DecodeError),
    #[error(transparent)]
    Codec(#[from] WireCodecError),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProtocolVersion {
    LegacyV382,
    ModernV400,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpcodeMatrix {
    pub login: u16,
    pub character_list: u16,
    pub character_create: u16,
    pub character_delete: u16,
    pub character_select: u16,
    pub enter_world: u16,
    pub move_cmd: u16,
    pub attack: u16,
    pub cast_skill: u16,
    pub pickup_item: u16,
    pub drop_item: u16,
    pub use_item: u16,
    pub npc_interaction: u16,
    pub chat: u16,
    pub whisper: u16,
    pub guild_chat: u16,
    pub heartbeat: u16,
    pub logout: u16,
}

impl OpcodeMatrix {
    pub const fn legacy_v382() -> Self {
        Self {
            login: OPCODE_LOGIN,
            character_list: OPCODE_CHARACTER_LIST,
            character_create: OPCODE_CHARACTER_CREATE,
            character_delete: OPCODE_CHARACTER_DELETE,
            character_select: OPCODE_CHARACTER_SELECT,
            enter_world: OPCODE_ENTER_WORLD,
            move_cmd: OPCODE_MOVE,
            attack: OPCODE_ATTACK,
            cast_skill: OPCODE_CAST_SKILL,
            pickup_item: OPCODE_PICKUP_ITEM,
            drop_item: OPCODE_DROP_ITEM,
            use_item: OPCODE_USE_ITEM,
            npc_interaction: OPCODE_NPC_INTERACTION,
            chat: OPCODE_CHAT,
            whisper: OPCODE_WHISPER,
            guild_chat: OPCODE_GUILD_CHAT,
            heartbeat: OPCODE_HEARTBEAT,
            logout: OPCODE_LOGOUT,
        }
    }

    pub const fn modern_v400() -> Self {
        // Adapter-friendly default matrix: keeps parity with current known opcodes.
        // If modern captures diverge, override this mapping in one place.
        Self::legacy_v382()
    }

    pub const fn for_version(version: ProtocolVersion) -> Self {
        match version {
            ProtocolVersion::LegacyV382 => Self::legacy_v382(),
            ProtocolVersion::ModernV400 => Self::modern_v400(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionPhase {
    PreAuth,
    PostAuth,
    InCharacterList,
    InWorld,
    Closed,
}

impl SessionPhase {
    pub fn as_domain_state(self) -> SessionState {
        match self {
            SessionPhase::PreAuth => SessionState::Connecting,
            SessionPhase::PostAuth => SessionState::Authenticated,
            SessionPhase::InCharacterList => SessionState::InCharacterList,
            SessionPhase::InWorld => SessionState::InWorld,
            SessionPhase::Closed => SessionState::Closed,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PacketHeader {
    pub length: u16,
    pub opcode: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecodedPacket {
    pub header: PacketHeader,
    pub payload: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum DecodeError {
    #[error("frame too short")]
    FrameTooShort,
    #[error("declared length mismatch")]
    InvalidLength,
    #[error("payload too large")]
    PayloadTooLarge,
}

#[derive(Debug, Error)]
pub enum TranslateError {
    #[error("unknown opcode 0x{0:04X}")]
    UnknownOpcode(u16),
    #[error("invalid payload: {0}")]
    InvalidPayload(&'static str),
    #[error(transparent)]
    InvalidSessionState(#[from] CommandValidationError),
}

pub fn decode_frame(frame: &[u8], max_payload: usize) -> Result<DecodedPacket, DecodeError> {
    if frame.len() < 4 {
        return Err(DecodeError::FrameTooShort);
    }

    let length = u16::from_le_bytes([frame[0], frame[1]]);
    let opcode = u16::from_le_bytes([frame[2], frame[3]]);

    if usize::from(length) + 2 != frame.len() {
        return Err(DecodeError::InvalidLength);
    }

    let payload = &frame[4..];
    if payload.len() > max_payload {
        return Err(DecodeError::PayloadTooLarge);
    }

    Ok(DecodedPacket {
        header: PacketHeader { length, opcode },
        payload: payload.to_vec(),
    })
}

pub fn encode_frame(opcode: u16, payload: &[u8]) -> Vec<u8> {
    let length = (2 + payload.len()) as u16;
    let mut out = Vec::with_capacity(usize::from(length) + 2);
    out.extend_from_slice(&length.to_le_bytes());
    out.extend_from_slice(&opcode.to_le_bytes());
    out.extend_from_slice(payload);
    out
}

pub fn translate_packet(
    packet: &DecodedPacket,
    session_phase: SessionPhase,
) -> Result<ClientCommand, TranslateError> {
    translate_packet_for_version(packet, session_phase, ProtocolVersion::LegacyV382)
}

pub fn translate_packet_for_version(
    packet: &DecodedPacket,
    session_phase: SessionPhase,
    version: ProtocolVersion,
) -> Result<ClientCommand, TranslateError> {
    use ClientCommand::*;
    let op = OpcodeMatrix::for_version(version);

    let cmd = match packet.header.opcode {
        x if x == op.login => Login(parse_login_payload(&packet.payload)?),
        x if x == op.character_list => CharacterList,
        x if x == op.character_create => {
            CharacterCreate(parse_character_create_payload(&packet.payload)?)
        }
        x if x == op.character_delete => CharacterDelete {
            character_id: parse_uuid_payload(&packet.payload)?,
        },
        x if x == op.character_select => CharacterSelect {
            character_id: parse_uuid_payload(&packet.payload)?,
        },
        x if x == op.enter_world => EnterWorld,
        x if x == op.move_cmd => {
            if packet.payload.len() < 9 {
                return Err(TranslateError::InvalidPayload("move requires x,y,run"));
            }
            let x = i32::from_le_bytes([
                packet.payload[0],
                packet.payload[1],
                packet.payload[2],
                packet.payload[3],
            ]);
            let y = i32::from_le_bytes([
                packet.payload[4],
                packet.payload[5],
                packet.payload[6],
                packet.payload[7],
            ]);
            let run = packet.payload[8] != 0;
            Move { x, y, run }
        }
        x if x == op.attack => Attack {
            target_id: parse_uuid_payload(&packet.payload)?,
        },
        x if x == op.cast_skill => {
            if packet.payload.len() < 4 {
                return Err(TranslateError::InvalidPayload(
                    "cast_skill requires skill_id",
                ));
            }
            let skill_id = i32::from_le_bytes([
                packet.payload[0],
                packet.payload[1],
                packet.payload[2],
                packet.payload[3],
            ]);
            let target_id = if packet.payload.len() >= 20 {
                Some(parse_uuid_payload(&packet.payload[4..20])?)
            } else {
                None
            };
            CastSkill {
                skill_id,
                target_id,
            }
        }
        x if x == op.pickup_item => PickupItem {
            entity_id: parse_uuid_payload(&packet.payload)?,
        },
        x if x == op.drop_item => {
            if packet.payload.len() < 8 {
                return Err(TranslateError::InvalidPayload(
                    "drop_item requires slot,quantity",
                ));
            }
            let slot = i32::from_le_bytes([
                packet.payload[0],
                packet.payload[1],
                packet.payload[2],
                packet.payload[3],
            ]);
            let quantity = i32::from_le_bytes([
                packet.payload[4],
                packet.payload[5],
                packet.payload[6],
                packet.payload[7],
            ]);
            DropItem { slot, quantity }
        }
        x if x == op.use_item => {
            if packet.payload.len() < 4 {
                return Err(TranslateError::InvalidPayload("use_item requires slot"));
            }
            let slot = i32::from_le_bytes([
                packet.payload[0],
                packet.payload[1],
                packet.payload[2],
                packet.payload[3],
            ]);
            UseItem { slot }
        }
        x if x == op.npc_interaction => NpcInteraction {
            npc_id: parse_uuid_payload(&packet.payload)?,
        },
        x if x == op.chat => Chat {
            message: parse_text_payload(&packet.payload, 160)?,
        },
        x if x == op.whisper => {
            let (to_character, message) = parse_whisper_payload(&packet.payload)?;
            Whisper {
                to_character,
                message,
            }
        }
        x if x == op.guild_chat => GuildChat {
            message: parse_text_payload(&packet.payload, 160)?,
        },
        x if x == op.heartbeat => Heartbeat,
        x if x == op.logout => Logout,
        other => return Err(TranslateError::UnknownOpcode(other)),
    };

    validate_client_command(session_phase.as_domain_state(), &cmd)?;
    Ok(cmd)
}

pub fn translate_server_packet(
    packet: &DecodedPacket,
) -> Result<ServerMessage, ServerTranslateError> {
    translate_server_packet_for_version(packet, ProtocolVersion::LegacyV382)
}

pub fn translate_server_packet_for_version(
    packet: &DecodedPacket,
    version: ProtocolVersion,
) -> Result<ServerMessage, ServerTranslateError> {
    let op = ServerOpcodeMatrix::for_version(version);

    let msg = match packet.header.opcode {
        x if x == op.login_result => parse_login_result(&packet.payload)?,
        x if x == op.character_list_result => ServerMessage::CharacterListResult {
            count: parse_u16_at(&packet.payload, 0, "character_list_result requires count")?,
        },
        x if x == op.character_create_result => parse_character_create_result(&packet.payload)?,
        x if x == op.character_delete_result => parse_character_delete_result(&packet.payload)?,
        x if x == op.character_select_result => parse_character_select_result(&packet.payload)?,
        x if x == op.enter_world_result => parse_enter_world_result(&packet.payload)?,
        x if x == op.move_result => ServerMessage::MoveResult {
            x: parse_i32_at(&packet.payload, 0, "move_result requires x,y")?,
            y: parse_i32_at(&packet.payload, 4, "move_result requires x,y")?,
        },
        x if x == op.attack_result => parse_attack_result(&packet.payload)?,
        x if x == op.cast_skill_result => parse_cast_skill_result(&packet.payload)?,
        x if x == op.pickup_item_result => parse_pickup_item_result(&packet.payload)?,
        x if x == op.drop_item_result => parse_drop_item_result(&packet.payload)?,
        x if x == op.use_item_result => parse_use_item_result(&packet.payload)?,
        x if x == op.npc_interaction_result => parse_npc_interaction_result(&packet.payload)?,
        x if x == op.chat_broadcast => {
            let (from, message) = parse_sender_and_message(&packet.payload)?;
            ServerMessage::ChatBroadcast { from, message }
        }
        x if x == op.whisper_broadcast => {
            let (from, message) = parse_sender_and_message(&packet.payload)?;
            ServerMessage::WhisperBroadcast { from, message }
        }
        x if x == op.guild_chat_broadcast => {
            let (from, message) = parse_sender_and_message(&packet.payload)?;
            ServerMessage::GuildChatBroadcast { from, message }
        }
        x if x == op.heartbeat_ack => ServerMessage::HeartbeatAck,
        x if x == op.logout_ack => ServerMessage::LogoutAck,
        x if x == op.error => parse_error_result(&packet.payload)?,
        other => return Err(ServerTranslateError::UnknownOpcode(other)),
    };

    Ok(msg)
}

pub fn parse_wire_error_code(raw: u16) -> WireErrorCode {
    WireErrorCode::from_u16(raw)
}

pub fn obfuscate_wire_payload(payload: &[u8], seed: u8) -> Vec<u8> {
    payload
        .iter()
        .enumerate()
        .map(|(idx, byte)| {
            let key = seed.wrapping_add((idx as u8).wrapping_mul(31));
            byte ^ key
        })
        .collect()
}

pub fn deobfuscate_wire_payload(payload: &[u8], seed: u8) -> Vec<u8> {
    // XOR stream is symmetric.
    obfuscate_wire_payload(payload, seed)
}

pub fn compress_wire_payload(payload: &[u8]) -> Vec<u8> {
    if payload.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::with_capacity(payload.len());
    let mut idx = 0usize;
    while idx < payload.len() {
        let byte = payload[idx];
        let mut run = 1usize;
        while idx + run < payload.len() && payload[idx + run] == byte && run < 255 {
            run += 1;
        }

        if run >= 4 || byte == 0xFF {
            out.push(0xFF);
            out.push(run as u8);
            out.push(byte);
        } else {
            for _ in 0..run {
                out.push(byte);
            }
        }
        idx += run;
    }
    out
}

pub fn decompress_wire_payload(
    payload: &[u8],
    max_output: usize,
) -> Result<Vec<u8>, WireCodecError> {
    let mut out = Vec::with_capacity(payload.len());
    let mut idx = 0usize;
    while idx < payload.len() {
        if payload[idx] != 0xFF {
            out.push(payload[idx]);
            if out.len() > max_output {
                return Err(WireCodecError::DecompressedPayloadTooLarge);
            }
            idx += 1;
            continue;
        }

        if idx + 2 >= payload.len() {
            return Err(WireCodecError::MalformedCompressionStream);
        }
        let count = payload[idx + 1] as usize;
        let value = payload[idx + 2];
        if count == 0 {
            return Err(WireCodecError::MalformedCompressionStream);
        }
        if out.len().saturating_add(count) > max_output {
            return Err(WireCodecError::DecompressedPayloadTooLarge);
        }
        for _ in 0..count {
            out.push(value);
        }
        idx += 3;
    }

    Ok(out)
}

pub fn encode_wire_frame(opcode: u16, payload: &[u8], options: WireCodecOptions) -> Vec<u8> {
    let mut encoded_payload = payload.to_vec();
    if options.compression {
        encoded_payload = compress_wire_payload(&encoded_payload);
    }
    if let Some(seed) = options.obfuscation_seed {
        encoded_payload = obfuscate_wire_payload(&encoded_payload, seed);
    }
    encode_frame(opcode, &encoded_payload)
}

pub fn decode_wire_frame(
    frame: &[u8],
    max_payload: usize,
    options: WireCodecOptions,
) -> Result<DecodedPacket, WireDecodeError> {
    let decoded = decode_frame(frame, max_payload)?;
    let mut payload = decoded.payload;

    if let Some(seed) = options.obfuscation_seed {
        payload = deobfuscate_wire_payload(&payload, seed);
    }
    if options.compression {
        payload = decompress_wire_payload(&payload, max_payload)?;
    }
    if payload.len() > max_payload {
        return Err(WireCodecError::DecompressedPayloadTooLarge.into());
    }

    Ok(DecodedPacket {
        header: PacketHeader {
            length: (2 + payload.len()) as u16,
            opcode: decoded.header.opcode,
        },
        payload,
    })
}

pub struct TokenBucketRateLimiter {
    rate_per_sec: f64,
    burst: f64,
    available: f64,
    last_refill: std::time::Instant,
}

impl TokenBucketRateLimiter {
    pub fn new(rate_per_sec: u32, burst: u32) -> Self {
        let burst_f = burst as f64;
        Self {
            rate_per_sec: rate_per_sec as f64,
            burst: burst_f,
            available: burst_f,
            last_refill: std::time::Instant::now(),
        }
    }

    pub fn try_acquire(&mut self, cost: u32) -> bool {
        self.refill();
        let cost_f = cost as f64;
        if self.available >= cost_f {
            self.available -= cost_f;
            true
        } else {
            false
        }
    }

    fn refill(&mut self) {
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.last_refill = now;
        self.available = (self.available + elapsed * self.rate_per_sec).min(self.burst);
    }
}

pub fn split_frames(buffer: &mut BytesMut) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();

    loop {
        if buffer.len() < 2 {
            break;
        }

        let length = u16::from_le_bytes([buffer[0], buffer[1]]) as usize;
        let total = length + 2;
        if buffer.len() < total {
            break;
        }

        let frame = buffer.split_to(total).to_vec();
        frames.push(frame);
    }

    frames
}

fn parse_u16_at(
    payload: &[u8],
    offset: usize,
    err: &'static str,
) -> Result<u16, ServerTranslateError> {
    if payload.len() < offset + 2 {
        return Err(ServerTranslateError::InvalidPayload(err));
    }
    Ok(u16::from_le_bytes([payload[offset], payload[offset + 1]]))
}

fn parse_i32_at(
    payload: &[u8],
    offset: usize,
    err: &'static str,
) -> Result<i32, ServerTranslateError> {
    if payload.len() < offset + 4 {
        return Err(ServerTranslateError::InvalidPayload(err));
    }
    Ok(i32::from_le_bytes([
        payload[offset],
        payload[offset + 1],
        payload[offset + 2],
        payload[offset + 3],
    ]))
}

fn parse_uuid_at(
    payload: &[u8],
    offset: usize,
    err: &'static str,
) -> Result<Uuid, ServerTranslateError> {
    if payload.len() < offset + 16 {
        return Err(ServerTranslateError::InvalidPayload(err));
    }
    let mut raw = [0_u8; 16];
    raw.copy_from_slice(&payload[offset..offset + 16]);
    Ok(Uuid::from_bytes(raw))
}

fn parse_bool_at(
    payload: &[u8],
    offset: usize,
    err: &'static str,
) -> Result<bool, ServerTranslateError> {
    if payload.len() <= offset {
        return Err(ServerTranslateError::InvalidPayload(err));
    }
    Ok(payload[offset] != 0)
}

fn bytes_to_optional_server_string(
    input: &[u8],
    max: usize,
) -> Result<Option<String>, ServerTranslateError> {
    let zero_terminated = input.split(|b| *b == 0).next().unwrap_or(input);
    let text = std::str::from_utf8(zero_terminated)
        .map_err(|_| ServerTranslateError::InvalidPayload("server payload is not utf8"))?
        .trim()
        .to_string();
    if text.len() > max {
        return Err(ServerTranslateError::InvalidPayload(
            "server text payload too long",
        ));
    }
    if text.is_empty() {
        Ok(None)
    } else {
        Ok(Some(text))
    }
}

fn parse_login_result(payload: &[u8]) -> Result<ServerMessage, ServerTranslateError> {
    if payload.len() < 3 {
        return Err(ServerTranslateError::InvalidPayload(
            "login_result requires accepted + code",
        ));
    }
    let accepted = payload[0] != 0;
    let code = parse_wire_error_code(u16::from_le_bytes([payload[1], payload[2]]));
    let message = if payload.len() > 3 {
        bytes_to_optional_server_string(&payload[3..], 160)?
    } else {
        None
    };
    Ok(ServerMessage::LoginResult {
        accepted,
        code,
        message,
    })
}

fn parse_character_create_result(payload: &[u8]) -> Result<ServerMessage, ServerTranslateError> {
    let code = parse_wire_error_code(parse_u16_at(
        payload,
        0,
        "character_create_result requires code",
    )?);
    let character_id = if payload.len() >= 18 {
        Some(parse_uuid_at(
            payload,
            2,
            "character_create_result invalid character_id",
        )?)
    } else {
        None
    };
    Ok(ServerMessage::CharacterCreateResult { code, character_id })
}

fn parse_character_delete_result(payload: &[u8]) -> Result<ServerMessage, ServerTranslateError> {
    let code = parse_wire_error_code(parse_u16_at(
        payload,
        0,
        "character_delete_result requires code",
    )?);
    let character_id = if payload.len() >= 18 {
        Some(parse_uuid_at(
            payload,
            2,
            "character_delete_result invalid character_id",
        )?)
    } else {
        None
    };
    Ok(ServerMessage::CharacterDeleteResult { code, character_id })
}

fn parse_character_select_result(payload: &[u8]) -> Result<ServerMessage, ServerTranslateError> {
    let code = parse_wire_error_code(parse_u16_at(
        payload,
        0,
        "character_select_result requires code",
    )?);
    let character_id = if payload.len() >= 18 {
        Some(parse_uuid_at(
            payload,
            2,
            "character_select_result invalid character_id",
        )?)
    } else {
        None
    };
    let map_id = if payload.len() >= 22 {
        Some(parse_i32_at(
            payload,
            18,
            "character_select_result invalid map_id",
        )?)
    } else {
        None
    };
    let x = if payload.len() >= 26 {
        Some(parse_i32_at(
            payload,
            22,
            "character_select_result invalid x",
        )?)
    } else {
        None
    };
    let y = if payload.len() >= 30 {
        Some(parse_i32_at(
            payload,
            26,
            "character_select_result invalid y",
        )?)
    } else {
        None
    };
    Ok(ServerMessage::CharacterSelectResult {
        code,
        character_id,
        map_id,
        x,
        y,
    })
}

fn parse_enter_world_result(payload: &[u8]) -> Result<ServerMessage, ServerTranslateError> {
    let code = parse_wire_error_code(parse_u16_at(
        payload,
        0,
        "enter_world_result requires code",
    )?);
    let map_id = if payload.len() >= 6 {
        Some(parse_i32_at(
            payload,
            2,
            "enter_world_result invalid map_id",
        )?)
    } else {
        None
    };
    let x = if payload.len() >= 10 {
        Some(parse_i32_at(payload, 6, "enter_world_result invalid x")?)
    } else {
        None
    };
    let y = if payload.len() >= 14 {
        Some(parse_i32_at(payload, 10, "enter_world_result invalid y")?)
    } else {
        None
    };
    Ok(ServerMessage::EnterWorldResult { code, map_id, x, y })
}

fn parse_attack_result(payload: &[u8]) -> Result<ServerMessage, ServerTranslateError> {
    Ok(ServerMessage::AttackResult {
        target_id: parse_uuid_at(payload, 0, "attack_result requires target_id")?,
        damage: parse_i32_at(payload, 16, "attack_result requires damage")?,
        defeated: parse_bool_at(payload, 20, "attack_result requires defeated flag")?,
    })
}

fn parse_cast_skill_result(payload: &[u8]) -> Result<ServerMessage, ServerTranslateError> {
    let skill_id = parse_i32_at(payload, 0, "cast_skill_result requires skill_id")?;
    let ok = parse_bool_at(payload, 4, "cast_skill_result requires ok flag")?;
    let target_id = if payload.len() >= 21 {
        Some(parse_uuid_at(
            payload,
            5,
            "cast_skill_result invalid target_id",
        )?)
    } else {
        None
    };
    Ok(ServerMessage::CastSkillResult {
        skill_id,
        target_id,
        ok,
    })
}

fn parse_pickup_item_result(payload: &[u8]) -> Result<ServerMessage, ServerTranslateError> {
    Ok(ServerMessage::PickupItemResult {
        entity_id: parse_uuid_at(payload, 0, "pickup_item_result requires entity_id")?,
        ok: parse_bool_at(payload, 16, "pickup_item_result requires ok flag")?,
    })
}

fn parse_drop_item_result(payload: &[u8]) -> Result<ServerMessage, ServerTranslateError> {
    Ok(ServerMessage::DropItemResult {
        slot: parse_i32_at(payload, 0, "drop_item_result requires slot")?,
        quantity: parse_i32_at(payload, 4, "drop_item_result requires quantity")?,
        ok: parse_bool_at(payload, 8, "drop_item_result requires ok flag")?,
    })
}

fn parse_use_item_result(payload: &[u8]) -> Result<ServerMessage, ServerTranslateError> {
    Ok(ServerMessage::UseItemResult {
        slot: parse_i32_at(payload, 0, "use_item_result requires slot")?,
        ok: parse_bool_at(payload, 4, "use_item_result requires ok flag")?,
    })
}

fn parse_npc_interaction_result(payload: &[u8]) -> Result<ServerMessage, ServerTranslateError> {
    let npc_id = parse_uuid_at(payload, 0, "npc_interaction_result requires npc_id")?;
    let text = if payload.len() > 16 {
        bytes_to_optional_server_string(&payload[16..], 256)?
    } else {
        None
    };
    Ok(ServerMessage::NpcInteractionResult {
        npc_id,
        payload: text,
    })
}

fn parse_sender_and_message(payload: &[u8]) -> Result<(String, String), ServerTranslateError> {
    let mut split = payload.splitn(2, |b| *b == 0);
    let from_raw = split
        .next()
        .ok_or(ServerTranslateError::InvalidPayload("sender missing"))?;
    let message_raw = split
        .next()
        .ok_or(ServerTranslateError::InvalidPayload("message missing"))?;
    let from = bytes_to_optional_server_string(from_raw, 32)?
        .ok_or(ServerTranslateError::InvalidPayload("sender empty"))?;
    let message = bytes_to_optional_server_string(message_raw, 160)?
        .ok_or(ServerTranslateError::InvalidPayload("message empty"))?;
    Ok((from, message))
}

fn parse_error_result(payload: &[u8]) -> Result<ServerMessage, ServerTranslateError> {
    let code = parse_wire_error_code(parse_u16_at(payload, 0, "error requires code")?);
    let message = if payload.len() > 2 {
        bytes_to_optional_server_string(&payload[2..], 160)?
    } else {
        None
    };
    Ok(ServerMessage::Error { code, message })
}

fn parse_login_payload(payload: &[u8]) -> Result<LoginPayload, TranslateError> {
    let parts: Vec<&[u8]> = payload.split(|b| *b == 0).collect();
    if parts.len() < 3 {
        return Err(TranslateError::InvalidPayload(
            "login payload expects username,password,version",
        ));
    }

    Ok(LoginPayload {
        username: bytes_to_clean_string(parts[0], 32)?,
        password: bytes_to_clean_string(parts[1], 128)?,
        client_version: bytes_to_clean_string(parts[2], 24)?,
    })
}

fn parse_character_create_payload(
    payload: &[u8],
) -> Result<CharacterCreatePayload, TranslateError> {
    let parts: Vec<&[u8]> = payload.split(|b| *b == 0).collect();
    if parts.len() < 2 {
        return Err(TranslateError::InvalidPayload(
            "character_create payload is malformed",
        ));
    }

    let class = match parts.get(1).and_then(|x| x.first()).copied().unwrap_or(0) {
        0 => CharacterClass::Warrior,
        1 => CharacterClass::Mage,
        _ => CharacterClass::Archer,
    };

    Ok(CharacterCreatePayload {
        name: bytes_to_clean_string(parts[0], 16)?,
        class,
        gender: 0,
        skin_color: 0,
        hair_style: 0,
        hair_color: 0,
        underwear_color: 0,
        stats: [10, 10, 10, 10, 10, 10],
    })
}

fn parse_uuid_payload(payload: &[u8]) -> Result<Uuid, TranslateError> {
    if payload.len() >= 16 {
        let mut raw = [0_u8; 16];
        raw.copy_from_slice(&payload[..16]);
        return Ok(Uuid::from_bytes(raw));
    }

    let text = parse_text_payload(payload, 64)?;
    Uuid::parse_str(text.trim()).map_err(|_| TranslateError::InvalidPayload("invalid uuid payload"))
}

fn parse_text_payload(payload: &[u8], max: usize) -> Result<String, TranslateError> {
    bytes_to_clean_string(payload, max)
}

fn parse_whisper_payload(payload: &[u8]) -> Result<(String, String), TranslateError> {
    let mut split = payload.splitn(2, |b| *b == 0);
    let to = split
        .next()
        .ok_or(TranslateError::InvalidPayload("whisper missing recipient"))?;
    let message = split
        .next()
        .ok_or(TranslateError::InvalidPayload("whisper missing message"))?;

    Ok((
        bytes_to_clean_string(to, 32)?,
        bytes_to_clean_string(message, 160)?,
    ))
}

fn bytes_to_clean_string(input: &[u8], max: usize) -> Result<String, TranslateError> {
    let zero_terminated = input.split(|b| *b == 0).next().unwrap_or(input);
    let text = std::str::from_utf8(zero_terminated)
        .map_err(|_| TranslateError::InvalidPayload("payload is not utf8"))?
        .trim()
        .to_string();
    if text.is_empty() {
        return Err(TranslateError::InvalidPayload("text payload is empty"));
    }
    if text.len() > max {
        return Err(TranslateError::InvalidPayload("text payload too long"));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_frame_ok() {
        let frame = vec![4, 0, 1, 0, 1, 2];
        let packet = decode_frame(&frame, 1024).expect("must decode");
        assert_eq!(packet.header.length, 4);
        assert_eq!(packet.header.opcode, 1);
        assert_eq!(packet.payload, vec![1, 2]);
    }

    #[test]
    fn decode_frame_invalid_len() {
        let frame = vec![5, 0, 1, 0, 1, 2];
        let err = decode_frame(&frame, 1024).expect_err("must fail");
        assert!(matches!(err, DecodeError::InvalidLength));
    }

    #[test]
    fn token_bucket_blocks_when_exhausted() {
        let mut limiter = TokenBucketRateLimiter::new(1, 1);
        assert!(limiter.try_acquire(1));
        assert!(!limiter.try_acquire(1));
    }

    #[test]
    fn translate_character_delete_command() {
        let character_id = Uuid::new_v4();
        let packet = DecodedPacket {
            header: PacketHeader {
                length: 18,
                opcode: OPCODE_CHARACTER_DELETE,
            },
            payload: character_id.as_bytes().to_vec(),
        };

        let cmd = translate_packet(&packet, SessionPhase::InCharacterList).expect("translate");
        match cmd {
            ClientCommand::CharacterDelete { character_id: got } => assert_eq!(got, character_id),
            _ => panic!("expected CharacterDelete"),
        }
    }

    #[test]
    fn translate_with_protocol_version_adapter() {
        let frame = encode_frame(OPCODE_HEARTBEAT, &[]);
        let decoded = decode_frame(&frame, 1024).expect("decode");
        let cmd = translate_packet_for_version(
            &decoded,
            SessionPhase::InWorld,
            ProtocolVersion::ModernV400,
        )
        .expect("translate");
        assert!(matches!(cmd, ClientCommand::Heartbeat));
    }

    #[test]
    fn wire_codec_roundtrip_with_compression_and_obfuscation() {
        let payload = b"AAAAAABBBBBBBBCCCCCCCCCCCC\xFF\xFFZZZZ".to_vec();
        let options = WireCodecOptions {
            compression: true,
            obfuscation_seed: Some(0x5A),
        };
        let frame = encode_wire_frame(OPCODE_CHAT, &payload, options);
        let decoded = decode_wire_frame(&frame, 1024, options).expect("wire decode");
        assert_eq!(decoded.header.opcode, OPCODE_CHAT);
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn translate_server_login_result() {
        let mut payload = Vec::new();
        payload.push(1);
        payload.extend_from_slice(&WireErrorCode::Ok.as_u16().to_le_bytes());
        payload.extend_from_slice(b"welcome\0");
        let packet = DecodedPacket {
            header: PacketHeader {
                length: (2 + payload.len()) as u16,
                opcode: S_OPCODE_LOGIN_RESULT,
            },
            payload,
        };
        let msg =
            translate_server_packet_for_version(&packet, ProtocolVersion::LegacyV382).expect("ok");
        assert!(matches!(
            msg,
            ServerMessage::LoginResult {
                accepted: true,
                code: WireErrorCode::Ok,
                ..
            }
        ));
    }

    #[test]
    fn parse_wire_error_code_unknown() {
        assert_eq!(parse_wire_error_code(9999), WireErrorCode::Unknown);
    }
}
