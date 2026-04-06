# TODO de Captura Real de Protocolo

Fuente: `docs/protocol_opcode_matrix.json`

## legacy_v382

- Cobertura real pendiente: 0/18
- Cobertura real lograda : 18/18
- Estado: sin brechas pendientes.

## modern_v400

- Cobertura real pendiente: 0/18
- Cobertura real lograda : 18/18
- Estado: sin brechas pendientes.

## Flujo recomendado

1. Iniciar captura (`make replay-capture-pipeline`).
2. Ejecutar acciones pendientes del protocolo objetivo.
3. Detener captura y validar `docs/protocol_opcode_matrix.md`.
4. Repetir hasta dejar `faltantes_real` en `ninguno`.

