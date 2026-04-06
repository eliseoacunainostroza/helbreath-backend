# Checklist de Release (v0.1.x)

Esta guia define la validacion minima antes de publicar una version del backend.

## 1. Preparacion

- [ ] Rama de release creada y sincronizada.
- [ ] `git status` limpio (sin cambios locales pendientes).
- [ ] Variables `.env` revisadas para entorno objetivo.
- [ ] Base de datos respaldada antes de migrar.

## 2. Build y calidad

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `RUN_FULL_STACK=1 bash deploy/scripts/verify_baseline.sh`

Comando recomendado (gate completo):

```bash
bash deploy/scripts/release_gate.sh

# opcional: autolimpieza de conflictos (systemd/procesos de smoke)
RELEASE_GATE_STOP_SYSTEMD=1 RELEASE_GATE_KILL_SMOKE_PROCS=1 bash deploy/scripts/release_gate.sh
```

## 3. Migraciones y datos

- [ ] Migraciones aplicadas sin errores:

```bash
bash deploy/scripts/migrate.sh
```

- [ ] Seed admin validado (si corresponde):

```bash
bash deploy/scripts/seed_admin.sh
```

## 4. Observabilidad

- [ ] Prometheus listo: `GET /-/ready`
- [ ] Reglas cargadas en Prometheus (`alerts.yml`)
- [ ] Grafana saludable: `GET /api/health`
- [ ] Dashboard `Helbreath Backend - Overview` visible.

## 5. Validacion funcional

- [ ] Smoke full-stack pasa (`40/40` o superior al momento de release).
- [ ] Canary check aprobado (`make canary-check`).
- [ ] Soak de estabilidad ejecutado (recomendado >= 3 iteraciones):
  `python3 deploy/scripts/soak_test.py --iterations 3 --delay-seconds 3`
- [ ] Reporte de tendencia soak actualizado: `docs/soak_stability.md`
- [ ] SLO check aprobado (`make slo-check`).
- [ ] Replay de protocolo validado (`cargo test -p net replay_cases_json_fixture`).
- [ ] Reporte de paridad real generado (`make replay-real-gap`) y brechas revisadas.
- [ ] TODO accionable de captura actualizado (`make replay-real-todo`).
- [ ] Playbook de ejecucion por escenarios actualizado (`make replay-real-playbook`).
- [ ] Checklist funcional migrada/probada actualizado (`make replay-real-checklist`).
- [ ] Refresco integral de artefactos de paridad + html (`make replay-real-refresh`).
- [ ] Gate estricto de paridad real habilitado cuando corresponda (`make replay-real-parity-check`).
- [ ] Login/character lifecycle operativos.
- [ ] Ruteo gateway -> world -> map validado.
- [ ] Portal admin (login, RBAC, auditoria) validado.
- [ ] Control de servicios (`start/restart/stop`) validado desde portal.
- [ ] Visualizacion de logs por servicio validada desde portal.

## 6. Operacion y despliegue

- [ ] Unidades systemd instaladas y habilitadas.
- [ ] Politicas de reinicio verificadas (`Restart=always`).
- [ ] Permisos sudoers para control de servicios desde admin-api instalados.
- [ ] Rotacion de logs/journal revisada.
- [ ] Backups de PostgreSQL programados.

Comando recomendado para sudoers (si se usara control desde el portal):

```bash
SUDOERS_USER=eliseo make install-admin-sudoers
```

Comandos de respaldo:

```bash
export HB_DATABASE_URL='postgres://hb:hbpass@127.0.0.1:5432/helbreath'
bash deploy/scripts/backup_postgres.sh
bash deploy/scripts/restore_latest_backup.sh
```

## 7. Cierre de release

- [ ] Documento de cambios (changelog) actualizado.
- [ ] Version/tag aplicado (ejemplo `v0.1.0`).
- [ ] Evidencia de validacion guardada (logs de baseline y smoke).
- [ ] Plan de rollback documentado.
- [ ] Politica SLO/SLA revisada: `docs/slo_sla.md`.
- [ ] Runbook canary/rollback revisado: `docs/canary_rollback_runbook.md`.
- [ ] Checklist final de publicacion revisado: `docs/publication_checklist.md`.

Tag recomendado:

```bash
VERSION=v0.1.0 make tag-release
git push origin v0.1.0
```
