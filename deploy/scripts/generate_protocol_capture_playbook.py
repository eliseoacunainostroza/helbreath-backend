#!/usr/bin/env python3
"""
Generate a detailed capture playbook from protocol parity gaps.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Dict, List, Sequence, Tuple


COMMAND_META: Dict[str, Dict[str, str]] = {
    "login": {
        "requisito": "1 cuenta",
        "accion": "Iniciar sesion en cliente.",
        "escenario": "Sesion A (baseline)",
    },
    "character_list": {
        "requisito": "1 cuenta",
        "accion": "Entrar a pantalla de seleccion/lista de personajes.",
        "escenario": "Sesion A (baseline)",
    },
    "character_create": {
        "requisito": "1 cuenta",
        "accion": "Crear personaje temporal.",
        "escenario": "Sesion A (baseline)",
    },
    "character_delete": {
        "requisito": "1 cuenta",
        "accion": "Eliminar personaje temporal.",
        "escenario": "Sesion A (baseline)",
    },
    "character_select": {
        "requisito": "1 cuenta",
        "accion": "Seleccionar personaje jugable.",
        "escenario": "Sesion A (baseline)",
    },
    "enter_world": {
        "requisito": "1 cuenta",
        "accion": "Entrar al mapa/mundo.",
        "escenario": "Sesion A (baseline)",
    },
    "move": {
        "requisito": "1 cuenta",
        "accion": "Mover personaje por al menos 5 segundos en varias direcciones.",
        "escenario": "Sesion A (baseline)",
    },
    "attack": {
        "requisito": "1 cuenta + objetivo",
        "accion": "Atacar NPC/mob al menos una vez.",
        "escenario": "Sesion B (combate/inventario)",
    },
    "cast_skill": {
        "requisito": "1 cuenta + skill aprendida",
        "accion": "Lanzar skill activa sobre target o self.",
        "escenario": "Sesion B (combate/inventario)",
    },
    "pickup_item": {
        "requisito": "1 cuenta + item en suelo",
        "accion": "Recoger un item del piso.",
        "escenario": "Sesion B (combate/inventario)",
    },
    "drop_item": {
        "requisito": "1 cuenta + item en inventario",
        "accion": "Soltar item desde inventario.",
        "escenario": "Sesion B (combate/inventario)",
    },
    "use_item": {
        "requisito": "1 cuenta + item usable",
        "accion": "Consumir/usar item.",
        "escenario": "Sesion B (combate/inventario)",
    },
    "npc_interaction": {
        "requisito": "1 cuenta + NPC cercano",
        "accion": "Interactuar con NPC (dialogo/tienda).",
        "escenario": "Sesion B (combate/inventario)",
    },
    "chat": {
        "requisito": "1 cuenta",
        "accion": "Enviar mensaje de chat de mapa/general.",
        "escenario": "Sesion C (social)",
    },
    "whisper": {
        "requisito": "2 cuentas online",
        "accion": "Enviar whisper de cuenta A a cuenta B.",
        "escenario": "Sesion C (social)",
    },
    "guild_chat": {
        "requisito": "2 cuentas + guild",
        "accion": "Enviar mensaje por canal de guild.",
        "escenario": "Sesion C (social)",
    },
    "heartbeat": {
        "requisito": "1 cuenta conectada",
        "accion": "Permanecer conectado 30-60 segundos sin desconectar.",
        "escenario": "Sesion A (baseline)",
    },
    "logout": {
        "requisito": "1 cuenta conectada",
        "accion": "Cerrar sesion limpio desde cliente.",
        "escenario": "Sesion A (baseline)",
    },
}

SCENARIO_ORDER: Sequence[str] = (
    "Sesion A (baseline)",
    "Sesion B (combate/inventario)",
    "Sesion C (social)",
)


def load_json(path: Path) -> dict:
    payload = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(payload, dict):
        raise ValueError("invalid opcode report json")
    return payload


def parse_protocols(raw: str) -> List[str]:
    return [chunk.strip().lower() for chunk in raw.split(",") if chunk.strip()]


def missing_for_protocol(payload: dict, protocol: str, source_mode: str) -> List[str]:
    key = "missing_capture_required" if source_mode == "capture_only" else "missing_real_required"
    missing = payload.get(key, {})
    if not isinstance(missing, dict):
        return []
    raw = missing.get(protocol, [])
    if not isinstance(raw, list):
        return []
    return [str(x).lower() for x in raw]


def render_protocol_table(lines: List[str], protocol: str, pending: List[str]) -> None:
    lines.append(f"## {protocol}")
    lines.append("")
    lines.append(f"- Pendientes de captura real: {len(pending)}")
    if not pending:
        lines.append("- Estado: sin pendientes.")
        lines.append("")
        return

    lines.append("| comando | requisito | accion sugerida | escenario recomendado | check |")
    lines.append("|---|---|---|---|---|")
    for command in pending:
        meta = COMMAND_META.get(
            command,
            {
                "requisito": "n/d",
                "accion": "Ejecutar accion funcional equivalente en cliente real.",
                "escenario": "Sesion A (baseline)",
            },
        )
        lines.append(
            "| "
            + f"`{command}`"
            + f" | {meta['requisito']}"
            + f" | {meta['accion']}"
            + f" | {meta['escenario']}"
            + " | [ ] |"
        )
    lines.append("")

    pending_set = set(pending)
    lines.append("### Orden recomendado de ejecucion")
    lines.append("")
    for scenario in SCENARIO_ORDER:
        scenario_commands = [
            command
            for command in pending
            if COMMAND_META.get(command, {}).get("escenario") == scenario
        ]
        if not scenario_commands:
            continue
        lines.append(f"- {scenario}: " + ", ".join(f"`{cmd}`" for cmd in scenario_commands))
    lines.append("")


def make_markdown(payload: dict, source: Path, protocols: List[str], source_mode: str) -> str:
    lines: List[str] = []
    lines.append("# Playbook de Captura Real de Protocolo")
    lines.append("")
    lines.append(f"Fuente: `{source.as_posix()}`")
    lines.append("")
    lines.append(
        "Este playbook transforma brechas de paridad real en sesiones accionables de captura con cliente real."
    )
    lines.append("")
    lines.append(f"Modo de cobertura: `{source_mode}`")
    lines.append("")
    for protocol in protocols:
        render_protocol_table(lines, protocol, missing_for_protocol(payload, protocol, source_mode))

    lines.append("## Flujo Operativo")
    lines.append("")
    lines.append("1. Iniciar captura (`make replay-capture-pipeline`).")
    lines.append("2. Ejecutar comandos pendientes por escenario (A -> B -> C).")
    lines.append("3. Cerrar captura y regenerar reportes (`make replay-real-gap`).")
    lines.append("4. Regenerar playbook (`make replay-real-playbook`).")
    lines.append("5. Repetir hasta dejar todos los checks en `[x]`.")
    lines.append("")
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate protocol capture playbook from parity gaps"
    )
    parser.add_argument(
        "--input",
        default="docs/protocol_opcode_matrix.json",
        help="opcode parity json report",
    )
    parser.add_argument(
        "--output",
        default="docs/protocol_capture_playbook.md",
        help="markdown output path",
    )
    parser.add_argument(
        "--protocols",
        default="legacy_v382,modern_v400",
        help="comma-separated protocol list",
    )
    parser.add_argument(
        "--source-mode",
        choices=["manual_capture", "capture_only"],
        default="capture_only",
        help="coverage source mode from opcode report json",
    )
    args = parser.parse_args()

    source = Path(args.input)
    payload = load_json(source)
    protocols = parse_protocols(args.protocols)
    if not protocols:
        protocols = ["legacy_v382", "modern_v400"]

    markdown = make_markdown(payload, source, protocols, args.source_mode)
    out = Path(args.output)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(markdown + "\n", encoding="utf-8")
    print(f"[replay-playbook] markdown -> {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
