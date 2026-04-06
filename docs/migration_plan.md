# Plan de Migración (Legacy -> Rust)

## Fase 0 - Descubrimiento y definición de paridad
- Inventariar formatos de paquetes legacy.
- Mapear el esquema SQL legacy al esquema objetivo en PostgreSQL.
- Capturar reglas de gameplay y semántica de timers.
- Clasificar bugs legacy: compatibilidad a preservar vs correcciones intencionales.

## Fase 1 - Edge y autenticación
- Decoder de framing y guardas de paquetes en gateway.
- Servicio de auth con persistencia de sesiones en PostgreSQL.
- Ciclo de vida de sesión y anti-flood básico.

## Fase 2 - Flujo de entrada de personaje
- Character list/create/delete/select.
- Carga de personaje desde PostgreSQL.
- Baseline de enter-world con asignación autoritativa de mapa.

## Fase 3 - Esqueleto world + map
- Coordinador de mundo y modelo de ownership por instancia de mapa.
- Tick loop fijo y orden determinista de comandos.
- Movimiento básico y AOI básico.

## Fase 4 - Gameplay central
- Resolución de combate dentro del map loop.
- Flujo transaccional de inventario/equipment/items.
- Interacción NPC y skills base.

## Fase 5 - Social + operaciones
- Chat, whisper, guild chat, mail.
- Admin API + dashboard + moderación + auditoría.
- Jobs runner y cobertura completa de métricas/tracing.

## Fase 6 - Hardening y producción
- Tests replay y packet golden.
- Soak tests y pruebas de carga.
- Dual-run con legacy durante ventana de confianza.
- Tuning de runtime y playbook de salida a producción.

## Notas de progreso (base actual)
- Gateway enruta auth/gameplay usando adaptadores entre servicios.
- Ciclo de personaje por auth-service implementado (list/create/delete/select enlazado a sesión).
- World coordinator ahora rastrea sesiones únicas por `session_id` (sin doble conteo en enter-world repetidos).
- El smoke full-stack valida flujo TCP de gateway -> world.
- El harness de replay soporta:
  - fixtures JSON estables (`replay_cases.json`)
  - replay opcional de captura binaria (`replay_frames.bin`)
