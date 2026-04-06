# Diccionario de Datos (PostgreSQL)

Este documento describe cada tabla del esquema definido en:
- `migrations/0001_init.sql`
- `migrations/0002_sanction_status.sql`

Objetivo:
- explicar que representa cada tabla
- explicar el objetivo funcional de cada una
- documentar la descripcion de todos sus campos

## Convenciones

- PK: llave primaria.
- FK: llave foranea.
- UK: llave unica.
- JSONB: estructura flexible para datos no normalizados.

## `accounts`

Objetivo: cuenta base de autenticacion del jugador.

| Campo | Tipo | Descripcion |
|---|---|---|
| `id` | `UUID` | PK de la cuenta. |
| `username` | `VARCHAR(32)` | Nombre de usuario unico para login. |
| `email` | `VARCHAR(190)` | Email unico opcional. |
| `password_hash` | `TEXT` | Hash de contrasena del usuario. |
| `status` | `VARCHAR(16)` | Estado de la cuenta (`active`, `blocked`, etc.). |
| `failed_login_count` | `INT` | Intentos fallidos acumulados. |
| `last_login_at` | `TIMESTAMPTZ` | Fecha del ultimo login exitoso. |
| `created_at` | `TIMESTAMPTZ` | Fecha de creacion. |
| `updated_at` | `TIMESTAMPTZ` | Fecha de ultima actualizacion. |

## `sessions`

Objetivo: sesion de juego activa o historica asociada a una cuenta.

| Campo | Tipo | Descripcion |
|---|---|---|
| `id` | `UUID` | PK de la sesion. |
| `account_id` | `UUID` | FK a `accounts.id`. |
| `character_id` | `UUID` | FK a `characters.id` (puede ser `NULL`). |
| `gateway_node` | `VARCHAR(64)` | Nodo gateway que gestiona la sesion. |
| `remote_ip` | `INET` | IP remota del cliente. |
| `protocol_version` | `VARCHAR(16)` | Version de protocolo detectada. |
| `state` | `VARCHAR(24)` | Estado de sesion (`pre_auth`, `in_world`, etc.). |
| `issued_at` | `TIMESTAMPTZ` | Fecha de emision. |
| `last_seen_at` | `TIMESTAMPTZ` | Ultimo heartbeat o actividad. |
| `expires_at` | `TIMESTAMPTZ` | Fecha de expiracion. |
| `closed_at` | `TIMESTAMPTZ` | Fecha de cierre de sesion. |

## `maps`

Objetivo: catalogo maestro de mapas jugables.

| Campo | Tipo | Descripcion |
|---|---|---|
| `id` | `INT` | PK del mapa. |
| `code` | `VARCHAR(64)` | Codigo unico del mapa. |
| `name` | `VARCHAR(128)` | Nombre visible del mapa. |
| `width` | `INT` | Ancho logico. |
| `height` | `INT` | Alto logico. |
| `default_spawn_x` | `INT` | Coordenada X de spawn por defecto. |
| `default_spawn_y` | `INT` | Coordenada Y de spawn por defecto. |
| `tick_ms` | `INT` | Tick objetivo del loop del mapa. |
| `created_at` | `TIMESTAMPTZ` | Fecha de alta del mapa. |

## `map_instances`

Objetivo: instancias concretas de ejecucion por mapa y shard.

| Campo | Tipo | Descripcion |
|---|---|---|
| `id` | `UUID` | PK de la instancia. |
| `map_id` | `INT` | FK a `maps.id`. |
| `shard_code` | `VARCHAR(64)` | Codigo del shard/instancia. |
| `status` | `VARCHAR(16)` | Estado de instancia (`active`, `stopped`, etc.). |
| `started_at` | `TIMESTAMPTZ` | Inicio de la instancia. |
| `stopped_at` | `TIMESTAMPTZ` | Termino de la instancia. |

## `characters`

Objetivo: estado persistente de cada personaje.

