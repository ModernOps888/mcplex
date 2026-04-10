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
    <style>
        :root {
            --bg-primary: #0a0e17;
            --bg-secondary: #111827;
            --bg-card: #1a2332;
            --bg-card-hover: #1e2a3d;
            --border: #2d3748;
            --text-primary: #e2e8f0;
            --text-secondary: #94a3b8;
            --text-muted: #64748b;
            --accent: #06b6d4;
            --accent-glow: rgba(6, 182, 212, 0.3);
            --success: #10b981;
            --warning: #f59e0b;
            --error: #ef4444;
            --gradient-1: linear-gradient(135deg, #06b6d4, #8b5cf6);
            --gradient-2: linear-gradient(135deg, #10b981, #06b6d4);
        }
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body {
            font-family: 'Inter', 'Segoe UI', system-ui, -apple-system, sans-serif;
            background: var(--bg-primary);
            color: var(--text-primary);
            min-height: 100vh;
        }
        .header {
            background: var(--bg-secondary);
            border-bottom: 1px solid var(--border);
            padding: 1rem 2rem;
            display: flex;
            align-items: center;
            justify-content: space-between;
        }
        .header h1 {
            font-size: 1.5rem;
            font-weight: 700;
            background: var(--gradient-1);
            -webkit-background-clip: text;
            -webkit-text-fill-color: transparent;
            display: flex;
            align-items: center;
            gap: 0.5rem;
        }
        .header .status {
            display: flex;
            align-items: center;
            gap: 0.5rem;
            color: var(--success);
            font-size: 0.875rem;
        }
        .header .status::before {
            content: '';
            width: 8px;
            height: 8px;
            background: var(--success);
            border-radius: 50%;
            animation: pulse 2s infinite;
        }
        @keyframes pulse {
            0%, 100% { opacity: 1; }
            50% { opacity: 0.5; }
        }
        .container {
            max-width: 1400px;
            margin: 0 auto;
            padding: 1.5rem;
        }
        .metrics-grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
            gap: 1rem;
            margin-bottom: 1.5rem;
        }
        .metric-card {
            background: var(--bg-card);
            border: 1px solid var(--border);
            border-radius: 12px;
            padding: 1.25rem;
            transition: all 0.3s ease;
        }
        .metric-card:hover {
            background: var(--bg-card-hover);
            border-color: var(--accent);
            box-shadow: 0 0 20px var(--accent-glow);
        }
        .metric-card .label {
            font-size: 0.75rem;
            color: var(--text-muted);
            text-transform: uppercase;
            letter-spacing: 0.05em;
            margin-bottom: 0.5rem;
        }
        .metric-card .value {
            font-size: 2rem;
            font-weight: 700;
            color: var(--text-primary);
            line-height: 1;
        }
        .metric-card .subtext {
            font-size: 0.75rem;
            color: var(--text-secondary);
            margin-top: 0.25rem;
        }
        .metric-card.accent .value { color: var(--accent); }
        .metric-card.success .value { color: var(--success); }
        .metric-card.warning .value { color: var(--warning); }
        .section {
            background: var(--bg-card);
            border: 1px solid var(--border);
            border-radius: 12px;
            margin-bottom: 1.5rem;
            overflow: hidden;
        }
        .section-header {
            padding: 1rem 1.5rem;
            border-bottom: 1px solid var(--border);
            font-weight: 600;
            display: flex;
            align-items: center;
            gap: 0.5rem;
        }
        table {
            width: 100%;
            border-collapse: collapse;
        }
        th, td {
            padding: 0.75rem 1rem;
            text-align: left;
            border-bottom: 1px solid var(--border);
        }
        th {
            color: var(--text-muted);
            font-size: 0.75rem;
            text-transform: uppercase;
            letter-spacing: 0.05em;
            font-weight: 600;
        }
        td { font-size: 0.875rem; }
        tr:hover { background: var(--bg-card-hover); }
        .badge {
            display: inline-block;
            padding: 0.125rem 0.5rem;
            border-radius: 9999px;
            font-size: 0.75rem;
            font-weight: 500;
        }
        .badge-success { background: rgba(16, 185, 129, 0.15); color: var(--success); }
        .badge-error { background: rgba(239, 68, 68, 0.15); color: var(--error); }
        .badge-info { background: rgba(6, 182, 212, 0.15); color: var(--accent); }
        .event-feed {
            max-height: 400px;
            overflow-y: auto;
            padding: 0.5rem;
        }
        .event-item {
            display: flex;
            align-items: center;
            gap: 1rem;
            padding: 0.5rem 1rem;
            border-radius: 8px;
            margin-bottom: 0.25rem;
            font-size: 0.8rem;
            transition: background 0.2s;
        }
        .event-item:hover { background: var(--bg-card-hover); }
        .event-time {
            color: var(--text-muted);
            font-family: monospace;
            font-size: 0.75rem;
            min-width: 80px;
        }
        .event-type {
            font-weight: 600;
            min-width: 100px;
        }
        .event-detail { color: var(--text-secondary); flex: 1; }
        .grid-2 {
            display: grid;
            grid-template-columns: 1fr 1fr;
            gap: 1.5rem;
        }
        @media (max-width: 900px) {
            .grid-2 { grid-template-columns: 1fr; }
        }
        .tokens-saved {
            font-size: 2.5rem !important;
            background: var(--gradient-2);
            -webkit-background-clip: text;
            -webkit-text-fill-color: transparent;
        }
        ::-webkit-scrollbar { width: 6px; }
        ::-webkit-scrollbar-track { background: var(--bg-primary); }
        ::-webkit-scrollbar-thumb { background: var(--border); border-radius: 3px; }
        ::-webkit-scrollbar-thumb:hover { background: var(--text-muted); }
    </style>
</head>
<body>
    <div class="header">
        <h1>🚀 MCPlex Dashboard</h1>
        <div class="status">Online — <span id="uptime">0s</span></div>
    </div>
    <div class="container">
        <div class="metrics-grid" id="metrics-grid"></div>
        <div class="grid-2">
            <div class="section">
                <div class="section-header">🔧 Tool Statistics</div>
                <table>
                    <thead>
                        <tr>
                            <th>Tool</th>
                            <th>Calls</th>
                            <th>Avg</th>
                            <th>P95</th>
                            <th>Errors</th>
                        </tr>
                    </thead>
                    <tbody id="tool-stats"></tbody>
                </table>
            </div>
            <div class="section">
                <div class="section-header">📡 Connected Servers</div>
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
            <div class="section-header">📋 Live Event Feed</div>
            <div class="event-feed" id="event-feed"></div>
        </div>
    </div>
    <script>
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
            } catch (e) {
                console.error('Failed to fetch data:', e);
            }
        }
        function updateMetrics(data) {
            const c = data.counters || {};
            document.getElementById('uptime').textContent = c.uptime || '0s';
            const grid = document.getElementById('metrics-grid');
            grid.innerHTML = `
                <div class="metric-card">
                    <div class="label">Total Requests</div>
                    <div class="value">${(c.total_requests || 0).toLocaleString()}</div>
                </div>
                <div class="metric-card accent">
                    <div class="label">Tool Calls</div>
                    <div class="value">${(c.total_tool_calls || 0).toLocaleString()}</div>
                </div>
                <div class="metric-card success">
                    <div class="label">Tokens Saved</div>
                    <div class="value tokens-saved">${formatNumber(c.total_tokens_saved || 0)}</div>
                    <div class="subtext">via intelligent routing</div>
                </div>
                <div class="metric-card">
                    <div class="label">Routing Queries</div>
                    <div class="value">${(c.total_routing_queries || 0).toLocaleString()}</div>
                </div>
                <div class="metric-card ${c.total_errors > 0 ? 'warning' : ''}">
                    <div class="label">Errors</div>
                    <div class="value">${(c.total_errors || 0).toLocaleString()}</div>
                </div>
            `;
        }
        function updateToolStats(tools) {
            const tbody = document.getElementById('tool-stats');
            if (!tools.length) {
                tbody.innerHTML = '<tr><td colspan="5" style="text-align:center;color:var(--text-muted)">No tool calls yet</td></tr>';
                return;
            }
            tbody.innerHTML = tools.map(t => `
                <tr>
                    <td><strong>${t.name}</strong></td>
                    <td>${t.invocations}</td>
                    <td>${t.avg_ms}ms</td>
                    <td>${t.p95_ms}ms</td>
                    <td>${t.errors > 0 ? `<span class="badge badge-error">${t.errors}</span>` : '<span class="badge badge-success">0</span>'}</td>
                </tr>
            `).join('');
        }
        function updateServers(servers) {
            const tbody = document.getElementById('server-list');
            if (!servers.length) {
                tbody.innerHTML = '<tr><td colspan="4" style="text-align:center;color:var(--text-muted)">No servers configured</td></tr>';
                return;
            }
            tbody.innerHTML = servers.map(s => `
                <tr>
                    <td><strong>${s.name}</strong></td>
                    <td><span class="badge badge-info">${s.transport}</span></td>
                    <td>${s.tools}</td>
                    <td>${s.connected ? '<span class="badge badge-success">Connected</span>' : '<span class="badge badge-error">Disconnected</span>'}</td>
                </tr>
            `).join('');
        }
        function updateEvents(events) {
            const feed = document.getElementById('event-feed');
            if (!events.length) {
                feed.innerHTML = '<div class="event-item"><div class="event-detail" style="text-align:center;color:var(--text-muted)">Waiting for events...</div></div>';
                return;
            }
            feed.innerHTML = events.slice(0, 50).map(e => {
                const time = new Date(e.timestamp).toLocaleTimeString();
                const type_colors = { tool_call: 'var(--accent)', routing: 'var(--success)', request: 'var(--text-secondary)', tool_blocked: 'var(--error)' };
                return `
                    <div class="event-item">
                        <div class="event-time">${time}</div>
                        <div class="event-type" style="color:${type_colors[e.event_type] || 'var(--text-primary)'}">${e.event_type}</div>
                        <div class="event-detail">${e.tool_name || ''} ${e.duration_ms ? `(${e.duration_ms}ms)` : ''} ${e.tokens_saved ? `🎯 ${e.tokens_saved} tokens saved` : ''}</div>
                    </div>
                `;
            }).join('');
        }
        function formatNumber(n) {
            if (n >= 1000000) return (n / 1000000).toFixed(1) + 'M';
            if (n >= 1000) return (n / 1000).toFixed(1) + 'K';
            return n.toString();
        }
        // Initial fetch and auto-refresh
        fetchData();
        setInterval(fetchData, 3000);
    </script>
</body>
</html>
"##;
