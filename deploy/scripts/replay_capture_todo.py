#!/usr/bin/env python3
"""
Generate an actionable capture TODO list from protocol opcode coverage report.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Dict, List, Set


DEFAULT_ACTIONS: Dict[str, str] = {
    "login": "Iniciar sesion con cuenta de prueba.",
    "character_list": "Abrir pantalla/lista de personajes.",
    "character_create": "Crear un personaje nuevo (nombre temporal).",
    "character_delete": "Eliminar un personaje de prueba.",
    "character_select": "Seleccionar un personaje existente.",
    "enter_world": "Entrar al mundo con el personaje seleccionado.",
    "move": "Moverse en varias direcciones (WASD/click) por al menos 5 segundos.",
    "attack": "Atacar un NPC o entidad al menos una vez.",
    "cast_skill": "Lanzar una skill activa sobre objetivo o self.",
    "pickup_item": "Recoger un item del suelo.",
    "drop_item": "Soltar un item desde inventario al suelo.",
    "use_item": "Usar/consumir un item desde inventario.",
    "npc_interaction": "Hablar/interactuar con un NPC.",
    "chat": "Enviar un mensaje por chat general/mapa.",
    "whisper": "Enviar un whisper/mensaje privado a otra cuenta/personaje.",
    "guild_chat": "Enviar un mensaje por canal guild.",
    "heartbeat": "Permanecer conectado al menos 30 segundos para forzar heartbeat.",
    "logout": "Cerrar sesion/logout limpio desde cliente.",
}

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


def load_report(path: Path) -> dict:
    payload = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(payload, dict):
        raise ValueError("invalid json report format")
    return payload


def parse_protocols(raw: str) -> List[str]:
    return [chunk.strip().lower() for chunk in raw.split(",") if chunk.strip()]


def as_set_map(payload: dict, key: str) -> Dict[str, Set[str]]:
    raw = payload.get(key, {})
    if not isinstance(raw, dict):
        return {}
    out: Dict[str, Set[str]] = {}
    for protocol, items in raw.items():
        if isinstance(items, list):
            out[str(protocol).lower()] = {str(item).lower() for item in items}
    return out


def make_markdown(report: dict, protocols: List[str], source: Path) -> str:
    required_commands = [
        str(x).lower() for x in report.get("required_commands", COMMAND_ORDER)
    ]
    if not required_commands:
        required_commands = list(COMMAND_ORDER)
    missing_by_protocol = report.get("missing_real_required", {})
    coverage_real_map = as_set_map(report, "coverage_real_manual_capture")
    lines: List[str] = []
    lines.append("# TODO de Captura Real de Protocolo")
    lines.append("")
    lines.append(f"Fuente: `{source.as_posix()}`")
    lines.append("")
    for protocol in protocols:
        missing = missing_by_protocol.get(protocol, [])
        if not isinstance(missing, list):
            missing = []
        missing = [str(cmd).lower() for cmd in missing]
        missing_set = set(missing)
        captured = coverage_real_map.get(protocol, set())
        done = [cmd for cmd in required_commands if cmd in captured]
        pending = [cmd for cmd in required_commands if cmd in missing_set]

        lines.append(f"## {protocol}")
        lines.append("")
        lines.append(f"- Cobertura real pendiente: {len(missing)}/{len(required_commands)}")
        lines.append(f"- Cobertura real lograda : {len(done)}/{len(required_commands)}")
        if not missing:
            lines.append("- Estado: sin brechas pendientes.")
            lines.append("")
            continue

        lines.append("### Pendientes")
        lines.append("")
        for command in pending:
            action = DEFAULT_ACTIONS.get(command, "Ejecutar accion correspondiente en cliente.")
            lines.append(f"  - [ ] `{command}`: {action}")

        if done:
            lines.append("")
            lines.append("### Ya cubiertos")
            lines.append("")
            for command in done:
                action = DEFAULT_ACTIONS.get(command, "Accion funcional ejecutada.")
                lines.append(f"  - [x] `{command}`: {action}")
        lines.append("")
    lines.append("## Flujo recomendado")
    lines.append("")
    lines.append("1. Iniciar captura (`make replay-capture-pipeline`).")
    lines.append("2. Ejecutar acciones pendientes del protocolo objetivo.")
    lines.append("3. Detener captura y validar `docs/protocol_opcode_matrix.md`.")
    lines.append("4. Repetir hasta dejar `faltantes_real` en `ninguno`.")
    lines.append("")
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate protocol capture TODO markdown from opcode report json",
    )
    parser.add_argument(
        "--input",
        default="docs/protocol_opcode_matrix.json",
        help="input json report generated by replay_opcode_report.py",
    )
    parser.add_argument(
        "--output",
        default="docs/protocol_capture_todo.md",
        help="markdown output path",
    )
    parser.add_argument(
        "--protocols",
        default="legacy_v382,modern_v400",
        help="comma-separated protocol list to include",
    )
    args = parser.parse_args()

    source = Path(args.input)
    report = load_report(source)
    protocols = parse_protocols(args.protocols)
    if not protocols:
        protocols = ["legacy_v382", "modern_v400"]

    markdown = make_markdown(report, protocols, source)
    out = Path(args.output)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(markdown + "\n", encoding="utf-8")
    print(f"[replay-capture-todo] markdown -> {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
