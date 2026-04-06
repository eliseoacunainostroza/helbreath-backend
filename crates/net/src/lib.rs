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
}
