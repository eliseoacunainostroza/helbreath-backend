#!/usr/bin/env python3
"""
Generate a detailed legacy-protocol parity checklist with migration/proof checks.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Dict, List, Set, Tuple


COMMAND_ORDER: List[str] = [
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
]

COMMAND_PHASE: Dict[str, str] = {
    "login": "pre_auth",
    "character_list": "in_character_list",
    "character_create": "in_character_list",
    "character_delete": "in_character_list",
    "character_select": "in_character_list",
    "enter_world": "in_character_list",
    "move": "in_world",
    "attack": "in_world",
    "cast_skill": "in_world",
    "pickup_item": "in_world",
    "drop_item": "in_world",
    "use_item": "in_world",
    "npc_interaction": "in_world",
    "chat": "in_world",
    "whisper": "in_world",
    "guild_chat": "in_world",
    "heartbeat": "in_world/post_auth",
    "logout": "post_auth/in_world",
}

COMMAND_PAYLOAD: Dict[str, str] = {
    "login": "username/password/version",
    "character_list": "sin payload",
    "character_create": "name/class",
    "character_delete": "uuid character_id",
    "character_select": "uuid character_id",
    "enter_world": "sin payload",
    "move": "x/y/run",
    "attack": "uuid target_id",
    "cast_skill": "skill_id(+target opcional)",
    "pickup_item": "uuid entity_id",
    "drop_item": "slot/quantity",
    "use_item": "slot",
    "npc_interaction": "uuid npc_id",
    "chat": "message",
    "whisper": "to_character/message",
    "guild_chat": "message",
    "heartbeat": "sin payload",
    "logout": "sin payload",
}

COMMAND_PARSE_EVIDENCE: Dict[str, str] = {
    "login": "`parse_login_payload`",
    "character_list": "opcode sin payload",
    "character_create": "`parse_character_create_payload`",
    "character_delete": "`parse_uuid_payload`",
    "character_select": "`parse_uuid_payload`",
    "enter_world": "opcode sin payload",
    "move": "parse inline x/y/run",
    "attack": "`parse_uuid_payload`",
    "cast_skill": "parse inline skill_id/target",
    "pickup_item": "`parse_uuid_payload`",
    "drop_item": "parse inline slot/quantity",
    "use_item": "parse inline slot",
    "npc_interaction": "`parse_uuid_payload`",
    "chat": "`parse_text_payload`",
    "whisper": "`parse_whisper_payload`",
    "guild_chat": "`parse_text_payload`",
    "heartbeat": "opcode sin payload",
    "logout": "opcode sin payload",
}

NET_CORE_FEATURES: List[Tuple[str, bool, bool, str]] = [
    (
        "Framing binario de entrada (u16 len + u16 opcode + payload)",
        True,
        True,
        "`decode_frame` + tests unitarios",
    ),
    (
        "Validacion de largo declarado vs bytes reales",
        True,
        True,
        "`DecodeError::InvalidLength`",
    ),
    (
        "Guardrail de tamano maximo de payload",
        True,
        True,
        "`DecodeError::PayloadTooLarge`",
    ),
    (
        "Encoder de frames de salida",
        True,
        True,
        "`encode_frame` + pruebas replay",
    ),
    (
        "Matriz de opcodes por version de protocolo",
        True,
        True,
        "`OpcodeMatrix::{legacy_v382,modern_v400}`",
    ),
    (
        "Traductor packet->ClientCommand con adaptador de version",
        True,
        True,
        "`translate_packet_for_version`",
    ),
    (
        "Validacion de estado de sesion por comando",
        True,
        True,
        "`validate_client_command`",
    ),
    (
        "Rate limiting por conexion (token bucket)",
        True,
        True,
        "`TokenBucketRateLimiter` + test",
    ),
    (
        "Reensamblado incremental de frames en stream TCP",
        True,
        True,
        "`split_frames`",
    ),
    (
        "Pipeline de replay (capture/synthetic/seed/manual) para regresion",
        True,
        True,
        "`replay_packets.rs` + scripts deploy/scripts/replay_*",
    ),
]

KNOWN_GAPS: List[Tuple[str, bool, bool, str]] = [
    (
        "Decodificacion formal server->client en `crates/net` (simetria completa de protocolo)",
        True,
        True,
        "Implementado con `ServerMessage` + `translate_server_packet_for_version` + tests unitarios.",
    ),
    (
        "Capa de cifrado/obfuscacion wire legacy dedicada",
        True,
        True,
        "Implementado con `obfuscate_wire_payload`/`deobfuscate_wire_payload` + test de roundtrip.",
    ),
    (
        "Compresion/descompresion de payloads de red en capa net",
        True,
        True,
        "Implementado con `compress_wire_payload`/`decompress_wire_payload` + test de roundtrip.",
    ),
    (
        "Catalogo exhaustivo de codigos de error legacy (wire-level)",
        True,
        True,
        "Implementado con `WireErrorCode` + `parse_wire_error_code` y uso en decode server->client.",
    ),
]

SMOKE_TCP_COMMANDS: Set[str] = {
    "login",
    "character_list",
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
}


def check(value: bool) -> str:
    return "[x]" if value else "[ ]"


def load_report(path: Path) -> dict:
    payload = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(payload, dict):
        raise ValueError("invalid opcode report json")
    return payload


def as_set_map(payload: dict, key: str) -> Dict[str, Set[str]]:
    raw = payload.get(key, {})
    if not isinstance(raw, dict):
        return {}
    out: Dict[str, Set[str]] = {}
    for protocol, items in raw.items():
        if isinstance(items, list):
            out[str(protocol).lower()] = {str(item).lower() for item in items}
    return out


def as_opcode_map(payload: dict, protocol: str) -> Dict[str, int]:
    matrix = payload.get("matrix", {})
    if not isinstance(matrix, dict):
        return {}
    protocol_map = matrix.get(protocol, {})
    if not isinstance(protocol_map, dict):
        return {}
    out: Dict[str, int] = {}
    for command, opcode_list in protocol_map.items():
        if isinstance(opcode_list, list) and opcode_list:
            try:
                out[str(command).lower()] = int(opcode_list[0])
            except (TypeError, ValueError):
                pass
    return out


def opcode_hex(command: str, opcode_map: Dict[str, int]) -> str:
    value = opcode_map.get(command)
    if value is None:
        return "-"
    return f"0x{value:04X}"


def bool_to_check(value: bool) -> str:
    return check(value)


def make_markdown(report_path: Path, payload: dict) -> str:
    required_raw = payload.get("required_commands", COMMAND_ORDER)
    required = [str(x).lower() for x in required_raw]
    if not required:
        required = list(COMMAND_ORDER)

    coverage_all = as_set_map(payload, "coverage_all")
    coverage_real = as_set_map(payload, "coverage_real_manual_capture")
    coverage_capture = as_set_map(payload, "coverage_capture_only")
    legacy_all = coverage_all.get("legacy_v382", set())
    modern_all = coverage_all.get("modern_v400", set())
    legacy_real = coverage_real.get("legacy_v382", set())
    modern_real = coverage_real.get("modern_v400", set())
    legacy_capture = coverage_capture.get("legacy_v382", set())
    modern_capture = coverage_capture.get("modern_v400", set())

    legacy_opcode_map = as_opcode_map(payload, "legacy_v382")
    modern_opcode_map = as_opcode_map(payload, "modern_v400")

    lines: List[str] = []
    lines.append("# Checklist Detallado de Paridad (Componente Legacy de Protocolo)")
    lines.append("")
    lines.append(f"Fuente: `{report_path.as_posix()}`")
    lines.append("")
    lines.append(
        "Convencion: `migrada` indica implementacion en Rust; `probada_auto` indica cobertura por tests/replay; "
        "`probada_real_*` indica evidencia manual/capture; `probada_capture_*` exige origen `capture` (cliente real)."
    )
    lines.append("")

    lines.append("## 1) Infraestructura de la Componente")
    lines.append("")
    lines.append("| funcionalidad original | migrada | probada_auto | evidencia |")
    lines.append("|---|---|---|---|")
    for feature, migrated, tested, evidence in NET_CORE_FEATURES:
        lines.append(
            f"| {feature} | {bool_to_check(migrated)} | {bool_to_check(tested)} | {evidence} |"
        )
    lines.append("")

    lines.append("## 2) Matriz Detallada por Comando Legacy")
    lines.append("")
    lines.append(
        "| comando | fase_legacy | payload_legacy | opcode_legacy | opcode_modern | "
        "migrada_decode_map | migrada_parse_payload | migrada_gate_sesion | probada_auto | probada_smoke_tcp | probada_real_legacy | probada_real_modern | probada_capture_legacy | probada_capture_modern | evidencia_parse |"
    )
    lines.append("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
    for command in required:
        migrated_decode_map = command in legacy_all and command in modern_all
        migrated_parse_payload = command in legacy_all
        migrated_gate = migrated_decode_map
        auto_tested = command in legacy_all and command in modern_all
        smoke_tcp_tested = command in SMOKE_TCP_COMMANDS
        real_l = command in legacy_real
        real_m = command in modern_real
        capture_l = command in legacy_capture
        capture_m = command in modern_capture
        lines.append(
            "| "
            + f"`{command}`"
            + " | "
            + COMMAND_PHASE.get(command, "n/a")
            + " | "
            + COMMAND_PAYLOAD.get(command, "n/a")
            + " | "
            + opcode_hex(command, legacy_opcode_map)
            + " | "
            + opcode_hex(command, modern_opcode_map)
            + " | "
            + bool_to_check(migrated_decode_map)
            + " | "
            + bool_to_check(migrated_parse_payload)
            + " | "
            + bool_to_check(migrated_gate)
            + " | "
            + bool_to_check(auto_tested)
            + " | "
            + bool_to_check(smoke_tcp_tested)
            + " | "
            + bool_to_check(real_l)
            + " | "
            + bool_to_check(real_m)
            + " | "
            + bool_to_check(capture_l)
            + " | "
            + bool_to_check(capture_m)
            + " | "
            + COMMAND_PARSE_EVIDENCE.get(command, "n/a")
            + " |"
        )
    lines.append("")

    lines.append("## 3) Brechas Funcionales Relevantes vs Componente Original")
    lines.append("")
    lines.append("| funcionalidad original esperada | migrada | probada_auto | observacion |")
    lines.append("|---|---|---|---|")
    for gap, migrated, tested, note in KNOWN_GAPS:
        lines.append(
            f"| {gap} | {bool_to_check(migrated)} | {bool_to_check(tested)} | {note} |"
        )
    lines.append("")

    lines.append("## 4) Resumen de Cobertura Real")
    lines.append("")
    lines.append(f"- Comandos requeridos: {len(required)}")
    lines.append(f"- Legacy real: {len(legacy_real)}/{len(required)}")
    lines.append(f"- Modern real: {len(modern_real)}/{len(required)}")
    lines.append(f"- Legacy capture (cliente real): {len(legacy_capture)}/{len(required)}")
    lines.append(f"- Modern capture (cliente real): {len(modern_capture)}/{len(required)}")
    lines.append(
        f"- Pendientes legacy real: {', '.join(x for x in required if x not in legacy_real) or 'ninguno'}"
    )
    lines.append(
        f"- Pendientes modern real: {', '.join(x for x in required if x not in modern_real) or 'ninguno'}"
    )
    lines.append(
        f"- Pendientes legacy capture: {', '.join(x for x in required if x not in legacy_capture) or 'ninguno'}"
    )
    lines.append(
        f"- Pendientes modern capture: {', '.join(x for x in required if x not in modern_capture) or 'ninguno'}"
    )
    lines.append(
        f"- Cobertura smoke tcp gateway: {sum(1 for x in required if x in SMOKE_TCP_COMMANDS)}/{len(required)}"
    )
    lines.append("")
    lines.append("## 5) Evidencia Smoke TCP")
    lines.append("")
    lines.append("- `gateway.tcp.route-flow`: valida login/list/select/enter-world/move/logout (legacy).")
    lines.append("- `gateway.tcp.route-flow.modern`: valida flujo base modern_v400.")
    lines.append("- `gateway.tcp.command-matrix`: valida attack/cast/pickup/drop/use/npc/chat/whisper/guild/heartbeat con evidencia outbound+persistencia.")
    lines.append("")
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate detailed net legacy protocol parity checklist markdown",
    )
    parser.add_argument(
        "--input",
        default="docs/protocol_opcode_matrix.json",
        help="opcode report json input",
    )
    parser.add_argument(
        "--output",
        default="docs/net_legacy_parity_checklist.md",
        help="markdown output path",
    )
    args = parser.parse_args()

    report_path = Path(args.input)
    payload = load_report(report_path)
    markdown = make_markdown(report_path, payload)
    out = Path(args.output)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(markdown + "\n", encoding="utf-8")
    print(f"[net-parity-checklist] markdown -> {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