| Campo | Tipo | Descripcion |
|---|---|---|
| `id` | `UUID` | PK del personaje. |
| `account_id` | `UUID` | FK a `accounts.id`. |
| `name` | `VARCHAR(16)` | Nombre unico del personaje. |
| `slot` | `SMALLINT` | Slot de personaje dentro de la cuenta. |
| `class_code` | `VARCHAR(16)` | Clase base del personaje. |
| `gender` | `SMALLINT` | Genero/variant visual. |
| `skin_color` | `SMALLINT` | Color de piel. |
| `hair_style` | `SMALLINT` | Estilo de cabello. |
| `hair_color` | `SMALLINT` | Color de cabello. |
| `underwear_color` | `SMALLINT` | Color de ropa base. |
| `level` | `INT` | Nivel actual. |
| `exp` | `BIGINT` | Experiencia acumulada. |
| `map_id` | `INT` | FK a `maps.id` (ubicacion actual). |
| `pos_x` | `INT` | Posicion X. |
| `pos_y` | `INT` | Posicion Y. |
| `hp` | `INT` | Vida actual. |
| `mp` | `INT` | Mana actual. |
| `sp` | `INT` | Stamina o resource secundario. |
| `str_stat` | `SMALLINT` | Fuerza. |
| `vit_stat` | `SMALLINT` | Vitalidad. |
| `dex_stat` | `SMALLINT` | Destreza. |
| `int_stat` | `SMALLINT` | Inteligencia. |
| `mag_stat` | `SMALLINT` | Magia. |
| `chr_stat` | `SMALLINT` | Carisma. |
| `is_deleted` | `BOOLEAN` | Borrado logico. |
| `created_at` | `TIMESTAMPTZ` | Fecha de creacion. |
| `updated_at` | `TIMESTAMPTZ` | Ultima actualizacion. |

## `entities`

Objetivo: entidades runtime del mapa (jugador/NPC/otros).

| Campo | Tipo | Descripcion |
|---|---|---|
| `id` | `UUID` | PK de entidad. |
| `map_instance_id` | `UUID` | FK a `map_instances.id`. |
| `entity_kind` | `VARCHAR(24)` | Tipo de entidad (`player`, `npc`, etc.). |
| `owner_character_id` | `UUID` | FK a `characters.id` cuando aplica. |
| `npc_id` | `UUID` | Referencia logica a `npcs.id` (sin FK formal). |
| `pos_x` | `INT` | Posicion X de entidad. |
| `pos_y` | `INT` | Posicion Y de entidad. |
| `hp` | `INT` | HP de la entidad. |
| `mp` | `INT` | MP de la entidad. |
| `alive` | `BOOLEAN` | Estado vivo/muerto. |
| `state_json` | `JSONB` | Estado runtime flexible. |
| `created_at` | `TIMESTAMPTZ` | Fecha de creacion. |
| `updated_at` | `TIMESTAMPTZ` | Ultima actualizacion. |

## `npcs`

Objetivo: definicion maestra de NPCs.

| Campo | Tipo | Descripcion |
|---|---|---|
| `id` | `UUID` | PK del NPC. |
| `code` | `VARCHAR(64)` | Codigo unico del NPC. |
| `name` | `VARCHAR(128)` | Nombre de despliegue. |
| `behavior_tree` | `VARCHAR(128)` | Arbol de comportamiento por defecto. |
| `base_level` | `INT` | Nivel base del NPC. |
| `config_json` | `JSONB` | Configuracion parametrica del NPC. |

## `items`

Objetivo: catalogo maestro de items.

| Campo | Tipo | Descripcion |
|---|---|---|
| `id` | `BIGSERIAL` | PK del item. |
| `code` | `VARCHAR(64)` | Codigo unico de item. |
| `item_type` | `VARCHAR(32)` | Tipo funcional (`weapon`, `consumable`, etc.). |
| `max_stack` | `INT` | Maximo apilable por slot. |
| `attrs` | `JSONB` | Atributos dinamicos del item. |

## `inventories`

Objetivo: cabecera de inventario por personaje.

| Campo | Tipo | Descripcion |
|---|---|---|
| `id` | `UUID` | PK del inventario. |
| `character_id` | `UUID` | FK unica a `characters.id`. |
| `gold` | `BIGINT` | Oro disponible. |
| `version` | `BIGINT` | Version para control de concurrencia. |
| `updated_at` | `TIMESTAMPTZ` | Ultima actualizacion. |

## `inventory_items`

Objetivo: detalle de slots de inventario.

| Campo | Tipo | Descripcion |
|---|---|---|
| `id` | `UUID` | PK del registro. |
| `inventory_id` | `UUID` | FK a `inventories.id`. |
| `item_id` | `BIGINT` | FK a `items.id`. |
| `slot` | `INT` | Slot ocupado dentro del inventario. |
| `quantity` | `INT` | Cantidad de unidades en el slot. |
| `metadata` | `JSONB` | Metadatos del item en inventario. |

