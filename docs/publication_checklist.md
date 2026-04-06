# Checklist Final de Publicacion

Este checklist resume los pasos finales para cerrar un release y publicar tag.

## 1) Precondiciones

- Release gate aprobado:
  - `release_gate.sh` OK
  - soak en verde
  - `slo-check` en verde
- Documentacion HTML actualizada.
- Sistema estable (`systemctl status hb-*` sin errores).

## 2) Validacion final recomendada

```bash
cd /mnt/helbreath/helbreath-backend

make canary-check
make replay-opcode-report
make slo-check
RELEASE_GATE_STOP_SYSTEMD=1 RELEASE_GATE_KILL_SMOKE_PROCS=1 RUN_SOAK=1 RUN_SLO_CHECK=1 bash deploy/scripts/release_gate.sh
```

## 3) Crear tag de version (annotated)

Opcion A (recomendada):

```bash
VERSION=v0.1.0 make tag-release
```

Opcion B:

```bash
bash deploy/scripts/tag_release.sh v0.1.0
```

## 4) Publicar branch + tag

```bash
git push origin <branch-actual>
git push origin v0.1.0
```

## 5) Evidencia minima a guardar

- log de `release_gate.sh`
- resultado de `make canary-check`
- reporte `docs/soak_stability.md`
- resultado de `make replay-opcode-report`
- hash de commit + tag publicado

## 6) Post-publicacion

```bash
sudo systemctl start hb-gateway hb-auth hb-world hb-map hb-chat hb-jobs hb-admin-api
sudo systemctl status hb-gateway hb-auth hb-world hb-map hb-chat hb-jobs hb-admin-api --no-pager
curl -fsS http://127.0.0.1:8080/healthz
curl -fsS http://127.0.0.1:9090/-/ready
```

## 7) Rollback

Si hay incidente despues de publicar, seguir:
- `docs/canary_rollback_runbook.md`
