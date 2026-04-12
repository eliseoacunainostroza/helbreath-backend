# Helbreath Backend (Rust + PostgreSQL)

Backend nuevo para migrar Helbreath a Rust sobre Linux, con arquitectura autoritativa, observabilidad desde el inicio y portal de administración.

> Alcance actual: base sólida de migración. El detalle exacto de protocolo legacy permanece adaptable (no hardcodeado a una sola variante).

## 1) Resumen de arquitectura

- Lenguaje/runtime: Rust + Tokio.
- API HTTP y admin: Axum.
- Datos: PostgreSQL (fuente de verdad) + Redis opcional.
- Acceso a datos: SQLx.
- Observabilidad: `tracing` + logs estructurados + métricas.
- Exportación Prometheus: `GET /metrics/prometheus` en todos los servicios.
- Topología:
  - `gateway` (borde de sesión TCP)
  - `auth-service`
  - `world-service`
  - `map-server`
  - `chat-service`
  - `admin-api`
  - `jobs-runner`

Estado de integración en gateway:
- reenvía `login/logout` hacia `auth-service`
- reenvía `character_list/create/delete/select` hacia `auth-service`
- reenvía comandos de gameplay hacia `world-service`

El gameplay crítico permanece dentro del núcleo world/map (movimiento, combate, AOI, IA, inventario, skills, timers).

## 2) Diagramas

Ver `docs/diagrams.md`.

## 3) Documentación HTML (en español)

Generar HTML estático desde los `.md`:

```bash
make docs-html
# o:
python3 deploy/scripts/generate_docs_html.py
```

Salida en `docs/html/`:
- `docs/html/index.html` (entrada)
- `docs/html/readme.html`
- `docs/html/architecture.html`
- `docs/html/diagrams.html`
- `docs/html/data_dictionary.html`
- `docs/html/migration_plan.html`
- `docs/html/observability.html`
- `docs/html/release_checklist.html`
- `docs/html/net_function_inventory.html`
- `docs/html/net_legacy_parity_checklist.html`
- `docs/html/protocol_capture_todo.html`
- `docs/html/protocol_capture_playbook.html`
- `docs/html/soak_stability.html`
- `docs/html/slo_sla.html`
- `docs/html/canary_rollback_runbook.html`
- `docs/html/publication_checklist.html`

Salida personalizada:

```bash
python3 deploy/scripts/generate_docs_html.py --output /tmp/hb-docs-html
```

## 4) Árbol del proyecto

```text
helbreath-backend/
  .github/
    workflows/
      ci.yml
  Cargo.toml
  rust-toolchain.toml
  .env.example
  docker-compose.yml
  Makefile
  migrations/
    0001_init.sql
    0002_sanction_status.sql
  proto/
    helbreath_internal.proto
  deploy/
    observability/
      prometheus.yml
      alerts.yml
      docker-compose.observability.yml
      grafana/
        provisioning/
          datasources/prometheus.yml
          dashboards/helbreath.yml
        dashboards/helbreath-overview.json
    systemd/
      hb-gateway.service
      hb-auth.service
      hb-world.service
      hb-map.service
      hb-chat.service
      hb-admin-api.service
      hb-jobs.service
    scripts/
      bootstrap_ubuntu.sh
      migrate.sh
      seed_admin.sh
      install_systemd.sh
      install_admin_sudoers.sh
      run_dev_stack.sh
      verify_baseline.sh
      run_observability.sh
      generate_docs_html.py
      release_gate.sh
      canary_check.sh
      evaluate_release_slo.py
      tag_release.sh
      backup_postgres.sh
      restore_postgres.sh
      restore_latest_backup.sh
  apps/
    gateway/
    auth-service/
    world-service/
    map-server/
    chat-service/
    admin-api/
    jobs-runner/
  crates/
    config/
    domain/
    application/
    net/
    proto/
    auth/
    world/
    map_server/
    chat/
    admin_portal/
    infrastructure/
    observability/
    test_support/
  docs/
    architecture.md
    diagrams.md
    data_dictionary.md
    migration_plan.md
    observability.md
    release_checklist.md
    net_function_inventory.md
    net_legacy_parity_checklist.md
    protocol_capture_todo.md
    protocol_capture_playbook.md
    soak_stability.md
    slo_sla.md
    canary_rollback_runbook.md
    publication_checklist.md
    html/
      index.html
      readme.html
      architecture.html
      diagrams.html
      data_dictionary.html
      migration_plan.html
      observability.html
      release_checklist.html
      net_function_inventory.html
      net_legacy_parity_checklist.html
      protocol_capture_todo.html
      protocol_capture_playbook.html
      soak_stability.html
      slo_sla.html
      canary_rollback_runbook.html
      publication_checklist.html
```

