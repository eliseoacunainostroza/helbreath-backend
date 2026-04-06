# Arquitectura Base

## Objetivos
- Gameplay autoritativo del lado servidor.
- Separación híbrida de servicios (gateway/auth/world/map/chat/admin/jobs) sin microfragmentar el gameplay.
- PostgreSQL como fuente de verdad.
- Portal administrativo operativo con RBAC y rastro de auditoría.
- Despliegue orientado a Linux (systemd), con `docker compose` para desarrollo.

## Responsabilidad por servicio
- `gateway`: borde de sesiones TCP, decode/validación de paquetes, rate limit, traducción a comandos internos.
- `auth-service`: autenticación de cuentas y persistencia del ciclo de vida de sesiones.
- `world-service`: enrutamiento de comandos y orquestación de instancias de mapa.
- `map-server`: loop de simulación determinista con tick fijo (movimiento/combate/IA/AOI/inventario).
- `chat-service`: canales de mapa/whisper/guild.
- `admin-api`: login admin, RBAC, moderación, gestión de cuentas/personajes/mundo, auditoría.
- `jobs-runner`: limpieza y trabajos asíncronos de fondo.

## Límites del dominio
El estado crítico de gameplay permanece dentro del loop world/map:
- movimiento
- combate
- comportamiento IA/NPC
- AOI
- inventario/equipment/skills
- timers críticos

## Modelo de concurrencia
- Una tarea principal por instancia de mapa.
- Colas MPSC para comandos de entrada.
- El tick loop es dueño del estado mutable del mapa.
- I/O y persistencia delegados a workers asíncronos auxiliares.
- No usar `Arc<Mutex<T>>` por defecto en la ruta de gameplay.

## Modelo de paquetes
El protocolo externo está aislado en `crates/net`:
- decodificador de frames
- mapeo de opcodes
- parsers de payload
- guardas de estado de sesión
- rate limiter tipo token bucket
- adaptador por versión de protocolo (`ProtocolVersion` + `OpcodeMatrix`)

La interacción con el dominio ocurre solo mediante `application::ClientCommand` tipado.

## Flujo del gateway (implementación actual)
- Ingreso TCP:
  - separación de paquetes enmarcados
  - decode + traducción según versión de protocolo
  - validación por fase de sesión
  - timeout de lectura idle por sesión (`HB_GATEWAY_IDLE_TIMEOUT_SECONDS`)
  - límite de conexiones por IP (`HB_GATEWAY_MAX_CONNECTIONS_PER_IP`)
- Enrutamiento interno:
  - `login/logout` -> `auth-service`
  - `character list/create/delete/select` -> `auth-service`
  - comandos de gameplay -> API de ruteo en `world-service`
  - el payload hacia `world-service` incluye `session_id` para contadores autoritativos
  - se rechazan comandos de gameplay cuando la sesión no está autenticada o no está en mundo

## Estrategia de persistencia
Persistencia inmediata:
- transiciones de sesión en login/logout
- carga/selección de personaje
- cambios críticos de movimiento (`characters` + `game_events`)
- cambios críticos de inventario (`inventories.version` + `game_events`)
- logs de combate (`combat_logs` + `game_events`)
- sanciones de moderación
- acciones admin auditables

Persistencia diferida o batch (solo ruta segura):
- telemetría/eventos no críticos
- snapshots pesados de mapa en intervalos configurables

## Línea base de seguridad
- validación autoritativa del servidor
- checks estrictos de comando/estado de sesión
- rate limiting en gateway
- RBAC admin con validación de permisos por endpoint
- sesiones admin de corta duración
- auditoría completa de acciones sensibles

## Línea base de observabilidad
- tracing estructurado
- registro de métricas (conexiones, errores auth, errores decode, profundidad de cola, métricas de tick)
- exportación Prometheus en todos los servicios vía `/metrics/prometheus`
- checks de readiness para DB y Redis opcional
- correlation IDs para acciones administrativas

## Pseudocódigo del tick loop
```text
loop cada tick_ms:
  inicio = now()

  drenar cola de comandos entrantes (hasta command_budget)
  procesar eventos programados para este tick

  # orden determinista
  normalizar input
  ejecutar sistema de movimiento
  ejecutar sistema de combate
  ejecutar sistema de IA de NPC
  ejecutar sistema de visibilidad AOI
  ejecutar sistema de inventario/equipment/items

  emitir eventos de salida
  encolar operaciones de persistencia

  elapsed = now() - inicio
  registrar tick_duration_ms
  if elapsed > tick_ms:
    tick_overruns += 1
```
