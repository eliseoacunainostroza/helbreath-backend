# Matriz de Opcodes por Version

Fuente: `crates/net/tests/fixtures/replay_cases.json`

Casos: command=48, decode_error=1, translate_error=5

Origen: capture=0, manual=53, seed=1, synthetic=0

modern_v400 comandos no-seed (manual/capture): 18

## Cobertura de Comandos Requeridos

Comandos requeridos (18): login, character_list, character_create, character_delete, character_select, enter_world, move, attack, cast_skill, pickup_item, drop_item, use_item, npc_interaction, chat, whisper, guild_chat, heartbeat, logout

| protocolo | cobertura_total | cobertura_real_manual_capture | faltantes_real |
|---|---:|---:|---|
| legacy_v382 | 18/18 | 18/18 | ninguno |
| modern_v400 | 18/18 | 18/18 | ninguno |

| comando | legacy_v382 | modern_v400 |
|---|---|---|
| attack | 0x0101 | 0x0101 |
| cast_skill | 0x0102 | 0x0102 |
| character_create | 0x0003 | 0x0003 |
| character_delete | 0x0006 | 0x0006 |
| character_list | 0x0002 | 0x0002 |
| character_select | 0x0004 | 0x0004 |
| chat | 0x0200 | 0x0200 |
| drop_item | 0x0104 | 0x0104 |
| enter_world | 0x0005 | 0x0005 |
| guild_chat | 0x0202 | 0x0202 |
| heartbeat | 0x02FE | 0x02FE |
| login | 0x0001 | 0x0001 |
| logout | 0x02FF | 0x02FF |
| move | 0x0100 | 0x0100 |
| npc_interaction | 0x0106 | 0x0106 |
| pickup_item | 0x0103 | 0x0103 |
| use_item | 0x0105 | 0x0105 |
| whisper | 0x0201 | 0x0201 |

