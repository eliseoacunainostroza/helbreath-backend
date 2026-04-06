#!/usr/bin/env python3
"""
Generate a synthetic replay_frames.bin when no game client capture is available.

The output uses Helbreath gateway framing:
- u16 length (LE), includes opcode + payload
- u16 opcode (LE)
- payload
"""

from __future__ import annotations

import argparse
from pathlib import Path
from typing import Iterable, List, Tuple


def le_i32(value: int) -> bytes:
    return int(value).to_bytes(4, "little", signed=True)


def encode_frame(opcode: int, payload: bytes) -> bytes:
    length = 2 + len(payload)
    return int(length).to_bytes(2, "little") + int(opcode).to_bytes(2, "little") + payload


def textz(*parts: str) -> bytes:
    return b"\x00".join(part.encode("utf-8") for part in parts) + b"\x00"


def uuid_blob(seed: int) -> bytes:
    # deterministic 16-byte payload for UUID-like fields in packet payloads
    return bytes(((seed + i) & 0xFF) for i in range(16))


def build_frames() -> Iterable[Tuple[str, bytes]]:
    frames: List[Tuple[str, bytes]] = [
        ("login_legacy", encode_frame(0x0001, textz("neo", "p4ss", "4.96"))),
        (
            "login_modern",
            encode_frame(0x0001, textz("neo", "p4ss", "Xtreme-Modern-0.0.1")),
        ),
        ("character_list", encode_frame(0x0002, b"")),
        ("character_create", encode_frame(0x0003, b"neo\x00\x01")),
        ("character_select", encode_frame(0x0004, uuid_blob(0x10))),
        ("character_delete", encode_frame(0x0006, uuid_blob(0x20))),
        ("enter_world", encode_frame(0x0005, b"")),
        ("heartbeat", encode_frame(0x02FE, b"")),
        ("move", encode_frame(0x0100, le_i32(12) + le_i32(24) + b"\x01")),
        ("attack", encode_frame(0x0101, uuid_blob(0x30))),
        ("cast_skill", encode_frame(0x0102, le_i32(42))),
        ("pickup_item", encode_frame(0x0103, uuid_blob(0x40))),
        ("drop_item", encode_frame(0x0104, le_i32(3) + le_i32(2))),
        ("use_item", encode_frame(0x0105, le_i32(7))),
        ("npc_interaction", encode_frame(0x0106, uuid_blob(0x50))),
        ("chat", encode_frame(0x0200, b"hola")),
        ("whisper", encode_frame(0x0201, b"neo\x00hola")),
        ("guild_chat", encode_frame(0x0202, b"guild")),
        ("logout", encode_frame(0x02FF, b"")),
        ("unknown_opcode", encode_frame(0x1234, b"")),
    ]
    return frames


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate synthetic replay_frames.bin for protocol replay tests"
    )
    parser.add_argument(
        "--output",
        default="crates/net/tests/fixtures/replay_frames.bin",
        help="output replay binary file",
    )
    args = parser.parse_args()

    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)

    frames = list(build_frames())
    output.write_bytes(b"".join(frame for _, frame in frames))
    print(
        f"[replay-synth] generated {len(frames)} frames -> {output}"
    )
    print(
        "[replay-synth] contains legacy + modern login, gameplay commands and one unknown opcode"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
