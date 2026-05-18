// MCPlex — The MCP Smart Gateway
// Copyright (c) 2026 ModernOps888. MIT License.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

mod config;
mod observe;
mod protocol;
mod router;
mod security;

use config::AppConfig;
use observe::dashboard::DashboardServer;
use observe::metrics::{EventType, MetricsCollector};
use protocol::cache::ToolCache;
use protocol::multiplexer::{DeathReceiver, Multiplexer};
use router::ToolRouter;
use security::SecurityEngine;

/// Shared application state accessible across all components
pub struct AppState {
    pub config: RwLock<AppConfig>,
    pub metrics: MetricsCollector,
    pub multiplexer: RwLock<Multiplexer>,
    pub security: RwLock<SecurityEngine>,
    pub router: RwLock<Box<dyn ToolRouter + Send + Sync>>,
    pub cache: ToolCache,
}

#[derive(clap::Parser)]
#[command(
    name = "mcplex",
    about = "🚀 MCPlex — The MCP Smart Gateway\n\nSemantic tool routing, security guardrails, and real-time observability for AI agents.",
    version,
    author
)]
struct Cli {
    /// Path to the configuration file
    #[arg(short, long, default_value = "mcplex.toml")]
    config: String,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Override the gateway listen address
    #[arg(long)]
    listen: Option<String>,

    /// Override the dashboard listen address
    #[arg(long)]
    dashboard: Option<String>,

    /// Validate config and exit
    #[arg(long)]
    check: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Check the health of a running gateway and its connected servers
    Doctor,
}

#[tokio::main]
async fn main() {
    // Print startup banner to stderr to group fatal errors without tracing
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    eprintln!("=== mcplex startup (unix time: {}) ===", now);

    if let Err(e) = run().await {
        error!("Fatal error: {:#}", e);
        eprintln!("[mcplex] Fatal error: {:#}", e);
        std::process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
    let cli = <Cli as clap::Parser>::parse();

    // Initialize tracing
    let filter = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter)),
        )
        .with_target(false)
        .init();

    // Load configuration
    info!("📂 Loading configuration from: {}", cli.config);
    let mut app_config = config::load_config(&cli.config)?;

    // Apply CLI overrides
    if let Some(ref listen) = cli.listen {
        app_config.gateway.listen = listen.clone();
    }
    if let Some(ref dashboard) = cli.dashboard {
        app_config.gateway.dashboard = Some(dashboard.clone());
    }

    // Config check mode
    if cli.check {
        info!("✅ Configuration is valid!");
        info!("   Gateway: {}", app_config.gateway.listen);
        info!(
            "   Dashboard: {}",
            app_config
                .gateway
                .dashboard
                .as_deref()
                .unwrap_or("disabled")
        );
        info!("   Servers: {}", app_config.servers.len());
        info!("   Router: {:?}", app_config.router.strategy);
        return Ok(());
    }

    if let Some(Commands::Doctor) = cli.command {
        return handle_doctor(&app_config).await;
    }

    print_banner();

    // Initialize metrics collector
    let metrics = MetricsCollector::new();

    // Initialize security engine
    let security = SecurityEngine::new(&app_config);
    info!(
        "🔒 Security engine initialized (RBAC: {}, Audit: {})",
        app_config.security.enable_rbac, app_config.security.enable_audit_log
    );

    // Initialize the multiplexer
    let (multiplexer, death_rx) = Multiplexer::new(&app_config).await?;
    let connected_count = multiplexer
        .get_server_statuses()
        .iter()
        .filter(|s| {
            s.get("connected")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        })
        .count();
    info!(
        "🔌 Connected to {}/{} MCP server(s)",
        connected_count,
        app_config.servers.len()
    );

    // Initialize the router
    let router = router::create_router(&app_config);
    info!(
        "🧠 Router initialized: {:?} (mode={:?}, top_k={})",
        app_config.router.strategy, app_config.router.mode, app_config.router.top_k
    );

    // Initialize cache
    let cache = ToolCache::new(
        app_config.cache.ttl_seconds,
        app_config.cache.max_entries,
        app_config.cache.patterns.clone(),
    );
    if app_config.cache.enabled {
        info!(
            "📦 Response cache enabled (TTL: {}s, max: {} entries)",
            app_config.cache.ttl_seconds, app_config.cache.max_entries
        );
    }

    // Multi-tenant API keys
    if !app_config.api_keys.is_empty() {
        info!(
            "🔑 {} API key(s) configured for multi-tenant access",
            app_config.api_keys.len()
        );
    }

    // Build shared state
    let state = Arc::new(AppState {
        config: RwLock::new(app_config.clone()),
        metrics,
        multiplexer: RwLock::new(multiplexer),
        security: RwLock::new(security),
        router: RwLock::new(router),
        cache,
    });

    // Start the hot-reload watcher
    if app_config.gateway.hot_reload {
        let config_path = cli.config.clone();
        let state_clone = Arc::clone(&state);
        tokio::spawn(async move {
            if let Err(e) = config::watch_config(&config_path, state_clone).await {
                error!("Config watcher failed: {}", e);
            }
        });
        info!("🔥 Hot-reload enabled — config changes apply without restart");
    }

    // Start the dead-server monitor (handles cleanup + respawn with backoff)
    let state_for_monitor = Arc::clone(&state);
    tokio::spawn(async move {
        dead_server_monitor(state_for_monitor, death_rx).await;
    });

    // Start the MCP gateway server
    let gateway_addr = app_config.gateway.listen.clone();
    let state_for_gateway = Arc::clone(&state);
    let gateway_handle = tokio::spawn(async move {
        if let Err(e) =
            protocol::transport::start_gateway_server(&gateway_addr, state_for_gateway).await
        {
            error!("Gateway server error: {}", e);
        }
    });

    // Start the dashboard server
    if let Some(ref dashboard_addr) = app_config.gateway.dashboard {
        let addr = dashboard_addr.clone();
        let state_for_dashboard = Arc::clone(&state);
        tokio::spawn(async move {
            if let Err(e) = DashboardServer::start(&addr, state_for_dashboard).await {
                error!("Dashboard server error: {}", e);
            }
        });
        info!("📊 Dashboard available at http://{}", dashboard_addr);
    }

    info!(
        "⚡ MCPlex gateway listening on {}",
        app_config.gateway.listen
    );
    info!("   Press Ctrl+C to stop");

    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;
    warn!("🛑 Shutdown signal received, cleaning up...");

    // Graceful shutdown
    gateway_handle.abort();
    info!("👋 MCPlex stopped. Goodbye!");

    Ok(())
}