## 5) Esquema PostgreSQL

La base inicial está en `migrations/0001_init.sql` e incluye:
- cuentas/sesiones
- mapas/instancias
- personajes/entidades/npc
- items/inventario/equipment
- skills
- logs de combate
- guild/chat/mail/eventos
- sanciones
- admin users/roles/permisos/sesiones
- auditoría administrativa

Diccionario de datos completo:
- `docs/data_dictionary.md`

## 6) Portal administrativo

Backend admin en `apps/admin-api`.

Frontend HTML5/CSS con foco operativo:
- `GET /admin`
- `GET /admin/dashboard`

Incluye:
- panel visual de estado por servicio (`health` + `systemd`)
- refresco automatico del dashboard
- acciones por servicio (`start`, `restart`, `stop`)
- tabla de mapas con estado activo/inactivo y activacion desde portal

Autenticación/sesiones:
- `POST /api/v1/admin/login`
- `POST /api/v1/admin/logout`
- sesiones admin con expiración (`HB_ADMIN_SESSION_TTL_SECONDS`)
- Bearer token obligatorio en rutas protegidas

Operaciones:
- Dashboard: `GET /api/v1/admin/dashboard`
- Cuentas:
  - `GET /api/v1/admin/accounts`
  - `POST /api/v1/admin/accounts/:account_id/block`
  - `POST /api/v1/admin/accounts/:account_id/unblock`
  - `POST /api/v1/admin/accounts/:account_id/reset-password`
  - `GET /api/v1/admin/accounts/:account_id/sanctions`
- Personajes:
  - `GET /api/v1/admin/characters`
  - `POST /api/v1/admin/characters/:character_id/move`
  - `POST /api/v1/admin/characters/:character_id/disconnect`
  - `GET /api/v1/admin/characters/:character_id/inventory`
- Mundo:
  - `GET /api/v1/admin/world/maps`
  - `POST /api/v1/admin/world/maps/:map_id/activate`
  - `POST /api/v1/admin/world/maps/:map_id/deactivate`
  - `POST /api/v1/admin/world/maps/:map_id/restart`
  - `POST /api/v1/admin/world/broadcast`
  - `POST /api/v1/admin/world/events/:event_code/toggle`
- Operación de servicios:
  - `GET /api/v1/admin/services`
  - `GET /api/v1/admin/services/:service/logs?lines=120`
  - `POST /api/v1/admin/services/:service/:action` (`start|restart|stop`)
- Moderación:
  - `POST /api/v1/admin/moderation/mute`
  - `POST /api/v1/admin/moderation/jail`
  - `POST /api/v1/admin/moderation/ban`
- Auditoría:
  - `GET /api/v1/admin/audit`
- Salud/ops:
  - `GET /healthz`
  - `GET /readyz`
  - `GET /metrics`
  - `GET /metrics/prometheus`

## 7) Contratos principales en Rust

