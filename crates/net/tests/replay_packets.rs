use application::ClientCommand;
use net::{decode_frame, translate_packet_for_version, ProtocolVersion, SessionPhase};
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct ReplayCase {
    name: String,
    phase: String,
    #[serde(default)]
    protocol_version: Option<String>,
    #[serde(default)]
    origin: Option<String>,
    frame_hex: String,
    expect: ReplayExpectation,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ReplayExpectation {
    Command { command: String },
    DecodeError,
    TranslateError,
}

#[test]
fn replay_cases_json_fixture() {
    let fixture = Path::new("tests/fixtures/replay_cases.json");
    let cases = load_fixture_cases(fixture);

    let mut seen_case_names = HashSet::new();
    let mut seen_commands = HashSet::new();
    let mut has_decode_error_case = false;
    let mut has_translate_error_case = false;
    let mut has_modern_case = false;
    let mut modern_non_seed_commands = HashSet::new();

    for case in cases {
        assert!(
            seen_case_names.insert(case.name.clone()),
            "duplicate replay fixture case name: {}",
            case.name
        );

        let origin = parse_origin(case.origin.as_deref());
        if case
            .protocol_version
            .as_deref()
            .is_some_and(|v| v.trim().eq_ignore_ascii_case("modern_v400"))
        {
            has_modern_case = true;
        }
        match &case.expect {
            ReplayExpectation::DecodeError => has_decode_error_case = true,
            ReplayExpectation::TranslateError => has_translate_error_case = true,
            ReplayExpectation::Command { command } => {
                seen_commands.insert(command.clone());
                let is_modern = parse_protocol_version(case.protocol_version.as_deref())
                    == ProtocolVersion::ModernV400;
                if is_modern && !matches!(origin, ReplayOrigin::Seed | ReplayOrigin::Synthetic) {
                    modern_non_seed_commands.insert(command.clone());
                }
            }
        }
        run_case(&case);
    }

    let required_commands = [
        "login",
        "character_list",
        "character_create",
        "character_delete",
        "character_select",
        "enter_world",
        "move",
        "attack",
        "cast_skill",
        "pickup_item",
        "drop_item",
        "use_item",
        "npc_interaction",
        "chat",
        "whisper",
        "guild_chat",
        "heartbeat",
        "logout",
    ];
    for command in required_commands {
        assert!(
            seen_commands.contains(command),
            "replay fixture missing command coverage for: {command}"
        );
    }
    assert!(
        has_decode_error_case,
        "replay fixture must include at least one decode_error case"
    );
    assert!(
        has_translate_error_case,
        "replay fixture must include at least one translate_error case"
    );
    assert!(
        has_modern_case,
        "replay fixture must include at least one modern_v400 case"
    );
    assert!(
        !modern_non_seed_commands.is_empty(),
        "replay fixture must include at least one modern_v400 command case with origin manual/capture"
    );
}

#[test]
fn replay_cases_legacy_v382_fixture() {
    run_protocol_fixture(
        Path::new("tests/fixtures/replay_cases_legacy_v382.json"),
        ProtocolVersion::LegacyV382,
    );
}

#[test]
fn replay_cases_modern_v400_fixture() {
    run_protocol_fixture(
        Path::new("tests/fixtures/replay_cases_modern_v400.json"),
        ProtocolVersion::ModernV400,
    );
}

#[test]
#[ignore = "optional raw capture fixture under tests/fixtures/replay_frames.bin"]
fn replay_binary_capture_fixture() {
    let fixture = Path::new("tests/fixtures/replay_frames.bin");
    let data = fs::read(fixture).expect("missing replay fixture");

    let mut offset = 0usize;
    while offset + 2 <= data.len() {
        let length = u16::from_le_bytes([data[offset], data[offset + 1]]) as usize;
        let end = offset + 2 + length;
        if end > data.len() {
            break;
        }
        let frame = &data[offset..end];
        let decoded = decode_frame(frame, 65535).expect("decode");
        let _ = translate_packet_for_version(
            &decoded,
            SessionPhase::InWorld,
            ProtocolVersion::LegacyV382,
        );
        offset = end;
    }
}

fn run_case(case: &ReplayCase) {
    let phase = parse_phase(&case.phase);
    let protocol_version = parse_protocol_version(case.protocol_version.as_deref());
    let frame = parse_hex_frame(&case.frame_hex);

    match &case.expect {
        ReplayExpectation::DecodeError => {
            let result = decode_frame(&frame, 65535);
            assert!(
                result.is_err(),
                "case={} expected decode error, got success",
                case.name
            );
        }
        ReplayExpectation::TranslateError => {
            let decoded = decode_frame(&frame, 65535)
                .unwrap_or_else(|e| panic!("case={} decode failed unexpectedly: {e}", case.name));
            let translated = translate_packet_for_version(&decoded, phase, protocol_version);
            assert!(
                translated.is_err(),
                "case={} expected translate error, got success",
                case.name
            );
        }
        ReplayExpectation::Command { command } => {
            let decoded = decode_frame(&frame, 65535)
                .unwrap_or_else(|e| panic!("case={} decode failed unexpectedly: {e}", case.name));
            let translated = translate_packet_for_version(&decoded, phase, protocol_version)
                .unwrap_or_else(|e| {
                    panic!("case={} translate failed unexpectedly: {e}", case.name)
                });
            let got = command_name(&translated);
            assert_eq!(
                got, command,
                "case={} expected command {}, got {}",
                case.name, command, got
            );
        }
    }
}

fn load_fixture_cases(path: &Path) -> Vec<ReplayCase> {
    let data = fs::read_to_string(path)
        .unwrap_or_else(|_| panic!("missing replay fixture file: {}", path.display()));
    serde_json::from_str(&data)
        .unwrap_or_else(|_| panic!("invalid replay fixture json: {}", path.display()))
}

fn run_protocol_fixture(path: &Path, expected: ProtocolVersion) {
    let cases = load_fixture_cases(path);
    assert!(
        !cases.is_empty(),
        "replay fixture must not be empty: {}",
        path.display()
    );
    let mut seen_commands = HashSet::new();
    let mut non_seed_commands = HashSet::new();
    for case in cases {
        let got = parse_protocol_version(case.protocol_version.as_deref());
        assert_eq!(
            got,
            expected,
            "fixture protocol mismatch in {} for case {}",
            path.display(),
            case.name
        );
        let origin = parse_origin(case.origin.as_deref());
        if let ReplayExpectation::Command { command } = &case.expect {
            seen_commands.insert(command.clone());
            if !matches!(origin, ReplayOrigin::Seed | ReplayOrigin::Synthetic) {
                non_seed_commands.insert(command.clone());
            }
        }
        run_case(&case);
    }
    let required_commands = [
        "login",
        "character_list",
        "character_create",
        "character_delete",
        "character_select",
        "enter_world",
        "move",
        "attack",
        "cast_skill",
        "pickup_item",
        "drop_item",
        "use_item",
        "npc_interaction",
        "chat",
        "whisper",
        "guild_chat",
        "heartbeat",
        "logout",
    ];
    for command in required_commands {
        assert!(
            seen_commands.contains(command),
            "fixture {} missing command coverage for: {command}",
            path.display()
        );
    }
    if expected == ProtocolVersion::ModernV400 {
        assert!(
            !non_seed_commands.is_empty(),
            "fixture {} must include at least one modern command from origin manual/capture",
            path.display()
        );
    }
}

fn parse_protocol_version(raw: Option<&str>) -> ProtocolVersion {
    match raw
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("legacy_v382")
        .to_ascii_lowercase()
        .as_str()
    {
        "legacy_v382" | "legacy" => ProtocolVersion::LegacyV382,
        "modern_v400" | "modern" => ProtocolVersion::ModernV400,
        other => panic!("unknown protocol_version in replay fixture: {other}"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplayOrigin {
    Manual,
    Capture,
    Synthetic,
    Seed,
}

fn parse_origin(raw: Option<&str>) -> ReplayOrigin {
    match raw
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("manual")
        .to_ascii_lowercase()
        .as_str()
    {
        "manual" => ReplayOrigin::Manual,
        "capture" => ReplayOrigin::Capture,
        "synthetic" => ReplayOrigin::Synthetic,
        "seed" => ReplayOrigin::Seed,
        other => panic!("unknown replay case origin: {other}"),
    }
}

fn parse_phase(raw: &str) -> SessionPhase {
    match raw.trim().to_ascii_lowercase().as_str() {
        "pre_auth" => SessionPhase::PreAuth,
        "post_auth" => SessionPhase::PostAuth,
        "in_character_list" => SessionPhase::InCharacterList,
        "in_world" => SessionPhase::InWorld,
        "closed" => SessionPhase::Closed,
        _ => panic!("unknown phase in replay fixture: {raw}"),
    }
}

fn parse_hex_frame(hex: &str) -> Vec<u8> {
    let compact: String = hex.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        compact.len().is_multiple_of(2),
        "hex payload length must be even"
    );

    (0..compact.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&compact[i..i + 2], 16).expect("invalid hex byte"))
        .collect()
}

fn command_name(cmd: &ClientCommand) -> &'static str {
    match cmd {
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
