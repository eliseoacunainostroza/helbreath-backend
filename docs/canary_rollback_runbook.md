# Runbook Canary y Rollback

Este runbook define el despliegue canario y el rollback operativo para el backend Rust.

## 1. Objetivo

- Reducir riesgo antes de publicar a todos los jugadores.
- Tener un procedimiento de reversa claro y rapido.

## 2. Precondiciones

- `release_gate.sh` aprobado.
- `RUN_SOAK=1` y `RUN_SLO_CHECK=1` aprobados.
- Backups de PostgreSQL recientes.
- Observabilidad arriba (Prometheus + Grafana).
- Servicios `hb-*` con `systemd` habilitados.

## 3. Canary (paso a paso)

Modo automatico (recomendado):

```bash
cd /mnt/helbreath/helbreath-backend
make canary-check
```

Parametros opcionales:

```bash
RUN_BASELINE=1 RUN_SOAK=1 RUN_SLO_CHECK=1 \
SOAK_ITERATIONS=3 SOAK_DELAY_SECONDS=3 \
make canary-check
```

### Paso A: Congelar cambios

```bash
cd /mnt/helbreath/helbreath-backend
git status
```

Esperado: sin cambios pendientes para el release.

### Paso B: Desplegar solo backend nuevo en una ventana controlada

```bash
sudo systemctl restart hb-gateway hb-auth hb-world hb-map hb-chat hb-jobs hb-admin-api
sudo systemctl status hb-gateway hb-auth hb-world hb-map hb-chat hb-jobs hb-admin-api --no-pager
```

### Paso C: Health y smoke canario

```bash
RUN_FULL_STACK=1 bash deploy/scripts/verify_baseline.sh
python3 deploy/scripts/smoke_test.py --launch --full-stack --with-db --only health --verbose
python3 deploy/scripts/smoke_test.py --launch --full-stack --with-db --only gateway.tcp.route-flow --verbose
python3 deploy/scripts/smoke_test.py --launch --full-stack --with-db --only gateway.tcp.route-flow.modern --verbose
```

### Paso D: Validar observabilidad

```bash
curl -fsS http://127.0.0.1:9090/-/ready
curl -fsS http://127.0.0.1:3000/api/health
curl -fsS http://127.0.0.1:9090/api/v1/rules
```

### Paso E: Ejecutar soak corto de canario

```bash
python3 deploy/scripts/soak_test.py --iterations 3 --delay-seconds 3 --full-stack --with-db --json-output .smoke/reports/soak_canary.json
python3 deploy/scripts/evaluate_release_slo.py --input .smoke/reports/soak_canary.json
```

### Paso F: Decision

- Si todo pasa: continuar a despliegue completo.
- Si algo falla: aplicar rollback inmediato.

## 4. Criterios de rollback inmediato

Rollback directo si ocurre cualquiera:

- `evaluate_release_slo.py` en estado FAIL.
- `HbServiceDown` sostenida > 5 min en servicios criticos.
- Fallas repetidas de login/admin o flujo TCP gateway.
- `HbHighDbLatency` o `HbTickOverrunsDetected` sostenidas > 10 min.

## 5. Rollback operativo

### Rollback A: Revertir binarios/servicios

1. Detener servicios:

```bash
sudo systemctl stop hb-gateway hb-auth hb-world hb-map hb-chat hb-jobs hb-admin-api
```

2. Restaurar binarios estables previos (ruta segun tu politica de artefactos).

3. Levantar servicios:

```bash
sudo systemctl start hb-gateway hb-auth hb-world hb-map hb-chat hb-jobs hb-admin-api
```

### Rollback B: Verificacion minima

```bash
sudo systemctl status hb-gateway hb-auth hb-world hb-map hb-chat hb-jobs hb-admin-api --no-pager
python3 deploy/scripts/smoke_test.py --launch --full-stack --with-db --only health --verbose
```

### Rollback C: Datos (solo si aplica)

Si hubo migracion incompatible o daño de datos:

```bash
export HB_DATABASE_URL='postgres://hb:hbpass@127.0.0.1:5432/helbreath'
make restore-db-last
```

## 6. Evidencia obligatoria post-evento

Guardar:

- logs de `release_gate.sh`
- resultado de `soak_test.py`
- salida de `evaluate_release_slo.py`
- resumen de alertas Prometheus
- causa raiz y correccion propuesta

## 7. Comando recomendado de gate final

```bash
RELEASE_GATE_STOP_SYSTEMD=1 RELEASE_GATE_KILL_SMOKE_PROCS=1 \
RUN_SOAK=1 RUN_SLO_CHECK=1 \
SLO_MIN_ITERATIONS=3 SLO_MAX_FAILED_ITERATIONS=0 \
SLO_MIN_PASS_RATE=100 SLO_MAX_AVG_ITERATION_SECONDS=30 SLO_MAX_P95_ITERATION_SECONDS=35 \
bash deploy/scripts/release_gate.sh
```