- Dominio e invariantes: `crates/domain/src/lib.rs`
- Capa de comandos/use-cases: `crates/application/src/lib.rs`
- Protocolo externo aislado: `crates/net/src/lib.rs`
- Repositorios SQLx: `crates/infrastructure/src/lib.rs`
- RBAC admin: `crates/admin_portal/src/lib.rs`
- Tick loop de mapa: `crates/map_server/src/lib.rs`

## 8) Instalación Linux + PostgreSQL

Bootstrap recomendado (Ubuntu/Debian):

```bash
cd /opt/helbreath-backend
bash deploy/scripts/bootstrap_ubuntu.sh
```

Instalación manual:

```bash
sudo apt-get update
sudo apt-get install -y ca-certificates curl git build-essential pkg-config libssl-dev \
  postgresql postgresql-contrib redis-server

curl https://sh.rustup.rs -sSf | sh -s -- -y
source "$HOME/.cargo/env"
rustup default stable

sudo systemctl enable --now postgresql
sudo systemctl enable --now redis-server
```

Crear usuario/base:

```bash
sudo -u postgres psql
CREATE ROLE hb WITH LOGIN PASSWORD 'hbpass';
CREATE DATABASE helbreath OWNER hb;
\q
```

Migraciones:

```bash
export HB_DATABASE_URL='postgres://hb:hbpass@127.0.0.1:5432/helbreath'
bash deploy/scripts/migrate.sh
```

Seed admin bootstrap:

```bash
export HB_BOOTSTRAP_ADMIN_EMAIL='admin@localhost'
export HB_BOOTSTRAP_ADMIN_PASSWORD_HASH='<hash_argon2>'
bash deploy/scripts/seed_admin.sh
```

## 9) Pseudocódigo del tick loop

```text
tick_loop(map):
  cada tick_ms:
    inicio = now()

    drenar cola de comandos (hasta command_budget)
    procesar eventos programados

    input_system()
    movement_system()
    combat_system()
    ai_system()
    aoi_system()
    inventory_system()

    emitir eventos de salida
    encolar persistencia

    elapsed = now() - inicio
    metrics.tick_duration_ms_last = elapsed
    if elapsed > tick_ms:
      metrics.tick_overruns_total += 1
```

## 10) Plan de migración

Detalle completo en `docs/migration_plan.md`.

Resumen por fases:
- Fase 0: inventario legacy y definición de paridad.
- Fase 1: parser/framing + gateway + auth + sesiones.
- Fase 2: lifecycle de personaje + enter-world base.
- Fase 3: world coordinator + map skeleton + AOI/movimiento base.
- Fase 4: combate + items + inventario + skills + NPC.
- Fase 5: chat/mail/guild + admin + jobs + observabilidad completa.
- Fase 6: hardening + replay/soak + dual-run + tuning.

## 11) Estado actual de la base

Incluye:
- binarios ejecutables por servicio
- traducción de protocolo con validación fuerte por estado de sesión
- loop de mapa determinista base
- esquema SQL + migraciones
- admin API con RBAC + auditoría
- scripts de operación (baseline, smoke, observabilidad, systemd)

RBAC:
- roles: `superadmin`, `admin`, `gm`, `support`, `readonly`
- permisos base: cuentas, personajes, mundo, moderación, broadcast, auditoría, métricas

Seguridad:
- validación autoritativa
- checks de estado de sesión/comando
- rate limiting en gateway
- sesiones admin con expiración
- auditoría completa de acciones sensibles
- acciones de control `systemctl` requieren permisos del usuario del servicio (`sudo -n` o policykit)
- helper para sudoers del portal: `SUDOERS_USER=eliseo make install-admin-sudoers`

## 12) Pruebas y validación

Objetivo de pruebas:
- unit tests (domain/net/admin_portal/map_server)
- integración (auth/admin/world/map/chat/jobs)
- packet golden/replay tests
- fixtures DB (`crates/test_support`)
- health/readiness

Smoke runner (extensible):