async fn handle_doctor(config: &AppConfig) -> anyhow::Result<()> {
    let listen = &config.gateway.listen;
    let url = format!("http://{}/health", listen.replace("0.0.0.0", "127.0.0.1"));

    println!("Gateway: {} [PROBING...]", url);

    let client = reqwest::Client::new();
    let resp = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Gateway: {} [UNREACHABLE]", url);
            eprintln!("Error: {}", e);
            eprintln!("Hint: Is the gateway running? Check your service manager or docker status.");
            std::process::exit(1);
        }
    };

    if !resp.status().is_success() {
        eprintln!("Gateway: {} [HTTP {}]", url, resp.status());
        std::process::exit(1);
    }

    let data: serde_json::Value = resp.json().await?;

    println!("Gateway: {} [OK, uptime {}]", url, data["uptime"].as_str().unwrap_or("unknown"));

    if let Some(dashboard) = &config.gateway.dashboard {
        println!("Dashboard: http://{} [OK]", dashboard.replace("0.0.0.0", "127.0.0.1"));
    } else {
        println!("Dashboard: disabled");
    }

    let servers = data["servers"].as_array().unwrap();
    let connected_count = servers.iter().filter(|s| s["connected"].as_bool().unwrap_or(false)).count();

    println!("Servers ({}/{} connected):", connected_count, servers.len());

    for server in servers {
        let name = server["name"].as_str().unwrap_or("unknown");
        let status = if server["connected"].as_bool().unwrap_or(false) { "[OK]" } else { "[DOWN]" };
        let tools = server["tools"].as_i64().unwrap_or(0);
        let resources = server["resources"].as_i64().unwrap_or(0);
        let prompts = server["prompts"].as_i64().unwrap_or(0);

        println!("  {} {} {} tools {} resources {} prompts", name, status, tools, resources, prompts);
    }

    let router = &data["router"];
    println!("Router: {} ({}, top_k={})", 
        router["strategy"].as_str().unwrap_or("unknown"),
        router["mode"].as_str().unwrap_or("unknown"),
        router["top_k"].as_i64().unwrap_or(0)
    );

    let cache = &data["cache"];
    if cache["enabled"].as_bool().unwrap_or(false) {
        println!("Cache: enabled (TTL {}s)", cache["ttl"].as_i64().unwrap_or(0));
    } else {
        println!("Cache: disabled");
    }

    if connected_count < config.servers.len() {
        std::process::exit(1);
    }

    Ok(())
}

