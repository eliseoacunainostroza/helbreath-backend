# Checklist Detallado de Paridad (Componente Legacy de Protocolo)

Fuente: `docs/protocol_opcode_matrix.json`

Convencion: `migrada` indica implementacion en Rust; `probada_auto` indica cobertura por tests/replay; `probada_real_*` indica evidencia manual/capture; `probada_capture_*` exige origen `capture` (cliente real).

## 1) Infraestructura de la Componente

| funcionalidad original | migrada | probada_auto | evidencia |
|---|---|---|---|
| Framing binario de entrada (u16 len + u16 opcode + payload) | [x] | [x] | `decode_frame` + tests unitarios |
| Validacion de largo declarado vs bytes reales | [x] | [x] | `DecodeError::InvalidLength` |
| Guardrail de tamano maximo de payload | [x] | [x] | `DecodeError::PayloadTooLarge` |
| Encoder de frames de salida | [x] | [x] | `encode_frame` + pruebas replay |
| Matriz de opcodes por version de protocolo | [x] | [x] | `OpcodeMatrix::{legacy_v382,modern_v400}` |
| Traductor packet->ClientCommand con adaptador de version | [x] | [x] | `translate_packet_for_version` |
| Validacion de estado de sesion por comando | [x] | [x] | `validate_client_command` |
| Rate limiting por conexion (token bucket) | [x] | [x] | `TokenBucketRateLimiter` + test |
| Reensamblado incremental de frames en stream TCP | [x] | [x] | `split_frames` |
| Pipeline de replay (capture/synthetic/seed/manual) para regresion | [x] | [x] | `replay_packets.rs` + scripts deploy/scripts/replay_* |

## 2) Matriz Detallada por Comando Legacy

| comando | fase_legacy | payload_legacy | opcode_legacy | opcode_modern | migrada_decode_map | migrada_parse_payload | migrada_gate_sesion | probada_auto | probada_smoke_tcp | probada_real_legacy | probada_real_modern | probada_capture_legacy | probada_capture_modern | evidencia_parse |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `login` | pre_auth | username/password/version | 0x0001 | 0x0001 | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | `parse_login_payload` |
| `character_list` | in_character_list | sin payload | 0x0002 | 0x0002 | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | opcode sin payload |
| `character_create` | in_character_list | name/class | 0x0003 | 0x0003 | [x] | [x] | [x] | [x] | [ ] | [x] | [x] | [x] | [x] | `parse_character_create_payload` |
| `character_delete` | in_character_list | uuid character_id | 0x0006 | 0x0006 | [x] | [x] | [x] | [x] | [ ] | [x] | [x] | [x] | [x] | `parse_uuid_payload` |
| `character_select` | in_character_list | uuid character_id | 0x0004 | 0x0004 | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | `parse_uuid_payload` |
| `enter_world` | in_character_list | sin payload | 0x0005 | 0x0005 | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | opcode sin payload |
| `move` | in_world | x/y/run | 0x0100 | 0x0100 | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | parse inline x/y/run |
| `attack` | in_world | uuid target_id | 0x0101 | 0x0101 | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | `parse_uuid_payload` |
| `cast_skill` | in_world | skill_id(+target opcional) | 0x0102 | 0x0102 | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | parse inline skill_id/target |
| `pickup_item` | in_world | uuid entity_id | 0x0103 | 0x0103 | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | `parse_uuid_payload` |
| `drop_item` | in_world | slot/quantity | 0x0104 | 0x0104 | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | parse inline slot/quantity |
| `use_item` | in_world | slot | 0x0105 | 0x0105 | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | parse inline slot |
| `npc_interaction` | in_world | uuid npc_id | 0x0106 | 0x0106 | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | `parse_uuid_payload` |
| `chat` | in_world | message | 0x0200 | 0x0200 | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | `parse_text_payload` |
| `whisper` | in_world | to_character/message | 0x0201 | 0x0201 | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | `parse_whisper_payload` |
| `guild_chat` | in_world | message | 0x0202 | 0x0202 | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | `parse_text_payload` |
| `heartbeat` | in_world/post_auth | sin payload | 0x02FE | 0x02FE | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | opcode sin payload |
| `logout` | post_auth/in_world | sin payload | 0x02FF | 0x02FF | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | [x] | opcode sin payload |

## 3) Brechas Funcionales Relevantes vs Componente Original

| funcionalidad original esperada | migrada | probada_auto | observacion |
|---|---|---|---|
| Decodificacion formal server->client en `crates/net` (simetria completa de protocolo) | [x] | [x] | Implementado con `ServerMessage` + `translate_server_packet_for_version` + tests unitarios. |
| Capa de cifrado/obfuscacion wire legacy dedicada | [x] | [x] | Implementado con `obfuscate_wire_payload`/`deobfuscate_wire_payload` + test de roundtrip. |
| Compresion/descompresion de payloads de red en capa net | [x] | [x] | Implementado con `compress_wire_payload`/`decompress_wire_payload` + test de roundtrip. |
| Catalogo exhaustivo de codigos de error legacy (wire-level) | [x] | [x] | Implementado con `WireErrorCode` + `parse_wire_error_code` y uso en decode server->client. |

## 4) Resumen de Cobertura Real

- Comandos requeridos: 18
- Legacy real: 18/18
- Modern real: 18/18
- Legacy capture (cliente real): 18/18
- Modern capture (cliente real): 18/18
- Pendientes legacy real: ninguno
- Pendientes modern real: ninguno
- Pendientes legacy capture: ninguno
- Pendientes modern capture: ninguno
- Cobertura smoke tcp gateway: 16/18

## 5) Evidencia Smoke TCP

- `gateway.tcp.route-flow`: valida login/list/select/enter-world/move/logout (legacy).
- `gateway.tcp.route-flow.modern`: valida flujo base modern_v400.
- `gateway.tcp.command-matrix`: valida attack/cast/pickup/drop/use/npc/chat/whisper/guild/heartbeat con evidencia outbound+persistencia.

