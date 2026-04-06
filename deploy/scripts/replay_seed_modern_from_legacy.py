#!/usr/bin/env python3
"""
Seed provisional modern_v400 command coverage from legacy_v382 cases.

This is useful while real modern captures are still limited.
"""

from __future__ import annotations

import argparse
import copy
import json
from pathlib import Path
from typing import Any, Dict, List, Set


def normalize_protocol(raw: str | None) -> str:
    value = (raw or "legacy_v382").strip().lower()
    if value in {"legacy", "legacy_v382"}:
        return "legacy_v382"
    if value in {"modern", "modern_v400"}:
        return "modern_v400"
    return value


def load_cases(path: Path) -> List[Dict[str, Any]]:
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, list):
        raise ValueError("fixture must be a JSON array")
    out: List[Dict[str, Any]] = []
    for row in data:
        if not isinstance(row, dict):
            continue
        case = dict(row)
        case["protocol_version"] = normalize_protocol(case.get("protocol_version"))
        out.append(case)
    return out


def is_command_case(case: Dict[str, Any]) -> bool:
    expect = case.get("expect")
    return isinstance(expect, dict) and expect.get("kind") == "command" and bool(expect.get("command"))


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Seed modern_v400 command cases from legacy_v382 fixtures"
    )
    parser.add_argument(
        "--input",
        default="crates/net/tests/fixtures/replay_cases.json",
        help="source replay fixture",
    )
    parser.add_argument(
        "--output",
        default="crates/net/tests/fixtures/replay_cases.json",
        help="output replay fixture",
    )
    parser.add_argument(
        "--name-suffix",
        default="modern_seed",
        help="suffix appended to generated case names",
    )
    args = parser.parse_args()

    in_path = Path(args.input)
    out_path = Path(args.output)
    cases = load_cases(in_path)

    modern_commands: Set[str] = set()
    for case in cases:
        if case.get("protocol_version") != "modern_v400":
            continue
        if not is_command_case(case):
            continue
        modern_commands.add(str(case["expect"]["command"]).strip().lower())

    legacy_candidates: Dict[str, Dict[str, Any]] = {}
    for case in cases:
        if case.get("protocol_version") != "legacy_v382":
            continue
        if not is_command_case(case):
            continue
        command = str(case["expect"]["command"]).strip().lower()
        legacy_candidates.setdefault(command, case)

    generated = 0
    for command in sorted(legacy_candidates.keys()):
        if command in modern_commands:
            continue
        source = legacy_candidates[command]
        seeded = copy.deepcopy(source)
        seeded["protocol_version"] = "modern_v400"
        seeded["name"] = f"{source.get('name', command)}_{args.name_suffix}"
        seeded["origin"] = "seed"
        cases.append(seeded)
        generated += 1

    cases.sort(key=lambda c: str(c.get("name", "")))
    out_path.write_text(json.dumps(cases, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(
        f"[replay-modern-seed] existing_modern_commands={len(modern_commands)} generated={generated} total_cases={len(cases)} -> {out_path}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