fn print_banner() {
    let banner = r#"
    ╔══════════════════════════════════════════════════╗
    ║                                                  ║
    ║    ███╗   ███╗ ██████╗██████╗ ██╗     ███████╗  ║
    ║    ████╗ ████║██╔════╝██╔══██╗██║     ██╔════╝  ║
    ║    ██╔████╔██║██║     ██████╔╝██║     █████╗    ║
    ║    ██║╚██╔╝██║██║     ██╔═══╝ ██║     ██╔══╝   ║
    ║    ██║ ╚═╝ ██║╚██████╗██║     ███████╗███████╗  ║
    ║    ╚═╝     ╚═╝ ╚═════╝╚═╝     ╚══════╝╚══════╝  ║
    ║                                                  ║
    ║     The MCP Smart Gateway — v0.3.0               ║
    ║     Semantic Routing • Security • Observability  ║
    ║                                                  ║
    ╚══════════════════════════════════════════════════╝
"#;
    println!("{}", banner);
}

// ---------------------------------------------------------------------------
// Circuit Breaker — cross-cycle crash protection  (fixes #16)
//
// Tracks per-server death timestamps in a sliding window.  When a server
// exceeds `max_crashes` deaths within `window`, the breaker *trips* and
// blocks all further respawn attempts for that server until the window
// elapses (automatic recovery).
//
// Design notes (future-proofing):
//   • The struct is self-contained and has no async dependencies — easy to
//     unit-test or extract into its own module.
//   • Thresholds are stored as fields, not hard-coded constants, so they
//     can be wired to config or per-server overrides without refactoring.
//   • `is_tripped` auto-prunes stale timestamps, bounding memory to
//     O(max_crashes × num_servers).
//   • A `status()` helper returns structured state for future dashboard /
//     /health endpoint integration.
// ---------------------------------------------------------------------------

/// Per-server circuit breaker that prevents unbounded respawn loops.
///
/// A single `CircuitBreaker` instance lives in the `dead_server_monitor`
/// loop (the sole consumer of `death_rx`), so no locking is required.
struct CircuitBreaker {
    /// Sliding-window duration within which crashes are counted.
    window: Duration,
    /// Maximum crashes allowed inside `window` before the breaker trips.
    max_crashes: usize,
    /// Per-server ring of death timestamps, newest last.
    history: HashMap<String, Vec<Instant>>,
}

/// Snapshot of a single server's circuit-breaker state, useful for
/// dashboards, health endpoints, or structured logging.
#[allow(dead_code)]
struct CircuitBreakerStatus {
    /// Number of crashes recorded inside the current window.
    crashes_in_window: usize,
    /// Whether the breaker is currently tripped (respawns blocked).
    tripped: bool,
    /// Time until the oldest crash falls out of the window (i.e. when the
    /// breaker will auto-recover).  `None` if not tripped.
    recovery_in: Option<Duration>,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with the given thresholds.
    fn new(window: Duration, max_crashes: usize) -> Self {
        Self {
            window,
            max_crashes,
            history: HashMap::new(),
        }
    }

    /// Record a death event for `server_name` and return `true` if the
    /// breaker has tripped (i.e. this server should NOT be respawned).
    ///
    /// Internally prunes timestamps older than `self.window` to keep
    /// memory bounded.
    fn record_death(&mut self, server_name: &str) -> bool {
        let now = Instant::now();
        let timestamps = self.history.entry(server_name.to_owned()).or_default();

        // Prune expired timestamps outside the sliding window.
        timestamps.retain(|t| now.duration_since(*t) < self.window);

        // Record the new death event.
        timestamps.push(now);

        // Trip if we've exceeded the threshold.
        timestamps.len() >= self.max_crashes
    }

    /// Check whether respawns are currently blocked for `server_name`
    /// without recording a new event.  Also prunes stale entries.
    #[allow(dead_code)]
    fn is_tripped(&mut self, server_name: &str) -> bool {
        let now = Instant::now();
        if let Some(timestamps) = self.history.get_mut(server_name) {
            timestamps.retain(|t| now.duration_since(*t) < self.window);
            timestamps.len() >= self.max_crashes
        } else {
            false
        }
    }

    /// Return a structured snapshot of the breaker state for a server.
    /// Useful for `/health` or dashboard integration.
    #[allow(dead_code)]
    fn status(&mut self, server_name: &str) -> CircuitBreakerStatus {
        let now = Instant::now();
        let timestamps = self.history.entry(server_name.to_owned()).or_default();
        timestamps.retain(|t| now.duration_since(*t) < self.window);

        let crashes_in_window = timestamps.len();
        let tripped = crashes_in_window >= self.max_crashes;

        // Recovery happens when the oldest crash exits the window.
        let recovery_in = if tripped {
            timestamps
                .first()
                .map(|oldest| self.window.saturating_sub(now.duration_since(*oldest)))
        } else {
            None
        };

        CircuitBreakerStatus {
            crashes_in_window,
            tripped,
            recovery_in,
        }
    }

    /// Clear all history for a server (e.g. after a manual config reload
    /// or explicit operator reset).
    #[allow(dead_code)]
    fn reset(&mut self, server_name: &str) {
        self.history.remove(server_name);
    }
}

