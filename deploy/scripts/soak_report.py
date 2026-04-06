#!/usr/bin/env python3
"""
Generate a stability trend report from soak_test JSON artifacts.
"""

from __future__ import annotations

import argparse
import json
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Any, Iterable, List


@dataclass
class SoakRun:
    path: Path
    started_at: datetime
    finished_at: datetime
    iterations_requested: int
    iterations_executed: int
    passed: int
    failed: int
    total_duration_seconds: float

    @property
    def pass_rate(self) -> float:
        if self.iterations_executed <= 0:
            return 0.0
        return (self.passed / self.iterations_executed) * 100.0

    @property
    def avg_iteration_seconds(self) -> float:
        if self.iterations_executed <= 0:
            return 0.0
        return self.total_duration_seconds / self.iterations_executed


def parse_dt(raw: str) -> datetime:
    value = raw.strip()
    if value.endswith("Z"):
        value = value[:-1] + "+00:00"
    return datetime.fromisoformat(value)


def load_run(path: Path) -> SoakRun:
    payload = json.loads(path.read_text(encoding="utf-8"))
    runs = payload.get("runs", [])
    total_duration = 0.0
    if isinstance(runs, list):
        for item in runs:
            if isinstance(item, dict):
                total_duration += float(item.get("duration_seconds", 0.0))

    return SoakRun(
        path=path,
        started_at=parse_dt(str(payload.get("started_at", "1970-01-01T00:00:00+00:00"))),
        finished_at=parse_dt(str(payload.get("finished_at", "1970-01-01T00:00:00+00:00"))),
        iterations_requested=int(payload.get("iterations_requested", 0)),
        iterations_executed=int(payload.get("iterations_executed", 0)),
        passed=int(payload.get("passed", 0)),
        failed=int(payload.get("failed", 0)),
        total_duration_seconds=total_duration,
    )


def percentile(values: List[float], p: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    idx = int(round((p / 100.0) * (len(ordered) - 1)))
    idx = max(0, min(idx, len(ordered) - 1))
    return ordered[idx]


def load_runs(paths: Iterable[Path]) -> List[SoakRun]:
    out: List[SoakRun] = []
    for path in paths:
        try:
            out.append(load_run(path))
        except Exception as exc:  # noqa: BLE001
            print(f"[soak-report] ignorando {path}: {exc}")
    out.sort(key=lambda r: r.started_at)
    return out


def make_markdown(runs: List[SoakRun], source_glob: str) -> str:
    lines: List[str] = []
    lines.append("# Reporte de Estabilidad (Soak)")
    lines.append("")
    lines.append(f"Fuente: `{source_glob}`")
    lines.append("")

    if not runs:
        lines.append("No hay artefactos de soak para analizar.")
        lines.append("")
        return "\n".join(lines)

    total_exec = sum(r.iterations_executed for r in runs)
    total_pass = sum(r.passed for r in runs)
    total_fail = sum(r.failed for r in runs)
    overall_pass_rate = (total_pass / total_exec * 100.0) if total_exec > 0 else 0.0
    per_run_avg = [r.avg_iteration_seconds for r in runs if r.iterations_executed > 0]
    p50 = percentile(per_run_avg, 50.0)
    p95 = percentile(per_run_avg, 95.0)
    latest = runs[-1]

    lines.append("## Resumen")
    lines.append("")
    lines.append(f"- Corridas analizadas: {len(runs)}")
    lines.append(f"- Iteraciones totales: {total_exec}")
    lines.append(f"- Iteraciones OK: {total_pass}")
    lines.append(f"- Iteraciones FAIL: {total_fail}")
    lines.append(f"- Tasa de exito global: {overall_pass_rate:.2f}%")
    lines.append(f"- Latencia promedio por iteracion (p50): {p50:.2f}s")
    lines.append(f"- Latencia promedio por iteracion (p95): {p95:.2f}s")
    lines.append(
        f"- Ultima corrida: {'OK' if latest.failed == 0 else 'FAIL'} "
        f"({latest.passed}/{latest.iterations_executed})"
    )
    lines.append("")

    lines.append("## Historial")
    lines.append("")
    lines.append(
        "| inicio_utc | archivo | ejecutadas | ok | fail | tasa_exito | promedio_iteracion_s |"
    )
    lines.append("|---|---|---:|---:|---:|---:|---:|")
    for run in runs:
        lines.append(
            "| "
            + run.started_at.strftime("%Y-%m-%d %H:%M:%S")
            + " | "
            + run.path.as_posix()
            + " | "
            + str(run.iterations_executed)
            + " | "
            + str(run.passed)
            + " | "
            + str(run.failed)
            + " | "
            + f"{run.pass_rate:.2f}%"
            + " | "
            + f"{run.avg_iteration_seconds:.2f}"
            + " |"
        )
    lines.append("")
    return "\n".join(lines)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Generate soak trend markdown report")
    parser.add_argument(
        "--input-glob",
        default=".smoke/reports/soak_*.json",
        help="glob with soak JSON files",
    )
    parser.add_argument(
        "--output-md",
        default="docs/soak_stability.md",
        help="output markdown path",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    paths = sorted(Path(".").glob(args.input_glob))
    runs = load_runs(paths)
    markdown = make_markdown(runs, args.input_glob)
    out_path = Path(args.output_md)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(markdown + "\n", encoding="utf-8")
    print(
        f"[soak-report] runs={len(runs)} output={out_path} "
        f"source_glob={args.input_glob}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
