#!/usr/bin/env python3
"""
Merge replay case JSON files with dedupe and stable ordering.

Typical usage:
  python3 deploy/scripts/replay_merge_cases.py \
    --base crates/net/tests/fixtures/replay_cases.json \
    --incoming crates/net/tests/fixtures/replay_cases.generated.json \
    --output crates/net/tests/fixtures/replay_cases.json
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any, Dict, List, Tuple

ALLOWED_ORIGINS = {"manual", "capture", "synthetic", "seed"}


def normalize_frame_hex(raw: str) -> str:
    compact = "".join(ch for ch in raw if not ch.isspace()).upper()
    if len(compact) % 2 != 0:
        raise ValueError("frame_hex length must be even")
    return " ".join(compact[i : i + 2] for i in range(0, len(compact), 2))


def normalize_case(case: Dict[str, Any]) -> Dict[str, Any]:
    out = dict(case)
    out["name"] = str(out.get("name", "")).strip()
    out["phase"] = str(out.get("phase", "in_world")).strip().lower()
    protocol = str(out.get("protocol_version", "legacy_v382")).strip().lower()
    out["protocol_version"] = protocol or "legacy_v382"
    origin = str(out.get("origin", "manual")).strip().lower()
    out["origin"] = origin if origin in ALLOWED_ORIGINS else "manual"
    out["frame_hex"] = normalize_frame_hex(str(out.get("frame_hex", "")))
    return out


def case_key(case: Dict[str, Any], dedupe_by: str) -> Tuple[str, ...]:
    if dedupe_by == "name":
        return (case["name"],)
    return (case["protocol_version"], case["phase"], case["frame_hex"])


def load_cases(path: Path) -> List[Dict[str, Any]]:
    if not path.exists():
        return []
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, list):
        raise ValueError(f"{path} must contain a JSON array")
    return [normalize_case(item) for item in data]


def main() -> int:
    parser = argparse.ArgumentParser(description="Merge replay case JSON fixtures")
    parser.add_argument("--base", required=True, help="canonical replay cases JSON")
    parser.add_argument("--incoming", required=True, help="incoming generated replay cases JSON")
    parser.add_argument("--output", default=None, help="output JSON path (default: --base)")
    parser.add_argument(
        "--dedupe-by",
        choices=["frame", "name"],
        default="frame",
        help="dedupe key strategy",
    )
    parser.add_argument(
        "--prefer",
        choices=["incoming", "base"],
        default="incoming",
        help="which side wins on duplicate key",
    )
    args = parser.parse_args()

    base_path = Path(args.base)
    incoming_path = Path(args.incoming)
    output_path = Path(args.output) if args.output else base_path

    base_cases = load_cases(base_path)
    incoming_cases = load_cases(incoming_path)

    merged: Dict[Tuple[str, ...], Dict[str, Any]] = {}
    order = [("base", base_cases), ("incoming", incoming_cases)]
    if args.prefer == "base":
        order = [("incoming", incoming_cases), ("base", base_cases)]

    for _, group in order:
        for case in group:
            if not case["name"]:
                raise ValueError("case with empty name found")
            key = case_key(case, args.dedupe_by)
            merged[key] = case

    merged_list = sorted(
        merged.values(),
        key=lambda c: (c["name"], c["protocol_version"], c["phase"]),
    )
    output_path.write_text(
        json.dumps(merged_list, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    print(
        f"[replay-merge] base={len(base_cases)} incoming={len(incoming_cases)} merged={len(merged_list)} -> {output_path}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
