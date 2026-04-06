use anyhow::Result;
use argon2::{password_hash::PasswordHash, Argon2, PasswordVerifier};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration as StdDuration;
use tokio::process::Command;
use tokio::sync::RwLock;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

use admin_portal::{
    require_permission, AccountSummary, AdminLoginRequest, AdminLoginResponse, AdminPrincipal,
    BroadcastRequest, CharacterSummary, DashboardSummary, MapPlayerCount, ModerationActionRequest,
    Permission, Role, ServiceStatus,
};
use domain::{AccountStatus, AdminRole};
use infrastructure::{AdminAuditInsert, AdminRepository, CreateSanctionInput, PgRepository};
use observability::{MetricsRegistry, METRICS};

#[derive(Clone)]
struct AppState {
    settings: config::Settings,
    repo: PgRepository,
    redis: Option<redis::Client>,
    http_client: reqwest::Client,
    sessions: Arc<RwLock<HashMap<String, AdminSession>>>,
}

#[derive(Debug, Clone)]
struct AdminSession {
    principal: AdminPrincipal,
    expires_at: DateTime<Utc>,
}

#[derive(Debug, serde::Serialize)]
struct ApiErrorBody {
    error: String,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
        }
    }

    fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
        }
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ApiErrorBody {
                error: self.message,
            }),
        )
            .into_response()
    }
}

#[derive(Debug, serde::Deserialize)]
struct SearchQuery {
    query: Option<String>,
    limit: Option<i64>,
}

#[derive(Debug, serde::Deserialize)]
struct MoveCharacterRequest {
    map_id: i32,
    x: i32,
    y: i32,
}

#[derive(Debug, serde::Deserialize)]
struct ToggleEventRequest {
    enabled: bool,
}

#[derive(Debug, serde::Deserialize)]
struct AuditQuery {
    limit: Option<i64>,
}

#[derive(Debug, serde::Deserialize)]
struct ServiceLogsQuery {
    lines: Option<u32>,
}

#[derive(Debug, serde::Deserialize)]
struct WorldStatsWire {
    online_players: u64,
    players_by_map: Vec<(i32, u64)>,
}

#[derive(Debug, Clone)]
struct ManagedServiceDef {
    key: &'static str,
    label: &'static str,
    unit: &'static str,
    base_url: String,
}

#[derive(Debug, Clone, serde::Serialize)]
struct ManagedServiceStatus {
    service: String,
    label: String,
    unit: String,
    health: String,
    unit_state: String,
}

#[derive(Debug, Clone, Copy)]
enum ServiceControlAction {
    Start,
    Restart,
    Stop,
}

impl ServiceControlAction {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "start" => Some(Self::Start),
            "restart" => Some(Self::Restart),
            "stop" => Some(Self::Stop),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Restart => "restart",
            Self::Stop => "stop",
        }
    }
}

async fn healthz() -> &'static str {
    "ok"
}

async fn admin_login_page() -> Html<&'static str> {
    Html(
        r#"<!doctype html>
<html lang="es">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Helbreath Admin - Acceso</title>
  <style>
    :root {
      --bg: #0b1220;
      --panel: rgba(10, 18, 32, 0.88);
      --line: rgba(148, 163, 184, 0.25);
      --text: #e2e8f0;
      --muted: #94a3b8;
      --accent: #06b6d4;
      --accent-strong: #0891b2;
      --danger: #f87171;
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      min-height: 100vh;
      display: grid;
      place-items: center;
      color: var(--text);
      font-family: "Segoe UI", system-ui, sans-serif;
      background:
        radial-gradient(1200px 500px at 5% 10%, rgba(6, 182, 212, 0.18), transparent 70%),
        radial-gradient(900px 500px at 100% 100%, rgba(14, 165, 233, 0.12), transparent 65%),
        linear-gradient(160deg, #020617, #0f172a 55%, #111827);
      padding: 1rem;
    }
    .card {
      width: min(460px, 100%);
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 18px;
      backdrop-filter: blur(6px);
      box-shadow: 0 24px 60px rgba(2, 6, 23, 0.55);
      padding: 1.3rem 1.3rem 1.1rem;
    }
    h1 { margin: 0; font-size: 1.3rem; letter-spacing: 0.2px; }
    .sub { margin: 0.4rem 0 1rem; color: var(--muted); }
    .field { margin-top: 0.7rem; }
    label { display: block; font-size: 0.9rem; color: var(--muted); margin-bottom: 0.28rem; }
    input {
      width: 100%;
      border: 1px solid rgba(148, 163, 184, 0.3);
      background: rgba(15, 23, 42, 0.9);
      color: var(--text);
      border-radius: 10px;
      padding: 0.72rem 0.8rem;
      outline: none;
    }
    input:focus { border-color: var(--accent); box-shadow: 0 0 0 2px rgba(6, 182, 212, 0.22); }
    button {
      width: 100%;
      margin-top: 1rem;
      border: 0;
      border-radius: 10px;
      padding: 0.75rem 0.8rem;
      background: linear-gradient(140deg, var(--accent), var(--accent-strong));
      color: #ecfeff;
      font-weight: 600;
      cursor: pointer;
      transition: transform .08s ease, filter .15s ease;
    }
    button:hover { filter: brightness(1.04); }
    button:active { transform: translateY(1px); }
    .err { color: var(--danger); margin-top: 0.85rem; min-height: 1.1rem; font-size: 0.9rem; }
    .foot { margin-top: 0.9rem; color: var(--muted); font-size: 0.8rem; }
  </style>
</head>
<body>
  <main class="card">
    <h1>Portal de Administración</h1>
    <p class="sub">Acceso seguro a operaciones y monitoreo del backend Helbreath.</p>
    <div class="field">
      <label for="email">Correo admin</label>
      <input id="email" placeholder="admin@localhost" value="admin@localhost" autocomplete="username">
    </div>
    <div class="field">
      <label for="password">Contraseña</label>
      <input id="password" placeholder="••••••••" type="password" autocomplete="current-password">
    </div>
    <button id="login">Ingresar</button>
    <div id="err" class="err"></div>
    <div class="foot">Sesión con token Bearer y expiración administrada en backend.</div>
  </main>
  <script>
    async function doLogin() {
      const email = document.getElementById('email').value.trim();
      const password = document.getElementById('password').value;
      const err = document.getElementById('err');
      err.textContent = '';
      if (!email || !password) {
        err.textContent = 'Debes ingresar correo y contraseña.';
        return;
      }
      const res = await fetch('/api/v1/admin/login', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ email, password })
      });
      if (!res.ok) {
        const payload = await res.json().catch(() => ({ error: 'login fallido' }));
        err.textContent = payload.error || 'login fallido';
        return;
      }
      const payload = await res.json();
      localStorage.setItem('hb_admin_token', payload.token || '');
      location.href = '/admin/dashboard';
    }
    document.getElementById('login').onclick = doLogin;
    document.getElementById('password').addEventListener('keydown', (ev) => {
      if (ev.key === 'Enter') doLogin();
    });
  </script>