```bash
python3 deploy/scripts/smoke_test.py --launch --with-db
python3 deploy/scripts/smoke_test.py --launch --full-stack --with-db
# soak de estabilidad (3 corridas consecutivas por defecto)
python3 deploy/scripts/soak_test.py --iterations 3 --delay-seconds 3
# equivalente:
make soak
# persistir corrida soak en JSON versionado
make soak-record
# generar tendencia markdown/html desde historico de corridas soak
make soak-trend
# evaluar umbrales SLO sobre la ultima corrida soak
make slo-check
# ejecutar secuencia canary (health + tcp legacy/modern + soak + slo opcional)
make canary-check
```

Nota:
- con `--launch`, si algun puerto de servicio esta ocupado (systemd u otro proceso),
  el smoke runner reasigna automaticamente puertos locales libres para la corrida.
- el smoke full-stack valida flujo TCP de gateway para `legacy` y `modern`.
- smoke full-stack tambien ejecuta `gateway.tcp.command-matrix` para validar comandos gameplay
  (attack/cast/pickup/drop/use/npc/chat/whisper/guild/heartbeat) con evidencia outbound + persistencia.
- `soak_test.py` repite smoke en bucle para detectar flakiness y puede exportar resumen JSON:
  `python3 deploy/scripts/soak_test.py --iterations 5 --json-output .smoke/soak_report.json`
- `soak_report.py` consolida historico y genera `docs/soak_stability.md`.
- `evaluate_release_slo.py` valida umbrales de gate (`pass_rate`, `failed`, `p95`) para decidir release.

Fixtures de replay:
- `crates/net/tests/fixtures/replay_cases.json` (estable)
- `crates/net/tests/fixtures/replay_frames.bin` (captura opcional)

Captura rapida de `replay_frames.bin` (proxy TCP local):

```bash
# 1) ejecutar proxy de captura
CAPTURE_UPSTREAM=127.0.0.1:2848 make capture-replay

# 2) apuntar cliente al proxy local (127.0.0.1:3848) y jugar flujo a capturar
# 3) detener con Ctrl+C
```

Pipeline de captura + merge + split + reporte (todo en uno):

```bash
# captura en vivo (legacy por defecto)
make replay-capture-pipeline

# captura modern
REPLAY_PROTOCOL=modern_v400 make replay-capture-pipeline

# ingerir un .bin ya capturado
REPLAY_PROTOCOL=legacy_v382 CAPTURE_INPUT=tmp/replay_capture_legacy.bin make replay-capture-ingest
```

Sin cliente disponible:

```bash
# genera captura sintetica util para validar pipeline replay end-to-end
make replay-fixture-synth

# alternativa para cubrir modern_v400 sin cliente real:
# genera frames sinteticos e ingesta como origen manual (cliente emulado)
make replay-modern-no-client
make replay-real-refresh
REAL_PARITY_PROTOCOLS=modern_v400 make replay-real-parity-check
```

Generador de casos desde binario:

