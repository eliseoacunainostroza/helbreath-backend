#!/usr/bin/env python3
"""
Evaluate release SLO thresholds from soak JSON results.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any, Dict, List


def percentile(values: List[float], p: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    idx = int(round((p / 100.0) * (len(ordered) - 1)))
    idx = max(0, min(idx, len(ordered) - 1))
    return ordered[idx]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Evaluate release SLO using soak results")
    parser.add_argument(
        "--input",
        default="",
        help="explicit soak JSON file path (optional)",
    )
    parser.add_argument(
        "--input-glob",
        default=".smoke/reports/soak_*.json",
        help="glob used when --input is not provided",
    )
    parser.add_argument("--min-iterations", type=int, default=3)
    parser.add_argument("--max-failed-iterations", type=int, default=0)
    parser.add_argument("--min-pass-rate", type=float, default=100.0)
    parser.add_argument("--max-avg-iteration-seconds", type=float, default=30.0)
    parser.add_argument("--max-p95-iteration-seconds", type=float, default=35.0)
    return parser.parse_args()


def resolve_input(args: argparse.Namespace) -> Path:
    if args.input.strip():
        path = Path(args.input)
        if not path.exists():
            raise SystemExit(f"[slo-check] no existe archivo: {path}")
        return path

    candidates = sorted(Path(".").glob(args.input_glob))
    if not candidates:
        raise SystemExit(f"[slo-check] no hay archivos para glob: {args.input_glob}")
    return candidates[-1]


def main() -> int:
    args = parse_args()
    in_path = resolve_input(args)
    payload: Dict[str, Any] = json.loads(in_path.read_text(encoding="utf-8"))

    executed = int(payload.get("iterations_executed", 0))
    passed = int(payload.get("passed", 0))
    failed = int(payload.get("failed", 0))
    runs = payload.get("runs", [])
    durations = [
        float(item.get("duration_seconds", 0.0))
        for item in runs
        if isinstance(item, dict)
    ]
    avg_duration = (sum(durations) / len(durations)) if durations else 0.0
    p95_duration = percentile(durations, 95.0)
    pass_rate = (passed / executed * 100.0) if executed > 0 else 0.0

    print(
        "[slo-check] "
        f"file={in_path} executed={executed} passed={passed} failed={failed} "
        f"pass_rate={pass_rate:.2f}% avg={avg_duration:.2f}s p95={p95_duration:.2f}s"
    )

    violations: List[str] = []
    if executed < args.min_iterations:
        violations.append(
            f"iterations_executed={executed} < min_iterations={args.min_iterations}"
        )
    if failed > args.max_failed_iterations:
        violations.append(
            f"failed={failed} > max_failed_iterations={args.max_failed_iterations}"
        )
    if pass_rate < args.min_pass_rate:
        violations.append(
            f"pass_rate={pass_rate:.2f}% < min_pass_rate={args.min_pass_rate:.2f}%"
        )
    if avg_duration > args.max_avg_iteration_seconds:
        violations.append(
            f"avg_iteration_seconds={avg_duration:.2f} > max_avg_iteration_seconds={args.max_avg_iteration_seconds:.2f}"
        )
    if p95_duration > args.max_p95_iteration_seconds:
        violations.append(
            f"p95_iteration_seconds={p95_duration:.2f} > max_p95_iteration_seconds={args.max_p95_iteration_seconds:.2f}"
        )

    if violations:
        print("[slo-check] FAIL")
        for item in violations:
            print(f"  - {item}")
        return 1

    print("[slo-check] OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