/// Dead-server monitor: receives death notifications from stdio child
/// watchdogs, cleans up multiplexer state, records dashboard events, and
/// attempts respawn with exponential backoff.
///
/// A cross-cycle `CircuitBreaker` prevents unbounded process-spawn loops
/// when a server crashes immediately after a successful `connect()`.
/// See <https://github.com/ModernOps888/mcplex/issues/16>.
async fn dead_server_monitor(state: Arc<AppState>, mut death_rx: DeathReceiver) {
    // ── Respawn-attempt constants (per cycle) ────────────────────────────
    const MAX_RESPAWN_ATTEMPTS: u32 = 5;
    const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
    const MAX_BACKOFF: Duration = Duration::from_secs(30);

    // ── Circuit breaker thresholds (cross-cycle) ─────────────────────────
    // If a server dies >= MAX_CRASHES_IN_WINDOW times within CRASH_WINDOW,
    // all further respawn attempts are blocked until the window elapses.
    const MAX_CRASHES_IN_WINDOW: usize = 5;
    const CRASH_WINDOW: Duration = Duration::from_secs(60);

    let mut breaker = CircuitBreaker::new(CRASH_WINDOW, MAX_CRASHES_IN_WINDOW);

    while let Some(server_name) = death_rx.recv().await {
        // ── Phase 1: Clean up — mark disconnected, remove from routing ──
        let tools_removed = {
            let mut mux = state.multiplexer.write().await;
            mux.mark_server_disconnected(&server_name)
        };

        state.metrics.record_event(EventType::ServerDisconnect {
            server_name: server_name.clone(),
            tools_removed,
        });

        // ── Phase 2: Circuit breaker gate ────────────────────────────────
        // Record this death and check whether the breaker has tripped.
        // This is the key fix for #16: without it, every death event
        // spawned a fresh respawn task with its own independent 1..=5
        // counter, leading to unbounded process fan-out.
        if breaker.record_death(&server_name) {
            error!(
                "🔴 Circuit breaker TRIPPED for '{}' — {} crashes in the last {:?}. \
                 Respawn suspended until the window elapses. \
                 Check server configuration / credentials. \
                 (See https://github.com/ModernOps888/mcplex/issues/16)",
                server_name, MAX_CRASHES_IN_WINDOW, CRASH_WINDOW,
            );
            continue; // Skip respawn entirely
        }

        // ── Phase 3: Respawn with exponential backoff ────────────────────
        let config = {
            let mux = state.multiplexer.read().await;
            mux.get_server_config(&server_name).cloned()
        };

        let Some(config) = config else {
            warn!("⚠️  No config found for '{}' — cannot respawn", server_name);
            continue;
        };

        // Only respawn stdio servers (SSE/HTTP servers reconnect on demand)
        if config.command.is_none() {
            continue;
        }

        let death_tx = {
            let mux = state.multiplexer.read().await;
            mux.death_tx()
        };

        let state_for_respawn = Arc::clone(&state);
        let name_for_respawn = server_name.clone();

        // Spawn respawn attempts in a separate task so we don't block
        // the monitor from handling other server deaths concurrently.
        tokio::spawn(async move {
            let mut delay = INITIAL_BACKOFF;

            for attempt in 1..=MAX_RESPAWN_ATTEMPTS {
                tokio::time::sleep(delay).await;
                info!(
                    "🔄 Respawn attempt {}/{} for '{}' (backoff: {:?})",
                    attempt, MAX_RESPAWN_ATTEMPTS, name_for_respawn, delay
                );

                match protocol::stdio::StdioConnection::connect(&config, death_tx.clone()).await {
                    Ok((conn, capabilities)) => {
                        let tools_restored = {
                            let mut mux = state_for_respawn.multiplexer.write().await;
                            mux.reconnect_server(&name_for_respawn, conn, capabilities)
                                .await
                        };

                        state_for_respawn
                            .metrics
                            .record_event(EventType::ServerReconnect {
                                server_name: name_for_respawn.clone(),
                                tools_restored,
                            });

                        info!(
                            "✅ Server '{}' respawned successfully on attempt {}",
                            name_for_respawn, attempt
                        );
                        return; // Success — exit respawn loop
                    }
                    Err(e) => {
                        warn!(
                            "🔄 Respawn attempt {}/{} for '{}' failed: {}",
                            attempt, MAX_RESPAWN_ATTEMPTS, name_for_respawn, e
                        );
                        delay = (delay * 2).min(MAX_BACKOFF);
                    }
                }
            }

            error!(
                "❌ Server '{}' failed to respawn after {} attempts — giving up",
                name_for_respawn, MAX_RESPAWN_ATTEMPTS
            );
        });
    }
}