```bash
python3 deploy/scripts/replay_fixture_from_bin.py \
  --input crates/net/tests/fixtures/replay_frames.bin \
  --output crates/net/tests/fixtures/replay_cases.generated.json \
  --phase in_world \
  --protocol-version legacy_v382 \
  --expect-mode opcode_command \
  --auto-phase \
  --origin capture

# merge de casos generados al fixture canonico (dedupe por frame)
python3 deploy/scripts/replay_merge_cases.py \
  --base crates/net/tests/fixtures/replay_cases.json \
  --incoming crates/net/tests/fixtures/replay_cases.generated.json \
  --output crates/net/tests/fixtures/replay_cases.json

# split por version (legacy/modern)
make replay-fixture-split

# bootstrap provisional de cobertura modern_v400 usando casos legacy
make replay-modern-seed
# (transitorio: reemplazar estos casos por capturas modernas reales cuando esten disponibles)

# generar matriz de opcodes por version
make replay-opcode-report

# reporte de brechas de paridad real (solo origen manual/capture)
make replay-real-gap

# checklist accionable de captura real por protocolo
make replay-real-todo

# playbook detallado por escenarios de captura real (A/B/C)
make replay-real-playbook

# checklist funcional de paridad legacy (migrada/probada con check)
make replay-real-checklist

# refresco completo de artefactos de paridad + docs html
make replay-real-refresh

# gate estricto de paridad real (falla si faltan comandos reales por protocolo)
make replay-real-parity-check
# opcional: exigir solo modern durante rollout
REAL_PARITY_PROTOCOLS=modern_v400 make replay-real-parity-check

# gate estricto de cliente real (solo origen capture)
make replay-capture-parity-check
# opcional: exigir solo modern en captura cliente
REAL_PARITY_PROTOCOLS=modern_v400 make replay-capture-parity-check

# validar replay fixture en tests
cargo test -p net replay_cases_json_fixture -- --nocapture

# guardrail: modern debe incluir al menos un comando con origen manual/capture
cargo test -p net replay_cases_modern_v400_fixture -- --nocapture
```

Flujo recomendado para paridad real con cliente:
- Ejecutar captura TCP (`make capture-replay`) apuntando al gateway real.
- Jugar flujo en cliente real (login, lista, create/delete, enter_world, movimiento, combate, chat, logout).
- Generar casos desde la captura con `--origin capture`.
- Merge + split + tests.
- Ejecutar `make replay-real-gap` y cerrar la lista de `faltantes_real`.
- Revisar/ejecutar checklist de acciones pendientes en `docs/protocol_capture_todo.md` (`make replay-real-todo`).
- Ejecutar playbook detallado por escenarios en `docs/protocol_capture_playbook.md` (`make replay-real-playbook`).
- Revisar checklist funcional migrada/probada en `docs/net_legacy_parity_checklist.md` (`make replay-real-checklist`).
- Cuando `faltantes_real` sea `ninguno` para los protocolos objetivo, habilitar `make replay-real-parity-check` en el gate de release.
- Para cerrar paridad de cliente real, exigir `make replay-capture-parity-check` (origen `capture`).

## 13) Ejecución local

```bash
cp .env.example .env
make up
make migrate
make smoke
make smoke-full
make baseline
make baseline-full
make docs-html
make observability-up
make release-check
make backup-db
SUDOERS_USER=eliseo make install-admin-sudoers

cargo run -p hb-admin-api
cargo run -p hb-gateway
cargo run -p hb-auth-service
cargo run -p hb-world-service
cargo run -p hb-chat-service
cargo run -p hb-jobs-runner
```

Checklist rapido para cierre de migracion en servidor (tests + DB + restart limpio):

```bash
export HB_DATABASE_URL='postgres://hb:hbpass@127.0.0.1:5432/helbreath'
make verify-stack-green
```

Opciones utiles del script:
- `RUN_SMOKE=0 make verify-stack-green` (si quieres omitir smoke temporalmente)
- `RESTART_SERVICES=0 make verify-stack-green` (si solo quieres validar suite y DB)
- `INSTALL_BINARIES=0 make verify-stack-green` (si no quieres copiar binarios a `bin/`)
- `RUN_SMOKE_LAUNCH=1 make verify-stack-green` (smoke con `--launch`; recompila debug y consume mas RAM)

Nota para VirtualBox/carpeta compartida:
- si aparece `Text file busy (os error 26)` durante `cargo`, use target local Linux:
  - `export CARGO_TARGET_DIR=/var/tmp/helbreath-cargo-target`
  - `export CARGO_INCREMENTAL=0`
  - `export CARGO_BUILD_JOBS=1`

Variables de hardening gateway:

```bash
HB_GATEWAY_IDLE_TIMEOUT_SECONDS=45
HB_GATEWAY_MAX_CONNECTIONS_PER_IP=32
HB_RATE_LIMIT_PER_SEC=40
HB_RATE_LIMIT_BURST=80
# habilita rate limit distribuido por IP en Redis (fallback local automatico)
HB_REDIS_ENABLED=true
HB_REDIS_URL=redis://127.0.0.1:6379
```

