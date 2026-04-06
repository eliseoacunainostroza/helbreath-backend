# Guía de Observabilidad

Este documento describe la base de observabilidad incluida en el backend.

## Stack

- Prometheus: `deploy/observability/prometheus.yml`
- Alertas Prometheus: `deploy/observability/alerts.yml`
- Provisioning de Grafana:
  - datasource: `deploy/observability/grafana/provisioning/datasources/prometheus.yml`
  - proveedor de dashboards: `deploy/observability/grafana/provisioning/dashboards/helbreath.yml`
  - dashboard JSON: `deploy/observability/grafana/dashboards/helbreath-overview.json`

## Inicio / Reinicio

```bash
cd /mnt/helbreath/helbreath-backend
bash deploy/scripts/run_observability.sh
```

Después de cambiar `prometheus.yml` o `alerts.yml`:

```bash
docker compose -f deploy/observability/docker-compose.observability.yml restart prometheus
```

Después de cambiar provisioning de Grafana o el JSON del dashboard:

```bash
docker compose -f deploy/observability/docker-compose.observability.yml restart grafana
```

## Endpoints

- UI Prometheus: `http://127.0.0.1:9090`
- Ready Prometheus: `http://127.0.0.1:9090/-/ready`
- UI Grafana: `http://127.0.0.1:3000` (`admin/admin`)
- Salud Grafana: `http://127.0.0.1:3000/api/health`

## Dashboard

Dashboard provisionado:
- Carpeta: `Helbreath`
- Nombre: `Helbreath Backend - Overview`
- UID: `helbreath-overview`

Paneles incluidos:
- Conexiones activas
- Jugadores online
- Promedio de duración de tick
- Latencia máxima de DB
- Throughput de auth/admin
- Errores de decode/persistencia/tick
- Profundidad de cola
- Estado de servicios (`up`)

## Reglas de alerta

Alertas actuales de Prometheus:
- `HbServiceDown`
- `HbTickOverrunsDetected`
- `HbHighDbLatency`
- `HbPacketDecodeErrorsSpiking`
- `HbPersistenceErrorsDetected`

Chequeo rápido:

```bash
curl -s http://127.0.0.1:9090/api/v1/rules | jq '.data.groups[].name'
```

## Contrato de métricas

Los servicios exponen ambos formatos:
- `GET /metrics` (snapshot JSON)
- `GET /metrics/prometheus` (texto Prometheus)

Métricas base:
- `hb_active_connections`
- `hb_logins_total`
- `hb_auth_failures_total`
- `hb_packet_decode_errors_total`
- `hb_command_queue_depth`
- `hb_tick_duration_ms_last`
- `hb_tick_overruns_total`
- `hb_players_online_total`
- `hb_db_latency_ms_last`
- `hb_persistence_errors_total`
- `hb_admin_actions_total`

## SLO de release

Para evaluar estabilidad antes de publicar:

```bash
# ejecutar soak y guardar historial
make soak-record

# generar tendencia
make soak-trend

# validar umbrales SLO
make slo-check
```

Documentos relacionados:
- Política SLO/SLA: `docs/slo_sla.md`
- Tendencia de estabilidad: `docs/soak_stability.md`
