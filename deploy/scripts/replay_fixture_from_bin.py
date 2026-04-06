#!/usr/bin/env python3
"""
Generate a JSON replay fixture skeleton from a raw framed binary capture.

Input frame format:
- u16 length (LE), includes opcode(2) + payload bytes
- u16 opcode (LE)
- payload
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Dict, Optional


LEGACY_OPCODE_TO_COMMAND: Dict[int, str] = {
    0x0001: "login",
    0x0002: "character_list",
    0x0003: "character_create",
    0x0004: "character_select",
    0x0005: "enter_world",
    0x0006: "character_delete",
    0x0100: "move",
    0x0101: "attack",
    0x0102: "cast_skill",
    0x0103: "pickup_item",
    0x0104: "drop_item",
    0x0105: "use_item",
    0x0106: "npc_interaction",
    0x0200: "chat",
    0x0201: "whisper",
    0x0202: "guild_chat",
    0x02FE: "heartbeat",
    0x02FF: "logout",
}

# Actualmente modern_v400 comparte matriz con legacy. Si cambia, ajustar aqui.
MODERN_OPCODE_TO_COMMAND: Dict[int, str] = dict(LEGACY_OPCODE_TO_COMMAND)
ALLOWED_ORIGINS = {"manual", "capture", "synthetic", "seed"}


def command_for_opcode(opcode: int, protocol_version: str) -> Optional[str]:
    if protocol_version == "modern_v400":
        return MODERN_OPCODE_TO_COMMAND.get(opcode)
    return LEGACY_OPCODE_TO_COMMAND.get(opcode)


def phase_for_command(command: str, fallback_phase: str) -> str:
    if command == "login":
        return "pre_auth"
    if command in {"character_list", "character_create", "character_delete", "character_select"}:
        return "in_character_list"
    if command == "enter_world":
        return "in_character_list"
    if command in {"move", "attack", "cast_skill", "pickup_item", "drop_item", "use_item", "npc_interaction", "chat", "whisper", "guild_chat"}:
        return "in_world"
    if command in {"heartbeat", "logout"}:
        return fallback_phase
    return fallback_phase


def to_hex(data: bytes) -> str:
    return " ".join(f"{b:02X}" for b in data)


def iter_frames(blob: bytes):
    offset = 0
    while offset + 2 <= len(blob):
        length = int.from_bytes(blob[offset : offset + 2], "little")
        end = offset + 2 + length
        if end > len(blob):
            break
        yield offset, blob[offset:end]
        offset = end


def main() -> int:
    parser = argparse.ArgumentParser(description="Generate replay_cases JSON from replay_frames.bin")
    parser.add_argument("--input", required=True, help="path to replay_frames.bin")
    parser.add_argument("--output", required=True, help="path to replay_cases.generated.json")
    parser.add_argument(
        "--phase",
        default="in_world",
        choices=["pre_auth", "post_auth", "in_character_list", "in_world", "closed"],
        help="default phase for generated cases",
    )
    parser.add_argument(
        "--protocol-version",
        default="legacy_v382",
        choices=["legacy_v382", "modern_v400"],
        help="protocol version to store in generated cases",
    )
    parser.add_argument(
        "--expect-mode",
        default="opcode_command",
        choices=["translate_error", "opcode_command"],
        help="how to generate expected result for each captured frame",
    )
    parser.add_argument(
        "--auto-phase",
        action="store_true",
        help="auto-assign phase based on inferred command when --expect-mode=opcode_command",
    )
    parser.add_argument(
        "--origin",
        default="capture",
        choices=sorted(ALLOWED_ORIGINS),
        help="case origin metadata",
    )
    args = parser.parse_args()

    inp = Path(args.input)
    out = Path(args.output)
    blob = inp.read_bytes()

    cases = []
    for idx, frame in iter_frames(blob):
        opcode = int.from_bytes(frame[2:4], "little")
        command = command_for_opcode(opcode, args.protocol_version)
        phase = args.phase
        expect = {"kind": "translate_error"}
        if args.expect_mode == "opcode_command" and command is not None:
            expect = {"kind": "command", "command": command}
            if args.auto_phase:
                phase = phase_for_command(command, args.phase)
        cases.append(
            {
                "name": f"capture_{idx:08d}_op_{opcode:04X}",
                "phase": phase,
                "protocol_version": args.protocol_version,
                "origin": args.origin,
                "frame_hex": to_hex(frame),
                "expect": expect,
            }
        )

    out.write_text(json.dumps(cases, indent=2), encoding="utf-8")
    print(f"[replay-fixture] wrote {len(cases)} cases -> {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