## 14) Baseline de CI

`.github/workflows/ci.yml` ejecuta:
- migraciones
- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -D warnings`
- `cargo test --workspace`
- `python3 deploy/scripts/generate_docs_html.py`
- smoke full-stack (`python3 deploy/scripts/smoke_test.py --launch --full-stack --with-db`)

Gate de release local (pre-tag):

```bash
bash deploy/scripts/release_gate.sh

# opcional: autolimpieza de conflictos antes del smoke --launch
RELEASE_GATE_STOP_SYSTEMD=1 RELEASE_GATE_KILL_SMOKE_PROCS=1 bash deploy/scripts/release_gate.sh

# opcional: incluir soak de estabilidad en el gate de release
RUN_SOAK=1 SOAK_ITERATIONS=3 SOAK_DELAY_SECONDS=3 bash deploy/scripts/release_gate.sh
# con RUN_SOAK=1 se guarda JSON en `.smoke/reports/` y se actualiza `docs/soak_stability.md`.

# opcional: exigir SLO al gate de release
RUN_SOAK=1 RUN_SLO_CHECK=1 \
SLO_MIN_ITERATIONS=3 SLO_MAX_FAILED_ITERATIONS=0 \
SLO_MIN_PASS_RATE=100 SLO_MAX_AVG_ITERATION_SECONDS=30 SLO_MAX_P95_ITERATION_SECONDS=35 \
bash deploy/scripts/release_gate.sh

# opcional: exigir paridad estricta solo con capturas de cliente real
RUN_PARITY_STRICT=1 REAL_PARITY_SOURCE_MODE=capture_only \
REAL_PARITY_PROTOCOLS=modern_v400 \
bash deploy/scripts/release_gate.sh

# secuencia canary operativa
make canary-check

# crear tag anotado de release
VERSION=v0.1.0 make tag-release
```

Checklist final de publicacion:
- `docs/publication_checklist.md`

## 15) Observabilidad

Inicio rápido:

```bash
make observability-up
# Prometheus: http://127.0.0.1:9090
# Grafana:    http://127.0.0.1:3000  (admin/admin)
```

Assets provisionados:
- Dashboard: `Helbreath / Helbreath Backend - Overview`
- Dashboard JSON: `deploy/observability/grafana/dashboards/helbreath-overview.json`
- Alertas: `deploy/observability/alerts.yml`
- Guía operativa: `docs/observability.md`
- Política SLO/SLA: `docs/slo_sla.md`
- Runbook canary/rollback: `docs/canary_rollback_runbook.md`

## 16) Despliegue con systemd

```bash
sudo cp deploy/systemd/*.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now hb-gateway hb-auth hb-world hb-map hb-chat hb-admin-api hb-jobs

# habilitar controles start/restart/stop desde el portal admin
SUDOERS_USER=eliseo make install-admin-sudoers
```

## 17) Respaldo y restauración

```bash
export HB_DATABASE_URL='postgres://hb:hbpass@127.0.0.1:5432/helbreath'
make backup-db
# restaurar un archivo específico:
BACKUP=backups/postgres/helbreath_YYYYMMDDTHHMMSSZ.dump.gz make restore-db
# restaurar automáticamente el último backup generado:
make restore-db-last
```

## 18) Próximas tareas priorizadas

1. Paridad exacta de paquetes con capturas reales por versión.
2. Profundizar reglas de gameplay (combate/skills/inventario) hasta paridad funcional.
3. Persistir y reproducir stream comando/evento para regresión determinista.
4. Wiring completo OpenTelemetry exporter.
5. Hardening adicional de gateway (caps globales + controles adaptativos anti-DoS).
6. Frontend admin más completo sobre la API actual.
