# Playbook de Captura Real de Protocolo

Fuente: `docs/protocol_opcode_matrix.json`

Este playbook transforma brechas de paridad real en sesiones accionables de captura con cliente real.

## legacy_v382

- Pendientes de captura real: 0
- Estado: sin pendientes.

## modern_v400

- Pendientes de captura real: 0
- Estado: sin pendientes.

## Flujo Operativo

1. Iniciar captura (`make replay-capture-pipeline`).
2. Ejecutar comandos pendientes por escenario (A -> B -> C).
3. Cerrar captura y regenerar reportes (`make replay-real-gap`).
4. Regenerar playbook (`make replay-real-playbook`).
5. Repetir hasta dejar todos los checks en `[x]`.

