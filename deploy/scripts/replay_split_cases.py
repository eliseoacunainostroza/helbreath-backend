#!/usr/bin/env python3
"""
Split replay_cases.json into version-specific fixtures.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any, Dict, List

ALLOWED_ORIGINS = {"manual", "capture", "synthetic", "seed"}


def normalize_protocol(raw: str | None) -> str:
    value = (raw or "legacy_v382").strip().lower()
    if value in {"legacy", "legacy_v382"}:
        return "legacy_v382"
    if value in {"modern", "modern_v400"}:
        return "modern_v400"
    raise ValueError(f"invalid protocol_version: {raw}")


def load_cases(path: Path) -> List[Dict[str, Any]]:
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, list):
        raise ValueError(f"{path} must contain a JSON array")
    out: List[Dict[str, Any]] = []
    for row in data:
        if not isinstance(row, dict):
            raise ValueError("invalid replay case object")
        case = dict(row)
        case["protocol_version"] = normalize_protocol(case.get("protocol_version"))
        origin = str(case.get("origin", "manual")).strip().lower()
        case["origin"] = origin if origin in ALLOWED_ORIGINS else "manual"
        out.append(case)
    return out


def write_cases(path: Path, cases: List[Dict[str, Any]]) -> None:
    path.write_text(json.dumps(cases, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Split replay_cases.json by protocol_version"
    )
    parser.add_argument(
        "--input",
        default="crates/net/tests/fixtures/replay_cases.json",
        help="source replay cases file",
    )
    parser.add_argument(
        "--legacy-output",
        default="crates/net/tests/fixtures/replay_cases_legacy_v382.json",
        help="legacy output file",
    )
    parser.add_argument(
        "--modern-output",
        default="crates/net/tests/fixtures/replay_cases_modern_v400.json",
        help="modern output file",
    )
    args = parser.parse_args()

    cases = load_cases(Path(args.input))
    legacy = [c for c in cases if c["protocol_version"] == "legacy_v382"]
    modern = [c for c in cases if c["protocol_version"] == "modern_v400"]

    legacy.sort(key=lambda c: c.get("name", ""))
    modern.sort(key=lambda c: c.get("name", ""))

    write_cases(Path(args.legacy_output), legacy)
    write_cases(Path(args.modern_output), modern)
    print(
        f"[replay-split] input={len(cases)} legacy={len(legacy)} modern={len(modern)}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
