#!/usr/bin/env python3
"""
Build an opcode/command report from replay_cases fixtures.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any, Dict, List, Optional, Set, Tuple

ALLOWED_ORIGINS = {"manual", "capture", "synthetic", "seed"}


def normalize_protocol(raw: str | None) -> str:
    value = (raw or "legacy_v382").strip().lower()
    if value in {"legacy", "legacy_v382"}:
        return "legacy_v382"
    if value in {"modern", "modern_v400"}:
        return "modern_v400"
    return value


def parse_frame_hex(frame_hex: str) -> bytes:
    compact = "".join(ch for ch in frame_hex if not ch.isspace())
    if len(compact) < 8 or len(compact) % 2 != 0:
        raise ValueError("invalid frame_hex")
    return bytes(int(compact[i : i + 2], 16) for i in range(0, len(compact), 2))


def read_cases(path: Path) -> List[Dict[str, Any]]:
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, list):
        raise ValueError("fixture must be a JSON array")
    return data


def build_matrix(
    cases: List[Dict[str, Any]]
) -> Tuple[Dict[str, Dict[str, Set[int]]], Dict[str, int], Dict[str, int], int]:
    matrix: Dict[str, Dict[str, Set[int]]] = {}
    counts: Dict[str, int] = {"command": 0, "decode_error": 0, "translate_error": 0}
    origin_counts: Dict[str, int] = {k: 0 for k in sorted(ALLOWED_ORIGINS)}
    modern_non_seed_commands: Set[str] = set()
    for case in cases:
        protocol = normalize_protocol(case.get("protocol_version"))
        origin = str(case.get("origin", "manual")).strip().lower()
        if origin not in ALLOWED_ORIGINS:
            origin = "manual"
        origin_counts[origin] += 1
        frame = parse_frame_hex(str(case.get("frame_hex", "")))
        opcode = int.from_bytes(frame[2:4], "little")
        expect = case.get("expect", {})
        kind = str(expect.get("kind", ""))
        if kind in counts:
            counts[kind] += 1
        if kind != "command":
            continue
        command = str(expect.get("command", "")).strip().lower()
        if not command:
            continue
        if protocol == "modern_v400" and origin in {"manual", "capture"}:
            modern_non_seed_commands.add(command)
        per_protocol = matrix.setdefault(protocol, {})
        per_protocol.setdefault(command, set()).add(opcode)
    return matrix, counts, origin_counts, len(modern_non_seed_commands)


def hex_opcode(value: int) -> str:
    return f"0x{value:04X}"


def make_markdown(
    matrix: Dict[str, Dict[str, Set[int]]],
    counts: Dict[str, int],
    origin_counts: Dict[str, int],
    modern_non_seed_command_count: int,
    source_path: Path,
) -> str:
    lines: List[str] = []
    lines.append("# Matriz de Opcodes por Version")
    lines.append("")
    lines.append(f"Fuente: `{source_path.as_posix()}`")
    lines.append("")
    lines.append(
        f"Casos: command={counts['command']}, decode_error={counts['decode_error']}, translate_error={counts['translate_error']}"
    )
    lines.append("")
    lines.append(
        "Origen: "
        + ", ".join(f"{origin}={origin_counts.get(origin, 0)}" for origin in sorted(ALLOWED_ORIGINS))
    )
    lines.append("")
    lines.append(f"modern_v400 comandos no-seed (manual/capture): {modern_non_seed_command_count}")
    lines.append("")

    all_commands: List[str] = sorted(
        {
            command
            for per_protocol in matrix.values()
            for command in per_protocol.keys()
        }
    )
    protocols: List[str] = sorted(matrix.keys())
    if not protocols:
        protocols = ["legacy_v382", "modern_v400"]

    header = "| comando | " + " | ".join(protocols) + " |"
    sep = "|---|" + "|".join("---" for _ in protocols) + "|"
    lines.append(header)
    lines.append(sep)
    for command in all_commands:
        cols: List[str] = []
        for protocol in protocols:
            opcodes = sorted(matrix.get(protocol, {}).get(command, set()))
            if not opcodes:
                cols.append("-")
            else:
                cols.append(", ".join(hex_opcode(v) for v in opcodes))
        lines.append("| " + command + " | " + " | ".join(cols) + " |")
    lines.append("")
    return "\n".join(lines)


def print_console(
    matrix: Dict[str, Dict[str, Set[int]]],
    counts: Dict[str, int],
    origin_counts: Dict[str, int],
    modern_non_seed_command_count: int,
) -> None:
    print(
        f"[replay-opcode-report] cases: command={counts['command']} decode_error={counts['decode_error']} translate_error={counts['translate_error']}"
    )
    print(
        "[replay-opcode-report] origins: "
        + ", ".join(f"{origin}={origin_counts.get(origin, 0)}" for origin in sorted(ALLOWED_ORIGINS))
    )
    print(
        f"[replay-opcode-report] modern non-seed commands (manual/capture): {modern_non_seed_command_count}"
    )
    for protocol in sorted(matrix.keys()):
        print(f"[{protocol}]")
        for command in sorted(matrix[protocol].keys()):
            opcodes = ", ".join(hex_opcode(v) for v in sorted(matrix[protocol][command]))
            print(f"  {command}: {opcodes}")


def main() -> int:
    parser = argparse.ArgumentParser(description="Build opcode matrix report from replay fixtures")
    parser.add_argument(
        "--input",
        default="crates/net/tests/fixtures/replay_cases.json",
        help="replay fixture source file",
    )
    parser.add_argument(
        "--markdown-output",
        default="docs/protocol_opcode_matrix.md",
        help="markdown report output path",
    )
    parser.add_argument(
        "--json-output",
        default="",
        help="optional json report output path",
    )
    args = parser.parse_args()

    source = Path(args.input)
    cases = read_cases(source)
    matrix, counts, origin_counts, modern_non_seed_command_count = build_matrix(cases)
    print_console(matrix, counts, origin_counts, modern_non_seed_command_count)

    markdown = make_markdown(
        matrix, counts, origin_counts, modern_non_seed_command_count, source
    )
    md_path = Path(args.markdown_output)
    md_path.parent.mkdir(parents=True, exist_ok=True)
    md_path.write_text(markdown + "\n", encoding="utf-8")
    print(f"[replay-opcode-report] markdown -> {md_path}")

    if args.json_output.strip():
        payload = {
            "source": str(source),
            "counts": counts,
            "origin_counts": origin_counts,
            "modern_non_seed_command_count": modern_non_seed_command_count,
            "matrix": {
                protocol: {cmd: sorted(vals) for cmd, vals in per.items()}
                for protocol, per in matrix.items()
            },
        }
        json_path = Path(args.json_output)
        json_path.parent.mkdir(parents=True, exist_ok=True)
        json_path.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        print(f"[replay-opcode-report] json -> {json_path}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
