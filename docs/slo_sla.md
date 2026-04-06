# SLO/SLA y Criterios de Rollback

Este documento define objetivos operativos para releases del backend Rust y
una politica clara de rollback.

## 1. Alcance

Servicios cubiertos:
- `hb-gateway`
- `hb-auth`
- `hb-world`
- `hb-map`
- `hb-chat`
- `hb-jobs`
- `hb-admin-api`

## 2. SLI (Indicadores)

Indicadores usados en pre-release:
- Exito de smoke full-stack (`deploy/scripts/smoke_test.py`).
- Exito de soak repetido (`deploy/scripts/soak_test.py`).
- Latencia por iteracion de soak (promedio y p95).

Indicadores usados en post-release:
- Salud de servicio (`/healthz`, `/readyz`).
- Alertas Prometheus:
  - `HbServiceDown`
  - `HbTickOverrunsDetected`
  - `HbHighDbLatency`
  - `HbPacketDecodeErrorsSpiking`
  - `HbPersistenceErrorsDetected`

## 3. SLO Objetivo (Release v0.1.x)

Umbrales recomendados para aprobar release:
- `iterations_executed >= 3`
- `failed_iterations <= 0`
- `pass_rate >= 100%`
- `avg_iteration_seconds <= 30`
- `p95_iteration_seconds <= 35`

Validacion automatizada:

```bash
make slo-check
```

Con ajuste de umbrales:

```bash
SLO_MIN_ITERATIONS=5 \
SLO_MAX_FAILED_ITERATIONS=0 \
SLO_MIN_PASS_RATE=100 \
SLO_MAX_AVG_ITERATION_SECONDS=30 \
SLO_MAX_P95_ITERATION_SECONDS=35 \
make slo-check
```

## 4. SLA Operativo (Objetivo de servicio)

Objetivo inicial recomendado para produccion:
- Disponibilidad mensual por servicio: `>= 99.5%`.
- Tiempo maximo de degradacion continua aceptable: `< 10 minutos`.

Nota:
- Para v0.1.x este SLA es objetivo operativo interno.
- Ajustar al alza cuando exista historial estable de soak + produccion.

## 5. Rollback (Pre-release)

Rollback obligatorio (no publicar) si ocurre cualquiera:
- Falla `release_gate.sh`.
- Falla `verify_baseline.sh`.
- Falla `slo-check`.
- Falla replay/protocolo (`cargo test -p net replay_cases_json_fixture`).

Accion:
1. Detener promocion de release.
2. Corregir causa raiz.
3. Re-ejecutar gate completo.

## 6. Rollback (Post-release)

Rollback recomendado inmediato si ocurre cualquiera:
- `HbServiceDown` sostenido por > 5 minutos en servicios criticos.
- Error masivo de login o smoke canario fallando repetidamente.
- Latencia DB o overruns de tick fuera de umbral sostenidos por > 10 minutos.

Playbook minimo:
1. Declarar incidente y congelar despliegues.
2. Restaurar version estable anterior de binarios/systemd.
3. Validar health + smoke canario.
4. Registrar incidente y acciones correctivas.

Runbook operativo detallado:
- `docs/canary_rollback_runbook.md`

## 7. Comando de Gate con SLO

```bash
RELEASE_GATE_STOP_SYSTEMD=1 RELEASE_GATE_KILL_SMOKE_PROCS=1 \
RUN_SOAK=1 SOAK_ITERATIONS=3 SOAK_DELAY_SECONDS=3 \
RUN_SLO_CHECK=1 \
SLO_MIN_ITERATIONS=3 \
SLO_MAX_FAILED_ITERATIONS=0 \
SLO_MIN_PASS_RATE=100 \
SLO_MAX_AVG_ITERATION_SECONDS=30 \
SLO_MAX_P95_ITERATION_SECONDS=35 \
bash deploy/scripts/release_gate.sh
```