## `equipment_slots`

Objetivo: equipo actualmente vestido/equipado por personaje.

| Campo | Tipo | Descripcion |
|---|---|---|
| `id` | `UUID` | PK del registro. |
| `character_id` | `UUID` | FK a `characters.id`. |
| `slot_code` | `VARCHAR(32)` | Tipo de slot (`head`, `weapon`, etc.). |
| `item_id` | `BIGINT` | FK a `items.id` (nullable). |
| `durability` | `INT` | Durabilidad actual del item equipado. |
| `metadata` | `JSONB` | Estado adicional del slot/item. |

## `skills`

Objetivo: catalogo maestro de skills.

| Campo | Tipo | Descripcion |
|---|---|---|
| `id` | `INT` | PK de skill. |
| `code` | `VARCHAR(64)` | Codigo unico de skill. |
| `display_name` | `VARCHAR(128)` | Nombre de visualizacion. |
| `mana_cost` | `INT` | Costo base de mana. |
| `cooldown_ms` | `INT` | Cooldown base en milisegundos. |
| `config_json` | `JSONB` | Parametros adicionales de skill. |

## `character_skills`

Objetivo: progreso de skills por personaje.

| Campo | Tipo | Descripcion |
|---|---|---|
| `character_id` | `UUID` | FK a `characters.id` (parte de PK compuesta). |
| `skill_id` | `INT` | FK a `skills.id` (parte de PK compuesta). |
| `level` | `SMALLINT` | Nivel de dominio de la skill. |
| `exp` | `BIGINT` | Experiencia acumulada en la skill. |

## `combat_logs`

Objetivo: trazabilidad de eventos de combate.

| Campo | Tipo | Descripcion |
|---|---|---|
| `id` | `UUID` | PK del log. |
| `map_instance_id` | `UUID` | FK a `map_instances.id`. |
| `attacker_entity_id` | `UUID` | ID de entidad atacante. |
| `defender_entity_id` | `UUID` | ID de entidad defensora. |
| `skill_id` | `INT` | Referencia de skill usada (sin FK formal). |
| `damage` | `INT` | Danio aplicado. |
| `was_critical` | `BOOLEAN` | Marca de golpe critico. |
| `payload` | `JSONB` | Payload tecnico del evento. |
| `occurred_at` | `TIMESTAMPTZ` | Momento del evento. |

## `guilds`

Objetivo: cabecera de gremios.

| Campo | Tipo | Descripcion |
|---|---|---|
| `id` | `UUID` | PK del gremio. |
| `name` | `VARCHAR(32)` | Nombre unico del gremio. |
| `notice` | `TEXT` | Mensaje/aviso del gremio. |
| `created_by` | `UUID` | FK a `characters.id` del fundador. |
| `created_at` | `TIMESTAMPTZ` | Fecha de creacion. |

## `guild_members`

Objetivo: membresias y rango dentro de un gremio.

| Campo | Tipo | Descripcion |
|---|---|---|
| `guild_id` | `UUID` | FK a `guilds.id` (PK compuesta). |
| `character_id` | `UUID` | FK a `characters.id` (PK compuesta). |
| `rank` | `VARCHAR(16)` | Rango dentro del gremio. |
| `joined_at` | `TIMESTAMPTZ` | Fecha de ingreso. |

## `mail_messages`

Objetivo: correo interno entre personajes.

| Campo | Tipo | Descripcion |
|---|---|---|
| `id` | `UUID` | PK del mensaje. |
| `from_character_id` | `UUID` | FK a `characters.id` remitente (nullable). |
| `to_character_id` | `UUID` | FK a `characters.id` destinatario. |
| `subject` | `VARCHAR(128)` | Asunto del correo. |
| `body` | `TEXT` | Cuerpo del mensaje. |
| `attached_item` | `JSONB` | Adjuntos en formato JSON. |
| `is_read` | `BOOLEAN` | Marca de lectura. |
| `created_at` | `TIMESTAMPTZ` | Fecha de envio. |

## `game_events`

Objetivo: bitacora de eventos de juego relevantes.

| Campo | Tipo | Descripcion |
|---|---|---|
| `id` | `UUID` | PK del evento. |
| `event_type` | `VARCHAR(64)` | Tipo de evento. |
| `actor_character_id` | `UUID` | FK a `characters.id` actor. |
| `map_id` | `INT` | FK a `maps.id` del contexto. |
| `payload` | `JSONB` | Detalle del evento. |
| `occurred_at` | `TIMESTAMPTZ` | Fecha de ocurrencia. |