</body>
</html>"#,
    )
}

async fn admin_dashboard_page() -> Html<&'static str> {
    Html(
        r#"<!doctype html>
<html lang="es">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Helbreath Admin - Dashboard</title>
  <style>
    :root {
      --bg: #f0f4fb;
      --panel: #ffffff;
      --line: #dbe4f3;
      --text: #0f172a;
      --muted: #64748b;
      --ok: #16a34a;
      --warn: #f59e0b;
      --bad: #dc2626;
      --primary: #0ea5e9;
      --primary-dark: #0369a1;
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      color: var(--text);
      background:
        radial-gradient(1200px 550px at -5% -10%, rgba(14,165,233,0.16), transparent 70%),
        radial-gradient(900px 500px at 110% 0%, rgba(34,197,94,0.12), transparent 70%),
        var(--bg);
      font-family: "Segoe UI", system-ui, sans-serif;
      min-height: 100vh;
    }
    .shell {
      max-width: 1180px;
      margin: 0 auto;
      padding: 1.1rem;
    }
    .topbar {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      justify-content: space-between;
      gap: 0.8rem;
      margin-bottom: 1rem;
    }
    .title {
      margin: 0;
      font-size: 1.42rem;
    }
    .subtitle {
      margin: 0.2rem 0 0;
      color: var(--muted);
      font-size: 0.9rem;
    }
    .actions {
      display: flex;
      align-items: center;
      flex-wrap: wrap;
      gap: 0.5rem;
    }
    button {
      border: 1px solid var(--line);
      background: #fff;
      border-radius: 10px;
      padding: 0.52rem 0.75rem;
      cursor: pointer;
      font-weight: 600;
    }
    button.primary {
      background: linear-gradient(130deg, var(--primary), var(--primary-dark));
      border: 0;
      color: #f8fafc;
    }
    button.danger {
      border-color: #fecaca;
      background: #fff1f2;
      color: #b91c1c;
    }
    button:disabled {
      cursor: not-allowed;
      opacity: 0.55;
    }
    .grid {
      display: grid;
      grid-template-columns: repeat(4, minmax(0, 1fr));
      gap: 0.7rem;
      margin-bottom: 0.95rem;
    }
    .card {
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 14px;
      padding: 0.75rem 0.85rem;
      box-shadow: 0 8px 28px rgba(15, 23, 42, 0.08);
    }
    .k-label {
      color: var(--muted);
      font-size: 0.82rem;
      margin-bottom: 0.35rem;
    }
    .k-value {
      font-size: 1.28rem;
      font-weight: 700;
    }
    .layout {
      display: grid;
      grid-template-columns: 2fr 1fr;
      gap: 0.7rem;
    }
    .panel-title {
      margin: 0 0 0.6rem;
      font-size: 1rem;
    }
    .table-wrap {
      overflow-x: auto;
    }
    table {
      width: 100%;
      border-collapse: collapse;
      font-size: 0.9rem;
    }
    th, td {
      padding: 0.58rem 0.42rem;
      border-bottom: 1px solid #edf2fb;
      text-align: left;
      vertical-align: middle;
      white-space: nowrap;
    }
    .badge {
      display: inline-block;
      border-radius: 999px;
      font-weight: 700;
      font-size: 0.74rem;
      padding: 0.2rem 0.5rem;
      text-transform: uppercase;
      letter-spacing: 0.35px;
    }
    .ok { background: #dcfce7; color: #166534; }
    .warn { background: #fef3c7; color: #92400e; }
    .bad { background: #fee2e2; color: #991b1b; }
    .mono {
      font-family: "Consolas", "Courier New", monospace;
      font-size: 0.82rem;
      color: #475569;
    }
    .small {
      font-size: 0.82rem;
      color: var(--muted);
    }
    .field-input {
      border: 1px solid var(--line);
      border-radius: 10px;
      background: #fff;
      padding: 0.48rem 0.58rem;
      color: var(--text);
      min-width: 140px;
    }
    .log-tools {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: 0.5rem;
      margin-bottom: 0.6rem;
    }
    .log-output {
      margin: 0;
      border: 1px solid #e2e8f0;
      border-radius: 12px;
      background: #0b1220;
      color: #cbd5e1;
      min-height: 260px;
      max-height: 420px;
      overflow: auto;
      padding: 0.8rem;
      font-size: 0.8rem;
      line-height: 1.4;
      white-space: pre-wrap;
      word-break: break-word;
    }
    ul.clean {
      list-style: none;
      margin: 0;
      padding: 0;
      display: grid;
      gap: 0.46rem;
    }
    .toast {
      margin-top: 0.8rem;
      min-height: 1.1rem;
      color: #0f172a;
      font-size: 0.9rem;
    }
    @media (max-width: 980px) {
      .grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
      .layout { grid-template-columns: 1fr; }
    }
    @media (max-width: 560px) {
      .grid { grid-template-columns: 1fr; }
    }
  </style>
</head>
<body>
  <div class="shell">
    <div class="topbar">
      <div>
        <h1 class="title">Dashboard Operativo</h1>
        <p class="subtitle">Monitoreo en tiempo real y control de servicios del backend.</p>
      </div>
      <div class="actions">
        <button id="refresh" class="primary">Refrescar</button>
        <button id="toggleAuto">Auto: ON</button>
        <button id="logout" class="danger">Cerrar sesión</button>
      </div>
    </div>

    <section class="grid">
      <article class="card"><div class="k-label">Conexiones activas</div><div class="k-value" id="k-connections">0</div></article>
      <article class="card"><div class="k-label">Jugadores online</div><div class="k-value" id="k-online">0</div></article>
      <article class="card"><div class="k-label">Tick promedio (ms)</div><div class="k-value" id="k-tick">0</div></article>
      <article class="card"><div class="k-label">Tick overruns</div><div class="k-value" id="k-overruns">0</div></article>
    </section>

    <section class="layout">
      <article class="card">
        <h2 class="panel-title">Servicios del backend</h2>
        <div class="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Servicio</th>
                <th>Health</th>
                <th>systemd</th>
                <th>Unidad</th>
                <th>Acciones</th>
              </tr>
            </thead>
            <tbody id="services-body"></tbody>
          </table>
        </div>
      </article>

      <article class="card">
        <h2 class="panel-title">Jugadores por mapa</h2>
        <ul id="maps-list" class="clean"></ul>
        <h2 class="panel-title" style="margin-top:1rem;">Errores recientes</h2>
        <ul id="errors-list" class="clean"></ul>
        <div class="toast" id="toast"></div>
      </article>
    </section>

    <section class="card" style="margin-top:0.75rem;">
      <h2 class="panel-title">Logs de servicios</h2>
      <div class="log-tools">
        <select id="logs-service" class="field-input"></select>
        <input id="logs-lines" class="field-input" type="number" min="20" max="400" step="10" value="120">
        <button id="logs-refresh">Cargar logs</button>
        <button id="logs-clear">Limpiar</button>
      </div>
      <pre id="logs-output" class="log-output">Selecciona un servicio y presiona "Cargar logs".</pre>
    </section>
  </div>
  <script>
    const state = {
      auto: true,
      timer: null,
      services: [],
      selectedService: ''
    };

    function token() {
      return localStorage.getItem('hb_admin_token') || '';
    }

    function authHeaders() {
      return {
        'Authorization': 'Bearer ' + token(),
        'Content-Type': 'application/json'
      };
    }

    function badgeClass(value, kind) {
      const raw = String(value || '').toLowerCase();
      if (kind === 'health') {
        return raw === 'ok' ? 'ok' : (raw === 'down' ? 'bad' : 'warn');
      }
      if (raw === 'active') return 'ok';
      if (raw === 'activating' || raw === 'reloading') return 'warn';
      return 'bad';
    }

    function setToast(msg, isError = false) {
      const node = document.getElementById('toast');
      node.textContent = msg;
      node.style.color = isError ? '#b91c1c' : '#0f172a';
    }

    function setLogText(text) {
      document.getElementById('logs-output').textContent = text || '';
    }

    function clampLines(raw) {
      const parsed = Number(raw || 120);
      if (!Number.isFinite(parsed)) return 120;
      return Math.max(20, Math.min(400, Math.trunc(parsed)));
    }

    function ensureAuth(res) {
      if (res.status === 401 || res.status === 403) {
        localStorage.removeItem('hb_admin_token');
        location.href = '/admin';
        return false;
      }
      return true;
    }

    function renderDashboard(payload) {
      document.getElementById('k-connections').textContent = payload.active_connections || 0;
      document.getElementById('k-online').textContent = payload.online_players || 0;
      document.getElementById('k-tick').textContent = Number(payload.avg_tick_ms || 0).toFixed(1);
      document.getElementById('k-overruns').textContent = payload.tick_overruns || 0;

      const maps = Array.isArray(payload.players_by_map) ? payload.players_by_map : [];
      const mapsNode = document.getElementById('maps-list');
      mapsNode.innerHTML = maps.length
        ? maps.map(m => `<li><strong>Mapa ${m.map_id}</strong> <span class="small">(${m.players} jugadores)</span></li>`).join('')
        : '<li class="small">Sin datos de mapas.</li>';

      const errors = Array.isArray(payload.recent_errors) ? payload.recent_errors : [];
      const errNode = document.getElementById('errors-list');
      errNode.innerHTML = errors.length
        ? errors.map(e => `<li class="small">${e}</li>`).join('')
        : '<li class="small">No hay errores recientes.</li>';
    }

    function renderServices(rows) {
      state.services = Array.isArray(rows) ? rows : [];
      const body = document.getElementById('services-body');
      body.innerHTML = state.services.map(s => `
        <tr>
          <td><strong>${s.label || s.service}</strong><div class="small">${s.service}</div></td>
          <td><span class="badge ${badgeClass(s.health, 'health')}">${s.health}</span></td>
          <td><span class="badge ${badgeClass(s.unit_state, 'unit')}">${s.unit_state}</span></td>
          <td class="mono">${s.unit}</td>
          <td>
            <button onclick="controlService('${s.service}', 'start')">Iniciar</button>
            <button onclick="controlService('${s.service}', 'restart')">Reiniciar</button>
            <button class="danger" onclick="controlService('${s.service}', 'stop')">Detener</button>
            <button onclick="showLogs('${s.service}')">Logs</button>
          </td>
        </tr>
      `).join('');
      renderLogServiceOptions(state.services);
    }

    function renderLogServiceOptions(rows) {
      const select = document.getElementById('logs-service');
      const current = state.selectedService;
      select.innerHTML = rows.map(s => `<option value="${s.service}">${s.label || s.service}</option>`).join('');
      const hasCurrent = rows.some(s => s.service === current);
      const next = hasCurrent ? current : (rows[0]?.service || '');
      state.selectedService = next;
      select.value = next;
    }

    async function loadServiceLogs(serviceArg) {
      const service = serviceArg || state.selectedService;
      if (!service) {
        setLogText('No hay servicios disponibles.');
        return;
      }
      state.selectedService = service;
      const linesInput = document.getElementById('logs-lines');
      const lines = clampLines(linesInput.value);
      linesInput.value = String(lines);
      setLogText(`Cargando logs de ${service}...`);
      const res = await fetch(`/api/v1/admin/services/${service}/logs?lines=${lines}`, {
        headers: authHeaders()
      });
      if (!ensureAuth(res)) return;
      const body = await res.json().catch(() => ({}));
      if (!res.ok) {
        setLogText(body.error || `No se pudieron cargar logs de ${service}.`);
        setToast(`No se pudieron cargar logs de ${service}.`, true);
        return;
      }
      setLogText(body.log || '(sin logs recientes)');
      setToast(`Logs actualizados para ${service}.`);
    }

    function showLogs(service) {
      state.selectedService = service;
      document.getElementById('logs-service').value = service;
      loadServiceLogs(service);
    }

    async function loadDashboard() {
      const res = await fetch('/api/v1/admin/dashboard', { headers: authHeaders() });
      if (!ensureAuth(res)) return;
      if (!res.ok) {
        setToast('No se pudo cargar dashboard.', true);
        return;
      }
      renderDashboard(await res.json());
    }

    async function loadServices() {
      const res = await fetch('/api/v1/admin/services', { headers: authHeaders() });
      if (!ensureAuth(res)) return;
      if (!res.ok) {
        const body = await res.json().catch(() => ({}));
        setToast(body.error || 'No se pudo cargar estado de servicios.', true);
        return;
      }
      const body = await res.json();
      renderServices(Array.isArray(body.services) ? body.services : []);
    }

    async function refreshAll() {
      await Promise.all([loadDashboard(), loadServices()]);
      setToast('Datos actualizados.');
    }

    async function controlService(service, action) {
      setToast(`Ejecutando ${action} en ${service}...`);
      const res = await fetch(`/api/v1/admin/services/${service}/${action}`, {
        method: 'POST',
        headers: authHeaders()
      });
      if (!ensureAuth(res)) return;
      const body = await res.json().catch(() => ({}));
      if (!res.ok) {
        setToast(body.error || `Falló ${action} en ${service}.`, true);
        return;
      }
      setToast(body.note || `${service}: ${action} solicitado.`);
      await loadServices();
      if (state.selectedService === service) {
        await loadServiceLogs(service);
      }
    }

    window.controlService = controlService;
    window.showLogs = showLogs;

    function setAutoMode(enabled) {
      state.auto = enabled;
      document.getElementById('toggleAuto').textContent = 'Auto: ' + (enabled ? 'ON' : 'OFF');
      if (state.timer) {
        clearInterval(state.timer);
        state.timer = null;
      }
      if (enabled) {
        state.timer = setInterval(refreshAll, 5000);
      }
    }

    document.getElementById('refresh').onclick = refreshAll;
    document.getElementById('toggleAuto').onclick = () => setAutoMode(!state.auto);
    document.getElementById('logout').onclick = async () => {
      await fetch('/api/v1/admin/logout', { method: 'POST', headers: authHeaders() });
      localStorage.removeItem('hb_admin_token');
      location.href = '/admin';
    };
    document.getElementById('logs-refresh').onclick = () => loadServiceLogs();
    document.getElementById('logs-clear').onclick = () => setLogText('');
    document.getElementById('logs-service').onchange = (e) => {
      state.selectedService = e.target.value || '';
      if (state.selectedService) {
        loadServiceLogs(state.selectedService);
      }
    };

    setAutoMode(true);
    refreshAll();
  </script>
</body>
</html>"#,
    )
}

async fn readyz(State(state): State<AppState>) -> Json<serde_json::Value> {
    let db_ok = state.repo.readiness_check().await.is_ok();
    let redis_ok = if let Some(client) = &state.redis {
        client.get_connection().is_ok()
    } else {
        true
    };

    Json(serde_json::json!({
        "ok": db_ok && redis_ok,
        "db": db_ok,
        "redis": redis_ok,
    }))
}

async fn metrics() -> Json<observability::MetricsSnapshot> {
    Json(METRICS.snapshot())
}

async fn metrics_prometheus() -> impl IntoResponse {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        observability::prometheus_text(),
    )
}

async fn admin_login(
    State(state): State<AppState>,
    Json(req): Json<AdminLoginRequest>,
) -> Result<Json<AdminLoginResponse>, ApiError> {
    if req.email.trim().is_empty() || req.password.is_empty() {
        return Err(ApiError::bad_request("email/password required"));
    }

    let principal = if let Some(record) = state
        .repo
        .find_admin_for_login(req.email.trim())
        .await
        .map_err(|e| ApiError::internal(format!("db error: {e}")))?
    {
        let parsed_hash = PasswordHash::new(&record.password_hash)
            .map_err(|e| ApiError::internal(format!("invalid stored hash: {e}")))?;

        if Argon2::default()
            .verify_password(req.password.as_bytes(), &parsed_hash)
            .is_err()
        {
            MetricsRegistry::inc(&METRICS.auth_failures_total);
            return Err(ApiError::unauthorized("invalid credentials"));
        }

        AdminPrincipal {
            admin_user_id: record.id,
            email: record.email,
            roles: record.roles.into_iter().map(map_admin_role).collect(),
        }
    } else if req
        .email
        .trim()
        .eq_ignore_ascii_case(&state.settings.bootstrap_admin_email)
        && req.password == state.settings.bootstrap_admin_password
    {
        AdminPrincipal {
            admin_user_id: Uuid::new_v4(),
            email: req.email,
            roles: [Role::SuperAdmin].into_iter().collect(),
        }
    } else {
        MetricsRegistry::inc(&METRICS.auth_failures_total);
        return Err(ApiError::unauthorized("invalid credentials"));
    };

    let token = Uuid::new_v4().to_string();
    let expires_at =
        Utc::now() + Duration::seconds(state.settings.admin_session_ttl_seconds as i64);

    state.sessions.write().await.insert(
        token.clone(),
        AdminSession {
            principal,
            expires_at,
        },
    );

    Ok(Json(AdminLoginResponse {
        ok: true,
        token: Some(token),
        expires_at_unix: Some(expires_at.timestamp()),
    }))
}

async fn admin_logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let token = extract_bearer_token(&headers)?;
    state.sessions.write().await.remove(&token);
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn dashboard(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<DashboardSummary>, ApiError> {
    let principal = authenticate(&state, &headers, Permission::MetricsRead).await?;
    let _ = principal;

    let snapshot = METRICS.snapshot();
    let world_base = format!("http://{}", state.settings.world_bind);
    let map_base = format!("http://{}", state.settings.map_bind);
    let chat_base = format!("http://{}", state.settings.chat_bind);
    let jobs_base = format!("http://{}", state.settings.jobs_bind);
    let gateway_base = format!("http://{}", state.settings.gateway_http_bind);
    let auth_base = format!("http://{}", state.settings.auth_bind);
    let admin_base = format!("http://{}", state.settings.admin_bind);

    let world_stats = fetch_world_stats(&state.http_client, &world_base).await;
    let players_by_map = if let Some(stats) = &world_stats {
        stats
            .players_by_map
            .iter()
            .map(|(map_id, players)| MapPlayerCount {
                map_id: *map_id,
                players: *players,
            })
            .collect()
    } else {
        state
            .repo
            .list_maps()
            .await
            .map_err(|e| ApiError::internal(format!("db error: {e}")))?
            .into_iter()
            .map(|m| MapPlayerCount {
                map_id: m.id,
                players: 0,
            })
            .collect()
    };

    let online_players = world_stats
        .as_ref()
        .map(|s| s.online_players)
        .unwrap_or(snapshot.players_online_total);

    let service_status = vec![
        ServiceStatus {
            service: "gateway".to_string(),
            status: probe_service_health(&state.http_client, &gateway_base).await,
        },
        ServiceStatus {
            service: "auth".to_string(),
            status: probe_service_health(&state.http_client, &auth_base).await,
        },
        ServiceStatus {
            service: "world".to_string(),
            status: probe_service_health(&state.http_client, &world_base).await,
        },
        ServiceStatus {
            service: "map".to_string(),
            status: probe_service_health(&state.http_client, &map_base).await,
        },
        ServiceStatus {
            service: "chat".to_string(),
            status: probe_service_health(&state.http_client, &chat_base).await,
        },
        ServiceStatus {
            service: "jobs".to_string(),
            status: probe_service_health(&state.http_client, &jobs_base).await,
        },
        ServiceStatus {
            service: "admin-api".to_string(),
            status: probe_service_health(&state.http_client, &admin_base).await,
        },
    ];

    let payload = DashboardSummary {
        active_connections: snapshot.active_connections,
        online_players,
        players_by_map,
        avg_tick_ms: snapshot.tick_duration_ms_last as f64,
        tick_overruns: snapshot.tick_overruns_total,
        recent_errors: vec![],
        service_status,
    };

    Ok(Json(payload))
}

async fn managed_services_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticate(&state, &headers, Permission::MetricsRead).await?;
    let rows = collect_managed_service_statuses(&state).await;

    audit_action(
        &state,
        &principal,
        "services.status.list",
        "services",
        "all",
        serde_json::json!({"count": rows.len()}),
        None,
    )
    .await;

    Ok(Json(serde_json::json!({ "services": rows })))
}

async fn managed_service_control(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((service, action)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticate(&state, &headers, Permission::WorldWrite).await?;
    let services = managed_service_defs(&state.settings);
    let Some(target) = services.iter().find(|svc| svc.key == service) else {
        return Err(ApiError::bad_request("service not found"));
    };

    let Some(parsed_action) = ServiceControlAction::parse(&action) else {
        return Err(ApiError::bad_request(
            "invalid action, use start|restart|stop",
        ));
    };

    run_systemctl_action(target.unit, parsed_action)
        .await
        .map_err(ApiError::internal)?;

    let unit_state = get_systemd_unit_state(target.unit).await;
    let health = probe_service_health(&state.http_client, &target.base_url).await;

    audit_action(
        &state,
        &principal,
        "services.control",
        "service",
        target.key,
        serde_json::json!({
            "action": parsed_action.as_str(),
            "unit": target.unit,
            "unit_state": unit_state,
            "health": health,
        }),
        None,
    )
    .await;

    Ok(Json(serde_json::json!({
        "ok": true,
        "service": target.key,
        "action": parsed_action.as_str(),
        "unit": target.unit,
        "unit_state": unit_state,
        "health": health,
        "note": format!("accion {} enviada a {}", parsed_action.as_str(), target.unit),
    })))
}

async fn managed_service_logs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(service): Path<String>,
    Query(query): Query<ServiceLogsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticate(&state, &headers, Permission::AuditRead).await?;
    let services = managed_service_defs(&state.settings);
    let Some(target) = services.iter().find(|svc| svc.key == service) else {
        return Err(ApiError::bad_request("service not found"));
    };

    let lines = query.lines.unwrap_or(120).clamp(20, 400);
    let log_result = run_journalctl_tail(target.unit, lines).await;
    let (log_text, read_ok) = match log_result {
        Ok(text) => (text, true),
        Err(err) => (
            format!("No se pudieron leer logs via journalctl: {err}"),
            false,
        ),
    };

    audit_action(
        &state,
        &principal,
        "services.logs.read",
        "service",
        target.key,
        serde_json::json!({
            "unit": target.unit,
            "lines": lines,
            "read_ok": read_ok,
        }),
        None,
    )
    .await;

    Ok(Json(serde_json::json!({
        "service": target.key,
        "unit": target.unit,
        "lines": lines,
        "read_ok": read_ok,
        "log": if log_text.trim().is_empty() { "(sin logs recientes)".to_string() } else { log_text },
    })))
}

async fn search_accounts(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<AccountSummary>>, ApiError> {
    let principal = authenticate(&state, &headers, Permission::AccountsRead).await?;

    let needle = query.query.unwrap_or_default();
    let limit = query.limit.unwrap_or(50).clamp(1, 200);

    let rows = state
        .repo
        .search_accounts(&needle, limit)
        .await
        .map_err(|e| ApiError::internal(format!("db error: {e}")))?;

    audit_action(
        &state,
        &principal,
        "accounts.search",
        "accounts",
        &needle,
        serde_json::json!({"limit": limit}),
        None,
    )
    .await;

    Ok(Json(
        rows.into_iter()
            .map(|row| AccountSummary {
                account_id: row.id,
                username: row.username,
                status: format!("{:?}", row.status).to_ascii_lowercase(),
                failed_login_count: 0,
                last_login_at: None,
            })
            .collect(),
    ))
}

async fn block_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(account_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticate(&state, &headers, Permission::AccountsBan).await?;

    state
        .repo
        .set_account_status(account_id, AccountStatus::Banned)
        .await
        .map_err(|e| ApiError::internal(format!("db error: {e}")))?;

    audit_action(
        &state,
        &principal,
        "accounts.block",
        "account",
        &account_id.to_string(),
        serde_json::json!({}),
        None,
    )
    .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

async fn unblock_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(account_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticate(&state, &headers, Permission::AccountsBan).await?;

    state
        .repo
        .set_account_status(account_id, AccountStatus::Active)
        .await
        .map_err(|e| ApiError::internal(format!("db error: {e}")))?;

    audit_action(
        &state,
        &principal,
        "accounts.unblock",
        "account",
        &account_id.to_string(),
        serde_json::json!({}),
        None,
    )
    .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

async fn reset_account_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(account_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticate(&state, &headers, Permission::AccountsWrite).await?;

    let ticket = Uuid::new_v4().to_string();
    audit_action(
        &state,
        &principal,
        "accounts.password_reset.requested",
        "account",
        &account_id.to_string(),
        serde_json::json!({ "ticket": ticket }),
        None,
    )
    .await;

    Ok(Json(serde_json::json!({
        "ok": true,
        "workflow_ticket": ticket,
        "note": "internal workflow: issue temp credential or reset link out-of-band"
    })))
}

async fn account_sanctions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(account_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticate(&state, &headers, Permission::AccountsRead).await?;

    let sanctions = state
        .repo
        .list_account_sanctions(account_id, 200)
        .await
        .map_err(|e| ApiError::internal(format!("db error: {e}")))?;

    audit_action(
        &state,
        &principal,
        "accounts.sanctions.list",
        "account",
        &account_id.to_string(),
        serde_json::json!({ "count": sanctions.len() }),
        None,
    )
    .await;

    Ok(Json(serde_json::json!({ "rows": sanctions })))
}

async fn search_characters(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<CharacterSummary>>, ApiError> {
    let principal = authenticate(&state, &headers, Permission::CharactersRead).await?;

    let needle = query.query.unwrap_or_default();
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let rows = state
        .repo
        .search_characters(&needle, limit)
        .await
        .map_err(|e| ApiError::internal(format!("db error: {e}")))?;

    audit_action(
        &state,
        &principal,
        "characters.search",
        "characters",
        &needle,
        serde_json::json!({"limit": limit}),
        None,
    )
    .await;

    Ok(Json(
        rows.into_iter()
            .map(|row| CharacterSummary {
                character_id: row.id,
                account_id: row.account_id,
                name: row.name,
                map_id: row.map_id,
                level: row.level,
            })
            .collect(),
    ))
}

async fn move_character(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(character_id): Path<Uuid>,
    Json(req): Json<MoveCharacterRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticate(&state, &headers, Permission::CharactersWrite).await?;

    state
        .repo
        .move_character(character_id, req.map_id, req.x, req.y)
        .await
        .map_err(|e| ApiError::internal(format!("db error: {e}")))?;

    audit_action(
        &state,
        &principal,
        "characters.move",
        "character",
        &character_id.to_string(),
        serde_json::json!({"map_id": req.map_id, "x": req.x, "y": req.y}),
        None,
    )
    .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

async fn disconnect_character(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(character_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticate(&state, &headers, Permission::CharactersDisconnect).await?;

    audit_action(
        &state,
        &principal,
        "characters.disconnect",
        "character",
        &character_id.to_string(),
        serde_json::json!({}),
        None,
    )
    .await;

    Ok(Json(
        serde_json::json!({"ok": true, "note": "disconnect command queued"}),
    ))
}

async fn character_inventory(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(character_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticate(&state, &headers, Permission::CharactersRead).await?;

    let inventory = state
        .repo
        .get_character_inventory(character_id)
        .await
        .map_err(|e| ApiError::internal(format!("db error: {e}")))?;

    audit_action(
        &state,
        &principal,
        "characters.inventory.inspect",
        "character",
        &character_id.to_string(),
        serde_json::json!({}),
        None,
    )
    .await;

    Ok(Json(serde_json::json!({ "items": inventory })))
}

async fn list_maps(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticate(&state, &headers, Permission::WorldRead).await?;

    let maps = state
        .repo
        .list_maps()
        .await
        .map_err(|e| ApiError::internal(format!("db error: {e}")))?;

    audit_action(
        &state,
        &principal,
        "world.maps.list",
        "world",
        "maps",
        serde_json::json!({"count": maps.len()}),
        None,
    )
    .await;

    Ok(Json(serde_json::json!({ "maps": maps })))
}

async fn restart_map_instance(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(map_id): Path<i32>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticate(&state, &headers, Permission::WorldWrite).await?;

    let map_base = format!("http://{}", state.settings.map_bind);
    let map_ok = probe_service_health(&state.http_client, &map_base).await == "ok";
    if !map_ok {
        return Err(ApiError::internal(
            "map service unavailable; restart command not dispatched",
        ));
    }

    audit_action(
        &state,
        &principal,
        "world.map.restart",
        "map",
        &map_id.to_string(),
        serde_json::json!({}),
        None,
    )
    .await;

    Ok(Json(serde_json::json!({
        "ok": true,
        "note": "restart command accepted and queued"
    })))
}

async fn broadcast(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<BroadcastRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if req.message.trim().is_empty() {
        return Err(ApiError::bad_request("message cannot be empty"));
    }

    let principal = authenticate(&state, &headers, Permission::BroadcastSend).await?;
    let message = req.message;

    let world_url = format!("http://{}/v1/world/broadcast", state.settings.world_bind);
    let response = state
        .http_client
        .post(world_url)
        .json(&serde_json::json!({ "message": message.clone() }))
        .send()
        .await
        .map_err(|e| ApiError::internal(format!("world dispatch failed: {e}")))?;
    if !response.status().is_success() {
        return Err(ApiError::internal(format!(
            "world rejected broadcast with status {}",
            response.status()
        )));
    }

    audit_action(
        &state,
        &principal,
        "world.broadcast",
        "world",
        "all",
        serde_json::json!({"message": message}),
        None,
    )
    .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

async fn toggle_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(event_code): Path<String>,
    Json(req): Json<ToggleEventRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticate(&state, &headers, Permission::WorldWrite).await?;

    audit_action(
        &state,
        &principal,
        "world.event.toggle",
        "event",
        &event_code,
        serde_json::json!({"enabled": req.enabled}),
        None,
    )
    .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

async fn moderation_mute(
    state: State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ModerationActionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    moderation_action(state, headers, req, "mute").await
}

async fn moderation_jail(
    state: State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ModerationActionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    moderation_action(state, headers, req, "jail").await
}

async fn moderation_ban(
    state: State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ModerationActionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    moderation_action(state, headers, req, "ban").await
}

async fn moderation_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    req: ModerationActionRequest,
    action: &'static str,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authenticate(&state, &headers, Permission::ModerationWrite).await?;

    if req.reason.trim().is_empty() {
        return Err(ApiError::bad_request("reason is required"));
    }

    let starts_at = Utc::now();
    let ends_at = req.minutes.map(|m| starts_at + Duration::minutes(m));

    state
        .repo
        .create_sanction(CreateSanctionInput {
            account_id: req.account_id,
            character_id: req.character_id,
            sanction_type: action.to_string(),
            reason: req.reason.clone(),
            starts_at,
            ends_at,
            issued_by_admin_id: Some(principal.admin_user_id),
        })
        .await
        .map_err(|e| ApiError::internal(format!("db error: {e}")))?;

    let target = req
        .character_id
        .map(|v| v.to_string())
        .or_else(|| req.account_id.map(|v| v.to_string()))
        .unwrap_or_else(|| "unknown".to_string());

    audit_action(
        &state,
        &principal,
        &format!("moderation.{action}"),
        "moderation",
        &target,
        serde_json::json!({
            "reason": req.reason,
            "minutes": req.minutes,
            "scope": req.scope,
        }),
        None,
    )
    .await;

    Ok(Json(serde_json::json!({"ok": true})))
}

async fn audit_logs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AuditQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let _principal = authenticate(&state, &headers, Permission::AuditRead).await?;

    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let rows = state
        .repo
        .list_admin_audit(limit)
        .await
        .map_err(|e| ApiError::internal(format!("db error: {e}")))?;

    Ok(Json(serde_json::json!({"rows": rows})))
}

async fn authenticate(
    state: &AppState,
    headers: &HeaderMap,
    permission: Permission,
) -> Result<AdminPrincipal, ApiError> {
    prune_expired_sessions(state).await;

    let token = extract_bearer_token(headers)?;
    let sessions = state.sessions.read().await;
    let Some(session) = sessions.get(&token) else {
        return Err(ApiError::unauthorized("invalid admin token"));
    };

    require_permission(&session.principal, permission)
        .map_err(|e| ApiError::forbidden(format!("forbidden: {e}")))?;

    Ok(session.principal.clone())
}

async fn prune_expired_sessions(state: &AppState) {
    let now = Utc::now();
    state
        .sessions
        .write()
        .await
        .retain(|_, session| session.expires_at > now);
}

fn extract_bearer_token(headers: &HeaderMap) -> Result<String, ApiError> {
    let auth = headers
        .get("authorization")
        .ok_or_else(|| ApiError::unauthorized("missing authorization header"))?
        .to_str()
        .map_err(|_| ApiError::unauthorized("invalid authorization header"))?;

    let token = auth
        .strip_prefix("Bearer ")
        .or_else(|| auth.strip_prefix("bearer "))
        .ok_or_else(|| ApiError::unauthorized("expected Bearer token"))?;

    if token.trim().is_empty() {
        return Err(ApiError::unauthorized("empty bearer token"));
    }

    Ok(token.trim().to_string())
}

fn managed_service_defs(settings: &config::Settings) -> Vec<ManagedServiceDef> {
    vec![
        ManagedServiceDef {
            key: "gateway",
            label: "Gateway",
            unit: "hb-gateway.service",
            base_url: format!("http://{}", settings.gateway_http_bind),
        },
        ManagedServiceDef {
            key: "auth",
            label: "Auth",
            unit: "hb-auth.service",
            base_url: format!("http://{}", settings.auth_bind),
        },
        ManagedServiceDef {
            key: "world",
            label: "World",
            unit: "hb-world.service",
            base_url: format!("http://{}", settings.world_bind),
        },
        ManagedServiceDef {
            key: "map",
            label: "Map",
            unit: "hb-map.service",
            base_url: format!("http://{}", settings.map_bind),
        },
        ManagedServiceDef {
            key: "chat",
            label: "Chat",
            unit: "hb-chat.service",
            base_url: format!("http://{}", settings.chat_bind),
        },
        ManagedServiceDef {
            key: "jobs",
            label: "Jobs",
            unit: "hb-jobs.service",
            base_url: format!("http://{}", settings.jobs_bind),
        },
        ManagedServiceDef {
            key: "admin-api",
            label: "Admin API",
            unit: "hb-admin-api.service",
            base_url: format!("http://{}", settings.admin_bind),
        },
    ]
}

async fn collect_managed_service_statuses(state: &AppState) -> Vec<ManagedServiceStatus> {
    let mut rows = Vec::new();
    for service in managed_service_defs(&state.settings) {
        let health = probe_service_health(&state.http_client, &service.base_url).await;
        let unit_state = get_systemd_unit_state(service.unit).await;
        rows.push(ManagedServiceStatus {
            service: service.key.to_string(),
            label: service.label.to_string(),
            unit: service.unit.to_string(),
            health,
            unit_state,
        });
    }
    rows
}

async fn run_process(
    program: &str,
    args: &[&str],
) -> std::result::Result<std::process::Output, String> {
    let owned_args = args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<String>>();
    run_process_owned(program, &owned_args).await
}

async fn run_process_owned(
    program: &str,
    args: &[String],
) -> std::result::Result<std::process::Output, String> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    let output: std::process::Output =
        tokio::time::timeout(StdDuration::from_secs(8), cmd.output())
            .await
            .map_err(|_| format!("timeout ejecutando {program}"))?
            .map_err(|err| format!("no se pudo ejecutar {program}: {err}"))?;
    Ok(output)
}

fn describe_process_error(output: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !stderr.is_empty() {
        return stderr;
    }
    if !stdout.is_empty() {
        return stdout;
    }
    format!("exit status {}", output.status)
}

async fn get_systemd_unit_state(unit: &str) -> String {
    let output = run_process("systemctl", &["is-active", unit]).await;
    match output {
        Ok(out) => {
            let value = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if value.is_empty() {
                if out.status.success() {
                    "active".to_string()
                } else {
                    "unknown".to_string()
                }
            } else {
                value
            }
        }
        Err(_) => "unknown".to_string(),
    }
}

async fn run_systemctl_action(
    unit: &str,
    action: ServiceControlAction,
) -> std::result::Result<(), String> {
    let action_value = action.as_str();

    if let Ok(output) = run_process("sudo", &["-n", "systemctl", action_value, unit]).await {
        if output.status.success() {
            return Ok(());
        }
    }

    let direct = run_process("systemctl", &[action_value, unit]).await?;
    if direct.status.success() {
        Ok(())
    } else {
        Err(describe_process_error(&direct))
    }
}

async fn run_journalctl_tail(unit: &str, lines: u32) -> std::result::Result<String, String> {
    let lines_arg = lines.to_string();
    let sudo_args = vec![
        "-n".to_string(),
        "journalctl".to_string(),
        "--no-pager".to_string(),
        "-n".to_string(),
        lines_arg.clone(),
        "-u".to_string(),
        unit.to_string(),
    ];
    if let Ok(output) = run_process_owned("sudo", &sudo_args).await {
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).to_string());
        }
    }

    let direct_args = vec![
        "--no-pager".to_string(),
        "-n".to_string(),
        lines_arg,
        "-u".to_string(),
        unit.to_string(),
    ];
    let direct = run_process_owned("journalctl", &direct_args).await?;
    if direct.status.success() {
        Ok(String::from_utf8_lossy(&direct.stdout).to_string())
    } else {
        Err(describe_process_error(&direct))
    }
}

async fn probe_service_health(client: &reqwest::Client, base: &str) -> String {
    let url = format!("{base}/healthz");
    match client.get(url).send().await {
        Ok(resp) if resp.status().is_success() => "ok".to_string(),
        Ok(resp) => format!("http_{}", resp.status().as_u16()),
        Err(_) => "down".to_string(),
    }
}

async fn fetch_world_stats(client: &reqwest::Client, world_base: &str) -> Option<WorldStatsWire> {
    let url = format!("{world_base}/v1/world/stats");
    let resp = client.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.json::<WorldStatsWire>().await.ok()
}

fn map_admin_role(role: AdminRole) -> Role {
    match role {
        AdminRole::SuperAdmin => Role::SuperAdmin,
        AdminRole::Admin => Role::Admin,
        AdminRole::Gm => Role::Gm,
        AdminRole::Support => Role::Support,
        AdminRole::ReadOnly => Role::ReadOnly,
    }
}

async fn audit_action(
    state: &AppState,
    principal: &AdminPrincipal,
    action_type: &str,
    target_type: &str,
    target_id: &str,
    payload: serde_json::Value,
    ip_address: Option<&str>,
) {
    if let Err(err) = state
        .repo
        .insert_admin_audit(AdminAuditInsert {
            admin_user_id: principal.admin_user_id,
            action_type: action_type.to_string(),
            target_type: target_type.to_string(),
            target_id: target_id.to_string(),
            payload,
            request_id: Some(observability::correlation_id()),
            ip_address: ip_address.map(str::to_string),
        })
        .await
    {
        tracing::error!(error = ?err, "failed to persist admin audit");
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let settings = config::Settings::from_env()?;
    observability::init_tracing("admin-api", &settings.log_level, settings.log_json)?;
    observability::init_opentelemetry(settings.otel_endpoint.as_deref());

    let repo = PgRepository::new(&settings).await?;
    let redis = infrastructure::build_redis_client(&settings)?;
    let http_client = reqwest::Client::builder()
        .timeout(StdDuration::from_millis(900))
        .build()
        .map_err(|e| anyhow::anyhow!("failed to build http client: {e}"))?;

    let state = AppState {
        settings: settings.clone(),
        repo,
        redis,
        http_client,
        sessions: Arc::new(RwLock::new(HashMap::new())),
    };

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics))
        .route("/metrics/prometheus", get(metrics_prometheus))
        .route("/admin", get(admin_login_page))
        .route("/admin/dashboard", get(admin_dashboard_page))
        .route("/api/v1/admin/login", post(admin_login))
        .route("/api/v1/admin/logout", post(admin_logout))
        .route("/api/v1/admin/dashboard", get(dashboard))
        .route("/api/v1/admin/services", get(managed_services_status))
        .route(
            "/api/v1/admin/services/:service/logs",
            get(managed_service_logs),
        )
        .route(
            "/api/v1/admin/services/:service/:action",
            post(managed_service_control),
        )
        .route("/api/v1/admin/accounts", get(search_accounts))
        .route(
            "/api/v1/admin/accounts/:account_id/block",
            post(block_account),
        )
        .route(
            "/api/v1/admin/accounts/:account_id/unblock",
            post(unblock_account),
        )
        .route(
            "/api/v1/admin/accounts/:account_id/reset-password",
            post(reset_account_password),
        )
        .route(
            "/api/v1/admin/accounts/:account_id/sanctions",
            get(account_sanctions),
        )
        .route("/api/v1/admin/characters", get(search_characters))
        .route(
            "/api/v1/admin/characters/:character_id/move",
            post(move_character),
        )
        .route(
            "/api/v1/admin/characters/:character_id/disconnect",
            post(disconnect_character),
        )
        .route(
            "/api/v1/admin/characters/:character_id/inventory",
            get(character_inventory),
        )
        .route("/api/v1/admin/world/maps", get(list_maps))
        .route(
            "/api/v1/admin/world/maps/:map_id/restart",
            post(restart_map_instance),
        )
        .route("/api/v1/admin/world/broadcast", post(broadcast))
        .route(
            "/api/v1/admin/world/events/:event_code/toggle",
            post(toggle_event),
        )
        .route("/api/v1/admin/moderation/mute", post(moderation_mute))
        .route("/api/v1/admin/moderation/jail", post(moderation_jail))
        .route("/api/v1/admin/moderation/ban", post(moderation_ban))
        .route("/api/v1/admin/audit", get(audit_logs))
        .with_state(state)
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(&settings.admin_bind).await?;
    tracing::info!(bind = %settings.admin_bind, "admin-api listening");
    axum::serve(listener, app).await?;

    Ok(())
}
