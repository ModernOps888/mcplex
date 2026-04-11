// MCPlex — The MCP Smart Gateway
// Copyright (c) 2026 ModernOps888. MIT License.

#![allow(dead_code)]

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

mod config;
mod observe;
mod protocol;
mod router;
mod security;

use config::AppConfig;
use observe::dashboard::DashboardServer;
use observe::metrics::MetricsCollector;
use protocol::cache::ToolCache;
use protocol::multiplexer::Multiplexer;
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
    let multiplexer = Multiplexer::new(&app_config).await?;
    info!("🔌 Connected to {} MCP server(s)", app_config.servers.len());

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
    ║     The MCP Smart Gateway — v0.2.1               ║
    ║     Semantic Routing • Security • Observability  ║
    ║                                                  ║
    ╚══════════════════════════════════════════════════╝
"#;
    println!("{}", banner);
}