## `sanctions`

Objetivo: historial de sanciones a cuenta/personaje.

| Campo | Tipo | Descripcion |
|---|---|---|
| `id` | `UUID` | PK de sancion. |
| `account_id` | `UUID` | FK a `accounts.id`. |
| `character_id` | `UUID` | FK a `characters.id`. |
| `sanction_type` | `VARCHAR(32)` | Tipo (`mute`, `ban`, `jail`, etc.). |
| `reason` | `TEXT` | Motivo de la sancion. |
| `starts_at` | `TIMESTAMPTZ` | Inicio de vigencia. |
| `ends_at` | `TIMESTAMPTZ` | Fin de vigencia (nullable). |
| `issued_by_admin_id` | `UUID` | Referencia logica al admin emisor (sin FK formal). |
| `status` | `VARCHAR(16)` | Estado de sancion (`active`, `expired`). Agregado en `0002`. |
| `created_at` | `TIMESTAMPTZ` | Fecha de registro. |

## `admin_users`

Objetivo: identidad de operadores del portal admin.

| Campo | Tipo | Descripcion |
|---|---|---|
| `id` | `UUID` | PK del admin. |
| `email` | `VARCHAR(190)` | Email unico de acceso. |
| `password_hash` | `TEXT` | Hash de contrasena. |
| `status` | `VARCHAR(16)` | Estado del usuario admin. |
| `created_at` | `TIMESTAMPTZ` | Fecha de alta. |

## `admin_sessions`

Objetivo: sesiones autenticadas del portal admin.

| Campo | Tipo | Descripcion |
|---|---|---|
| `id` | `UUID` | PK de sesion admin. |
| `admin_user_id` | `UUID` | FK a `admin_users.id`. |
| `token_hash` | `TEXT` | Hash del token bearer. |
| `issued_at` | `TIMESTAMPTZ` | Emision de sesion. |
| `expires_at` | `TIMESTAMPTZ` | Expiracion de sesion. |
| `revoked_at` | `TIMESTAMPTZ` | Revocacion de sesion. |

## `admin_roles`

Objetivo: catalogo de roles RBAC.

| Campo | Tipo | Descripcion |
|---|---|---|
| `id` | `UUID` | PK de rol. |
| `code` | `VARCHAR(32)` | Codigo unico de rol. |
| `name` | `VARCHAR(64)` | Nombre de rol. |

## `admin_permissions`

Objetivo: catalogo de permisos RBAC.

| Campo | Tipo | Descripcion |
|---|---|---|
| `id` | `UUID` | PK de permiso. |
| `code` | `VARCHAR(64)` | Codigo unico de permiso. |
| `description` | `TEXT` | Descripcion funcional del permiso. |

## `admin_role_permissions`

Objetivo: asignacion many-to-many entre roles y permisos.

| Campo | Tipo | Descripcion |
|---|---|---|
| `role_id` | `UUID` | FK a `admin_roles.id` (PK compuesta). |
| `permission_id` | `UUID` | FK a `admin_permissions.id` (PK compuesta). |

## `admin_user_roles`

Objetivo: asignacion many-to-many entre usuarios admin y roles.

| Campo | Tipo | Descripcion |
|---|---|---|
| `admin_user_id` | `UUID` | FK a `admin_users.id` (PK compuesta). |
| `role_id` | `UUID` | FK a `admin_roles.id` (PK compuesta). |

## `admin_audit_logs`

Objetivo: trazabilidad de acciones administrativas sensibles.

| Campo | Tipo | Descripcion |
|---|---|---|
| `id` | `UUID` | PK del evento de auditoria. |
| `admin_user_id` | `UUID` | FK a `admin_users.id`. |
| `action_type` | `VARCHAR(64)` | Tipo de accion ejecutada. |
| `target_type` | `VARCHAR(64)` | Tipo de recurso objetivo. |
| `target_id` | `VARCHAR(128)` | Identificador del recurso objetivo. |
| `request_id` | `VARCHAR(64)` | Correlation/request id. |
| `payload` | `JSONB` | Datos auxiliares de auditoria. |
| `ip_address` | `INET` | IP origen de la accion. |
| `created_at` | `TIMESTAMPTZ` | Fecha/hora del evento. |
