// MCPlex — The MCP Smart Gateway
// Copyright (c) 2026 ModernOps888. MIT License.

#![allow(dead_code)]

use std::sync::Arc;
use std::time::Duration;
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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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

/// Dead-server monitor: receives death notifications from stdio child watchdogs,
/// cleans up multiplexer state, records dashboard events, and attempts respawn
/// with exponential backoff.
async fn dead_server_monitor(state: Arc<AppState>, mut death_rx: DeathReceiver) {
    const MAX_RESPAWN_ATTEMPTS: u32 = 5;
    const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
    const MAX_BACKOFF: Duration = Duration::from_secs(30);

    while let Some(server_name) = death_rx.recv().await {
        // Phase 1: Clean up — mark disconnected and remove from routing
        let tools_removed = {
            let mut mux = state.multiplexer.write().await;
            mux.mark_server_disconnected(&server_name)
        };

        state.metrics.record_event(EventType::ServerDisconnect {
            server_name: server_name.clone(),
            tools_removed,
        });

        // Phase 2: Respawn with exponential backoff
        let config = {
            let mux = state.multiplexer.read().await;
            mux.get_server_config(&server_name).cloned()
        };

        let Some(config) = config else {
            warn!("⚠️  No config found for '{}' — cannot respawn", server_name);
            continue;
        };

        // Only respawn stdio servers
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
        // the monitor from handling other server deaths concurrently
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
