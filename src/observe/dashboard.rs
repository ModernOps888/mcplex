// MCPlex — Dashboard Server
// Built-in web dashboard for real-time observability

use axum::{
    extract::State,
    response::{Html, IntoResponse},
    routing::get,
    Json, Router,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing::info;

use crate::AppState;

/// Dashboard server
pub struct DashboardServer;

impl DashboardServer {
    /// Start the dashboard HTTP server
    pub async fn start(addr: &str, state: Arc<AppState>) -> anyhow::Result<()> {
        let app = Router::new()
            .route("/", get(serve_dashboard))
            .route("/api/metrics", get(api_metrics))
            .route("/api/tools", get(api_tools))
            .route("/api/servers", get(api_servers))
            .route("/api/events", get(api_events))
            .route("/api/config", get(api_config))
            .layer(CorsLayer::permissive())
            .with_state(state);

        let listener = tokio::net::TcpListener::bind(addr).await?;
        info!("📊 Dashboard server started on {}", addr);
        axum::serve(listener, app).await?;

        Ok(())
    }
}

/// Serve the dashboard HTML
async fn serve_dashboard() -> impl IntoResponse {
    Html(DASHBOARD_HTML)
}

/// API: Get all metrics
async fn api_metrics(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(state.metrics.get_dashboard_data())
}

/// API: Get tool statistics
async fn api_tools(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let stats = state.metrics.get_tool_stats();
    let tools: Vec<serde_json::Value> = stats
        .values()
        .map(|s| {
            serde_json::json!({
                "name": s.name,
                "invocations": s.invocation_count,
                "successes": s.success_count,
                "errors": s.error_count,
                "avg_ms": format!("{:.1}", s.avg_duration_ms()),
                "p50_ms": s.p50(),
                "p95_ms": s.p95(),
                "p99_ms": s.p99(),
            })
        })
        .collect();
    Json(serde_json::json!({ "tools": tools }))
}

/// API: Get server statuses
async fn api_servers(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let multiplexer = state.multiplexer.read().await;
    Json(serde_json::json!({
        "servers": multiplexer.get_server_statuses()
    }))
}

/// API: Get recent events
async fn api_events(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let events = state.metrics.get_recent_events(100);
    Json(serde_json::json!({ "events": events }))
}

/// API: Get current configuration (sanitized)
async fn api_config(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let config = state.config.read().await;
    Json(serde_json::json!({
        "gateway": {
            "listen": config.gateway.listen,
            "dashboard": config.gateway.dashboard,
            "hot_reload": config.gateway.hot_reload,
        },
        "router": {
            "strategy": format!("{:?}", config.router.strategy),
            "top_k": config.router.top_k,
            "similarity_threshold": config.router.similarity_threshold,
        },
        "security": {
            "rbac_enabled": config.security.enable_rbac,
            "audit_enabled": config.security.enable_audit_log,
        },
        "servers_count": config.servers.len(),
    }))
}

/// Embedded dashboard HTML — single page, no build step needed
const DASHBOARD_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>MCPlex Dashboard — MCP Smart Gateway</title>
    <link rel="preconnect" href="https://fonts.googleapis.com">
    <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
    <link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700;800&family=JetBrains+Mono:wght@400;500&display=swap" rel="stylesheet">
    <style>
        :root {
            --bg-primary: #05080f;
            --bg-secondary: #0c1220;
            --bg-card: rgba(15, 23, 42, 0.6);
            --bg-card-hover: rgba(22, 33, 55, 0.8);
            --bg-glass: rgba(15, 23, 42, 0.45);
            --border: rgba(56, 72, 104, 0.35);
            --border-glow: rgba(6, 182, 212, 0.25);
            --text-primary: #f1f5f9;
            --text-secondary: #94a3b8;
            --text-muted: #5a6a82;
            --accent: #06b6d4;
            --accent-2: #8b5cf6;
            --accent-glow: rgba(6, 182, 212, 0.15);
            --accent-glow-strong: rgba(6, 182, 212, 0.4);
            --success: #10b981;
            --success-glow: rgba(16, 185, 129, 0.15);
            --warning: #f59e0b;
            --error: #ef4444;
            --error-glow: rgba(239, 68, 68, 0.15);
            --gradient-brand: linear-gradient(135deg, #06b6d4, #8b5cf6, #ec4899);
            --gradient-success: linear-gradient(135deg, #10b981, #06b6d4);
            --gradient-card: linear-gradient(135deg, rgba(6, 182, 212, 0.05), rgba(139, 92, 246, 0.05));
            --shadow-card: 0 4px 24px rgba(0, 0, 0, 0.3), 0 1px 2px rgba(0, 0, 0, 0.2);
            --shadow-glow: 0 0 30px rgba(6, 182, 212, 0.12);
            --radius: 16px;
            --radius-sm: 10px;
        }
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body {
            font-family: 'Inter', system-ui, -apple-system, sans-serif;
            background: var(--bg-primary);
            color: var(--text-primary);
            min-height: 100vh;
            overflow-x: hidden;
        }
        /* ─── Animated background ─── */
        body::before {
            content: '';
            position: fixed;
            top: 0; left: 0; right: 0; bottom: 0;
            background:
                radial-gradient(ellipse 80% 60% at 20% 10%, rgba(6, 182, 212, 0.07) 0%, transparent 60%),
                radial-gradient(ellipse 60% 50% at 80% 80%, rgba(139, 92, 246, 0.06) 0%, transparent 60%),
                radial-gradient(ellipse 50% 40% at 50% 50%, rgba(236, 72, 153, 0.03) 0%, transparent 60%);
            pointer-events: none;
            z-index: 0;
            animation: bgShift 20s ease-in-out infinite;
        }
        @keyframes bgShift {
            0%, 100% { opacity: 1; }
            50% { opacity: 0.7; }
        }
        /* ─── Header ─── */
        .header {
            position: relative;
            z-index: 1;
            background: var(--bg-secondary);
            border-bottom: 1px solid var(--border);
            padding: 0.875rem 2rem;
            display: flex;
            align-items: center;
            justify-content: space-between;
            backdrop-filter: blur(24px);
            -webkit-backdrop-filter: blur(24px);
        }
        .header::after {
            content: '';
            position: absolute;
            bottom: 0; left: 0; right: 0;
            height: 1px;
            background: var(--gradient-brand);
            opacity: 0.4;
        }
        .header-left {
            display: flex;
            align-items: center;
            gap: 1rem;
        }
        .logo {
            width: 36px; height: 36px;
            background: var(--gradient-brand);
            border-radius: 10px;
            display: flex;
            align-items: center;
            justify-content: center;
            font-size: 1.1rem;
            box-shadow: 0 2px 12px rgba(6, 182, 212, 0.25);
        }
        .header h1 {
            font-size: 1.25rem;
            font-weight: 700;
            letter-spacing: -0.02em;
        }
        .header h1 span {
            background: var(--gradient-brand);
            -webkit-background-clip: text;
            -webkit-text-fill-color: transparent;
            background-clip: text;
        }
        .header .version {
            font-size: 0.7rem;
            color: var(--text-muted);
            background: rgba(56, 72, 104, 0.25);
            padding: 0.15rem 0.45rem;
            border-radius: 6px;
            font-family: 'JetBrains Mono', monospace;
            font-weight: 500;
        }
        .header-right {
            display: flex;
            align-items: center;
            gap: 1.5rem;
        }
        .status-pill {
            display: flex;
            align-items: center;
            gap: 0.5rem;
            color: var(--success);
            font-size: 0.8rem;
            font-weight: 500;
            background: var(--success-glow);
            padding: 0.35rem 0.85rem;
            border-radius: 20px;
            border: 1px solid rgba(16, 185, 129, 0.2);
        }
        .status-dot {
            width: 7px; height: 7px;
            background: var(--success);
            border-radius: 50%;
            box-shadow: 0 0 8px rgba(16, 185, 129, 0.6);
            animation: dotPulse 2s ease-in-out infinite;
        }
        @keyframes dotPulse {
            0%, 100% { box-shadow: 0 0 6px rgba(16,185,129,0.6); transform: scale(1); }
            50% { box-shadow: 0 0 14px rgba(16,185,129,0.9); transform: scale(1.2); }
        }
        .refresh-hint {
            color: var(--text-muted);
            font-size: 0.7rem;
            font-family: 'JetBrains Mono', monospace;
        }
        /* ─── Container ─── */
        .container {
            position: relative;
            z-index: 1;
            max-width: 1440px;
            margin: 0 auto;
            padding: 1.5rem;
        }
        /* ─── Metric Cards ─── */
        .metrics-grid {
            display: grid;
            grid-template-columns: repeat(5, 1fr);
            gap: 1rem;
            margin-bottom: 1.5rem;
        }
        @media (max-width: 1200px) {
            .metrics-grid { grid-template-columns: repeat(3, 1fr); }
        }
        @media (max-width: 700px) {
            .metrics-grid { grid-template-columns: repeat(2, 1fr); }
        }
        .metric-card {
            position: relative;
            background: var(--bg-glass);
            backdrop-filter: blur(16px);
            -webkit-backdrop-filter: blur(16px);
            border: 1px solid var(--border);
            border-radius: var(--radius);
            padding: 1.3rem 1.4rem;
            transition: all 0.35s cubic-bezier(0.25, 0.46, 0.45, 0.94);
            overflow: hidden;
            box-shadow: var(--shadow-card);
        }
        .metric-card::before {
            content: '';
            position: absolute;
            top: 0; left: 0; right: 0;
            height: 2px;
            background: var(--gradient-brand);
            opacity: 0;
            transition: opacity 0.35s;
        }
        .metric-card:hover {
            background: var(--bg-card-hover);
            border-color: var(--border-glow);
            box-shadow: var(--shadow-card), var(--shadow-glow);
            transform: translateY(-2px);
        }
        .metric-card:hover::before { opacity: 1; }
        .metric-card .icon {
            font-size: 1.3rem;
            margin-bottom: 0.75rem;
            display: inline-block;
        }
        .metric-card .label {
            font-size: 0.7rem;
            color: var(--text-muted);
            text-transform: uppercase;
            letter-spacing: 0.08em;
            font-weight: 600;
            margin-bottom: 0.5rem;
        }
        .metric-card .value {
            font-size: 2rem;
            font-weight: 800;
            color: var(--text-primary);
            line-height: 1;
            letter-spacing: -0.03em;
            font-variant-numeric: tabular-nums;
        }
        .metric-card .subtext {
            font-size: 0.7rem;
            color: var(--text-muted);
            margin-top: 0.4rem;
            font-weight: 500;
        }
        .metric-card.accent .value { color: var(--accent); }
        .metric-card.success .value {
            background: var(--gradient-success);
            -webkit-background-clip: text;
            -webkit-text-fill-color: transparent;
            background-clip: text;
        }
        .metric-card.warning .value { color: var(--warning); }
        .metric-card.error .value { color: var(--error); }
        .metric-card.highlight {
            background: var(--gradient-card);
            border-color: rgba(6, 182, 212, 0.15);
        }
        .metric-card.highlight .value {
            font-size: 2.25rem;
            background: var(--gradient-success);
            -webkit-background-clip: text;
            -webkit-text-fill-color: transparent;
            background-clip: text;
        }
        /* ─── Sections ─── */
        .section {
            background: var(--bg-glass);
            backdrop-filter: blur(16px);
            -webkit-backdrop-filter: blur(16px);
            border: 1px solid var(--border);
            border-radius: var(--radius);
            margin-bottom: 1.5rem;
            overflow: hidden;
            box-shadow: var(--shadow-card);
            transition: border-color 0.3s;
        }
        .section:hover {
            border-color: rgba(56, 72, 104, 0.5);
        }
        .section-header {
            padding: 1rem 1.5rem;
            border-bottom: 1px solid var(--border);
            font-weight: 600;
            font-size: 0.9rem;
            display: flex;
            align-items: center;
            gap: 0.6rem;
            letter-spacing: -0.01em;
        }
        .section-header .count {
            font-size: 0.7rem;
            font-weight: 600;
            color: var(--accent);
            background: var(--accent-glow);
            padding: 0.15rem 0.5rem;
            border-radius: 10px;
            font-family: 'JetBrains Mono', monospace;
            margin-left: auto;
        }
        /* ─── Tables ─── */
        table { width: 100%; border-collapse: collapse; }
        th, td {
            padding: 0.7rem 1.25rem;
            text-align: left;
            border-bottom: 1px solid rgba(56, 72, 104, 0.2);
        }
        th {
            color: var(--text-muted);
            font-size: 0.68rem;
            text-transform: uppercase;
            letter-spacing: 0.06em;
            font-weight: 600;
            background: rgba(0, 0, 0, 0.15);
        }
        td {
            font-size: 0.85rem;
            font-variant-numeric: tabular-nums;
        }
        tr { transition: background 0.15s; }
        tr:hover { background: rgba(6, 182, 212, 0.04); }
        tbody tr:last-child td { border-bottom: none; }
        td strong {
            font-weight: 600;
            color: var(--text-primary);
        }
        .mono {
            font-family: 'JetBrains Mono', monospace;
            font-size: 0.8rem;
        }
        /* ─── Badges ─── */
        .badge {
            display: inline-flex;
            align-items: center;
            gap: 0.3rem;
            padding: 0.2rem 0.6rem;
            border-radius: 8px;
            font-size: 0.72rem;
            font-weight: 600;
            letter-spacing: 0.01em;
        }
        .badge-success {
            background: var(--success-glow);
            color: var(--success);
            border: 1px solid rgba(16, 185, 129, 0.15);
        }
        .badge-error {
            background: var(--error-glow);
            color: var(--error);
            border: 1px solid rgba(239, 68, 68, 0.15);
        }
        .badge-info {
            background: var(--accent-glow);
            color: var(--accent);
            border: 1px solid rgba(6, 182, 212, 0.15);
        }
        .badge-warning {
            background: rgba(245, 158, 11, 0.12);
            color: var(--warning);
            border: 1px solid rgba(245, 158, 11, 0.15);
        }
        .badge-dot {
            width: 5px; height: 5px;
            border-radius: 50%;
            background: currentColor;
        }
        /* ─── Event Feed ─── */
        .event-feed {
            max-height: 420px;
            overflow-y: auto;
            padding: 0.375rem;
        }
        .event-item {
            display: grid;
            grid-template-columns: 72px 110px 1fr;
            align-items: center;
            gap: 0.5rem;
            padding: 0.55rem 1rem;
            border-radius: var(--radius-sm);
            margin-bottom: 2px;
            font-size: 0.8rem;
            transition: background 0.15s;
        }
        .event-item:hover { background: rgba(6, 182, 212, 0.04); }
        .event-time {
            color: var(--text-muted);
            font-family: 'JetBrains Mono', monospace;
            font-size: 0.72rem;
        }
        .event-type {
            font-weight: 600;
            font-size: 0.78rem;
        }
        .event-detail {
            color: var(--text-secondary);
            font-size: 0.8rem;
            white-space: nowrap;
            overflow: hidden;
            text-overflow: ellipsis;
        }
        .event-detail .latency {
            font-family: 'JetBrains Mono', monospace;
            font-size: 0.72rem;
            color: var(--text-muted);
            background: rgba(56, 72, 104, 0.2);
            padding: 0.1rem 0.35rem;
            border-radius: 4px;
        }
        .event-detail .save-tag {
            color: var(--success);
            font-weight: 500;
        }
        /* ─── Grid ─── */
        .grid-2 {
            display: grid;
            grid-template-columns: 1fr 1fr;
            gap: 1.5rem;
        }
        @media (max-width: 900px) {
            .grid-2 { grid-template-columns: 1fr; }
        }
        /* ─── Scrollbar ─── */
        ::-webkit-scrollbar { width: 5px; }
        ::-webkit-scrollbar-track { background: transparent; }
        ::-webkit-scrollbar-thumb { background: var(--border); border-radius: 4px; }
        ::-webkit-scrollbar-thumb:hover { background: var(--text-muted); }
        /* ─── Animations ─── */
        @keyframes fadeSlideIn {
            from { opacity: 0; transform: translateY(8px); }
            to   { opacity: 1; transform: translateY(0); }
        }
        .metric-card, .section {
            animation: fadeSlideIn 0.4s ease-out backwards;
        }
        .metrics-grid .metric-card:nth-child(1) { animation-delay: 0.05s; }
        .metrics-grid .metric-card:nth-child(2) { animation-delay: 0.1s; }
        .metrics-grid .metric-card:nth-child(3) { animation-delay: 0.15s; }
        .metrics-grid .metric-card:nth-child(4) { animation-delay: 0.2s; }
        .metrics-grid .metric-card:nth-child(5) { animation-delay: 0.25s; }
        .grid-2 .section:nth-child(1) { animation-delay: 0.3s; }
        .grid-2 .section:nth-child(2) { animation-delay: 0.35s; }
        .container > .section:last-child { animation-delay: 0.4s; }
        /* ─── Empty state ─── */
        .empty-state {
            text-align: center;
            color: var(--text-muted);
            padding: 2.5rem 1rem;
            font-size: 0.85rem;
        }
        .empty-state .empty-icon {
            font-size: 1.5rem;
            margin-bottom: 0.5rem;
            opacity: 0.5;
        }
    </style>
</head>
<body>
    <div class="header">
        <div class="header-left">
            <div class="logo">⚡</div>
            <h1><span>MCPlex</span></h1>
            <span class="version">v0.3.0</span>
        </div>
        <div class="header-right">
            <span class="refresh-hint" id="last-update">—</span>
            <div class="status-pill">
                <div class="status-dot"></div>
                Online · <span id="uptime">0s</span>
            </div>
        </div>
    </div>
    <div class="container">
        <div class="metrics-grid" id="metrics-grid"></div>
        <div class="grid-2">
            <div class="section">
                <div class="section-header">
                    🔧 Tool Performance
                    <span class="count" id="tool-count">0</span>
                </div>
                <table>
                    <thead>
                        <tr>
                            <th>Tool Name</th>
                            <th>Calls</th>
                            <th>Avg</th>
                            <th>P95</th>
                            <th>Status</th>
                        </tr>
                    </thead>
                    <tbody id="tool-stats"></tbody>
                </table>
            </div>
            <div class="section">
                <div class="section-header">
                    📡 Server Fleet
                    <span class="count" id="server-count">0</span>
                </div>
                <table>
                    <thead>
                        <tr>
                            <th>Server</th>
                            <th>Transport</th>
                            <th>Tools</th>
                            <th>Status</th>
                        </tr>
                    </thead>
                    <tbody id="server-list"></tbody>
                </table>
            </div>
        </div>
        <div class="section">
            <div class="section-header">
                📋 Live Event Stream
                <span class="count" id="event-count">0</span>
            </div>
            <div class="event-feed" id="event-feed"></div>
        </div>
    </div>
    <script>
        let prevCounters = {};

        async function fetchData() {
            try {
                const [metrics, servers] = await Promise.all([
                    fetch('/api/metrics').then(r => r.json()),
                    fetch('/api/servers').then(r => r.json()),
                ]);
                updateMetrics(metrics);
                updateToolStats(metrics.tools || []);
                updateServers(servers.servers || []);
                updateEvents(metrics.recent_events || []);
                document.getElementById('last-update').textContent = new Date().toLocaleTimeString();
            } catch (e) {
                console.error('Failed to fetch data:', e);
            }
        }

        function trendArrow(key, current) {
            const prev = prevCounters[key] || 0;
            if (current > prev && prev > 0) return ' ↑';
            return '';
        }

        function updateMetrics(data) {
            const c = data.counters || {};
            document.getElementById('uptime').textContent = c.uptime || '0s';

            const grid = document.getElementById('metrics-grid');
            grid.innerHTML = `
                <div class="metric-card">
                    <div class="icon">📥</div>
                    <div class="label">Total Requests</div>
                    <div class="value">${(c.total_requests || 0).toLocaleString()}${trendArrow('req', c.total_requests)}</div>
                    <div class="subtext">since startup</div>
                </div>
                <div class="metric-card accent">
                    <div class="icon">🔧</div>
                    <div class="label">Tool Calls</div>
                    <div class="value">${(c.total_tool_calls || 0).toLocaleString()}${trendArrow('tc', c.total_tool_calls)}</div>
                    <div class="subtext">dispatched to upstreams</div>
                </div>
                <div class="metric-card highlight">
                    <div class="icon">🎯</div>
                    <div class="label">Tokens Saved</div>
                    <div class="value">${formatNumber(c.total_tokens_saved || 0)}</div>
                    <div class="subtext">via intelligent routing</div>
                </div>
                <div class="metric-card">
                    <div class="icon">🧠</div>
                    <div class="label">Routing Queries</div>
                    <div class="value">${(c.total_routing_queries || 0).toLocaleString()}</div>
                    <div class="subtext">semantic matches</div>
                </div>
                <div class="metric-card ${(c.total_errors || 0) > 0 ? 'error' : ''}">
                    <div class="icon">${(c.total_errors || 0) > 0 ? '⚠️' : '✅'}</div>
                    <div class="label">Errors</div>
                    <div class="value">${(c.total_errors || 0).toLocaleString()}</div>
                    <div class="subtext">${(c.total_errors || 0) === 0 ? 'all clear' : 'check event log'}</div>
                </div>
            `;
            prevCounters = { req: c.total_requests, tc: c.total_tool_calls };
        }

        function updateToolStats(tools) {
            const tbody = document.getElementById('tool-stats');
            document.getElementById('tool-count').textContent = tools.length;
            if (!tools.length) {
                // Fixes #10 UX: When requests exist but no tool calls have been
                // recorded, show a helpful hint instead of the generic empty state.
                // This addresses the macOS scenario where clients stay in
                // meta-tool discovery loops without invoking mcplex_call_tool.
                const reqCount = prevCounters.req || 0;
                const hint = reqCount > 0
                    ? `No tool calls recorded yet — gateway has served ${reqCount} request${reqCount !== 1 ? 's' : ''}. Discovery/list traffic doesn't appear here.`
                    : 'No tool calls yet';
                tbody.innerHTML = `<tr><td colspan="5"><div class="empty-state"><div class="empty-icon">🔧</div>${hint}</div></td></tr>`;
                return;
            }
            tbody.innerHTML = tools.map(t => {
                const errRate = t.invocations > 0 ? (t.errors / t.invocations * 100) : 0;
                const statusBadge = t.errors > 0
                    ? `<span class="badge badge-error"><span class="badge-dot"></span>${t.errors} err</span>`
                    : `<span class="badge badge-success"><span class="badge-dot"></span>OK</span>`;
                return `
                    <tr>
                        <td><strong>${t.name}</strong></td>
                        <td class="mono">${t.invocations}</td>
                        <td class="mono">${t.avg_ms}ms</td>
                        <td class="mono">${t.p95_ms}ms</td>
                        <td>${statusBadge}</td>
                    </tr>
                `;
            }).join('');
        }

        function updateServers(servers) {
            const tbody = document.getElementById('server-list');
            const connected = servers.filter(s => s.connected).length;
            document.getElementById('server-count').textContent = `${connected}/${servers.length}`;
            if (!servers.length) {
                tbody.innerHTML = '<tr><td colspan="4"><div class="empty-state"><div class="empty-icon">📡</div>No servers configured</div></td></tr>';
                return;
            }
            tbody.innerHTML = servers.map(s => `
                <tr>
                    <td><strong>${s.name}</strong></td>
                    <td><span class="badge badge-info">${s.transport}</span></td>
                    <td class="mono">${s.tools}</td>
                    <td>${s.connected
                        ? '<span class="badge badge-success"><span class="badge-dot"></span>Connected</span>'
                        : '<span class="badge badge-error"><span class="badge-dot"></span>Down</span>'}</td>
                </tr>
            `).join('');
        }

        function updateEvents(events) {
            const feed = document.getElementById('event-feed');
            document.getElementById('event-count').textContent = events.length;
            if (!events.length) {
                feed.innerHTML = '<div class="empty-state"><div class="empty-icon">📋</div>Waiting for events…</div>';
                return;
            }
            const typeConfig = {
                tool_call:     { color: 'var(--accent)',         icon: '🔧' },
                routing:       { color: 'var(--success)',        icon: '🧠' },
                request:       { color: 'var(--text-secondary)', icon: '📥' },
                tool_blocked:  { color: 'var(--error)',          icon: '🚫' },
                server_disconnect: { color: 'var(--warning)',    icon: '⚠️' },
                server_reconnect:  { color: 'var(--success)',    icon: '🔄' },
            };
            feed.innerHTML = events.slice(0, 50).map(e => {
                const time = new Date(e.timestamp).toLocaleTimeString();
                const cfg = typeConfig[e.event_type] || { color: 'var(--text-primary)', icon: '•' };
                const latency = e.duration_ms ? `<span class="latency">${e.duration_ms}ms</span>` : '';
                const saved = e.tokens_saved ? `<span class="save-tag">🎯 ${e.tokens_saved} saved</span>` : '';
                return `
                    <div class="event-item">
                        <div class="event-time">${time}</div>
                        <div class="event-type" style="color:${cfg.color}">${cfg.icon} ${e.event_type}</div>
                        <div class="event-detail">${e.tool_name || e.query || ''} ${latency} ${saved}</div>
                    </div>
                `;
            }).join('');
        }

        function formatNumber(n) {
            if (n >= 1000000) return (n / 1000000).toFixed(1) + 'M';
            if (n >= 1000) return (n / 1000).toFixed(1) + 'K';
            return n.toString();
        }

        fetchData();
        setInterval(fetchData, 3000);
    </script>
</body>
</html>
"##;
