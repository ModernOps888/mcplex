// MCPlex — Configuration Module
// Hot-reloadable TOML configuration with CLI overrides

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tracing::{error, info, warn};

/// Root configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub gateway: GatewayConfig,
    #[serde(default)]
    pub router: RouterConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    pub servers: Vec<ServerConfig>,
    #[serde(default)]
    pub roles: HashMap<String, RoleConfig>,
    /// API key → role mapping for multi-tenant access
    #[serde(default)]
    pub api_keys: HashMap<String, ApiKeyConfig>,
    /// Optional: Forward events to AgentLens for visualization
    /// MCPlex works 100% independently without this — opt-in only
    #[serde(default)]
    pub agentlens: AgentLensConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    /// Address to listen for MCP client connections
    #[serde(default = "default_listen")]
    pub listen: String,
    /// Address for the observability dashboard
    pub dashboard: Option<String>,
    /// Enable hot-reload of configuration
    #[serde(default = "default_true")]
    pub hot_reload: bool,
    /// Server name for identification
    #[serde(default = "default_server_name")]
    pub name: String,
    /// API key for gateway authentication (optional — if set, clients must provide it)
    /// Supports ${ENV_VAR} expansion, e.g. "${MCPLEX_API_KEY}"
    pub api_key: Option<String>,
    /// Maximum requests per second per client (0 = unlimited)
    #[serde(default)]
    pub rate_limit_rps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterConfig {
    /// Routing strategy: "semantic", "keyword", or "passthrough"
    #[serde(default = "default_strategy")]
    pub strategy: RouterStrategy,
    /// Routing mode: "metatool", "passthrough", or "legacy"
    /// - metatool: Expose 3 gateway meta-tools (works with ALL standard MCP clients)
    /// - passthrough: Return all real tools directly (no routing indirection)
    /// - legacy: Use _mcplex_query param extension (requires custom client)
    #[serde(default = "default_mode")]
    pub mode: RouterMode,
    /// Number of top tools to return per query
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    /// Cache tool embeddings for faster routing
    #[serde(default = "default_true")]
    pub cache_embeddings: bool,
    /// Minimum similarity score threshold (0.0 - 1.0)
    #[serde(default = "default_threshold")]
    pub similarity_threshold: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Enable role-based access control
    #[serde(default)]
    pub enable_rbac: bool,
    /// Enable structured audit logging
    #[serde(default)]
    pub enable_audit_log: bool,
    /// Path for audit log file
    #[serde(default = "default_audit_path")]
    pub audit_log_path: String,
    /// Maximum audit log file size in MB before rotation
    #[serde(default = "default_max_log_size")]
    pub max_log_size_mb: u64,
}

/// Cache configuration for tool response caching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Enable tool response caching
    #[serde(default)]
    pub enabled: bool,
    /// TTL in seconds for cached responses
    #[serde(default = "default_cache_ttl")]
    pub ttl_seconds: u64,
    /// Maximum number of cached entries
    #[serde(default = "default_cache_max")]
    pub max_entries: usize,
    /// Tool patterns to cache (empty = auto-detect read-only tools)
    #[serde(default)]
    pub patterns: Vec<String>,
}

/// API key configuration for multi-tenant access
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyConfig {
    /// Role name this API key maps to
    pub role: String,
    /// Description of this key holder
    #[serde(default)]
    pub description: String,
    /// Whether this key is active
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Unique name for this server
    pub name: String,
    /// Command to start the server (stdio transport)
    pub command: Option<String>,
    /// Arguments for the command
    #[serde(default)]
    pub args: Vec<String>,
    /// URL for remote server (streamable HTTP transport)
    pub url: Option<String>,
    /// Transport type override
    #[serde(default)]
    pub transport: TransportType,
    /// Environment variables for the server process
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Roles allowed to access this server
    #[serde(default)]
    pub allowed_roles: Vec<String>,
    /// Specific tools to block from this server
    #[serde(default)]
    pub blocked_tools: Vec<String>,
    /// Specific tools to allow (if set, only these are allowed)
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    /// Whether this server is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleConfig {
    /// Tool patterns this role can access (glob syntax)
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    /// Tool patterns this role cannot access
    #[serde(default)]
    pub blocked_tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RouterStrategy {
    Semantic,
    Keyword,
    Passthrough,
}

/// Router mode controls how tools/list behaves
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RouterMode {
    /// Expose 3 gateway meta-tools — works with ALL standard MCP clients
    MetaTool,
    /// Return all real tools directly — no routing indirection
    Passthrough,
    /// Use _mcplex_query param extension — requires custom client (deprecated)
    Legacy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum TransportType {
    Stdio,
    #[serde(rename = "streamable-http")]
    StreamableHttp,
    #[default]
    Auto,
}

// Defaults
fn default_listen() -> String {
    "127.0.0.1:3100".to_string()
}
fn default_server_name() -> String {
    "mcplex".to_string()
}
fn default_true() -> bool {
    true
}
fn default_strategy() -> RouterStrategy {
    RouterStrategy::Keyword
}
fn default_mode() -> RouterMode {
    RouterMode::MetaTool
}
fn default_top_k() -> usize {
    5
}
fn default_threshold() -> f32 {
    0.3
}
fn default_audit_path() -> String {
    "./logs/audit.jsonl".to_string()
}
fn default_max_log_size() -> u64 {
    100
}
fn default_cache_ttl() -> u64 {
    300
}
fn default_cache_max() -> usize {
    1000
}
fn default_agentlens_url() -> String {
    "http://127.0.0.1:3000/api/ingest".to_string()
}
fn default_agentlens_session() -> String {
    "MCPlex Gateway".to_string()
}

/// Optional integration with AgentLens observability platform.
/// When enabled, MCPlex forwards tool call events to AgentLens for
/// visualization in the timeline replay UI. MCPlex works perfectly
/// without this — it is entirely opt-in.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentLensConfig {
    /// Enable the AgentLens bridge (default: false)
    #[serde(default)]
    pub enabled: bool,
    /// AgentLens ingest API URL
    #[serde(default = "default_agentlens_url")]
    pub url: String,
    /// Session name to use in AgentLens (identifies this gateway)
    #[serde(default = "default_agentlens_session")]
    pub session_name: String,
    /// Forward tool call events
    #[serde(default = "default_true")]
    pub forward_tool_calls: bool,
    /// Forward security events (RBAC blocks, rate limit hits)
    #[serde(default = "default_true")]
    pub forward_security_events: bool,
    /// Forward cache hit/miss events (noisy — default false)
    #[serde(default)]
    pub forward_cache_events: bool,
}

impl Default for AgentLensConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            url: default_agentlens_url(),
            session_name: default_agentlens_session(),
            forward_tool_calls: true,
            forward_security_events: true,
            forward_cache_events: false,
        }
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            ttl_seconds: default_cache_ttl(),
            max_entries: default_cache_max(),
            patterns: vec![],
        }
    }
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            strategy: default_strategy(),
            mode: default_mode(),
            top_k: default_top_k(),
            cache_embeddings: true,
            similarity_threshold: default_threshold(),
        }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            enable_rbac: false,
            enable_audit_log: false,
            audit_log_path: default_audit_path(),
            max_log_size_mb: default_max_log_size(),
        }
    }
}

/// Load configuration from a TOML file
pub fn load_config(path: &str) -> anyhow::Result<AppConfig> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read config file '{}': {}", path, e))?;

    // Expand environment variables in the config
    let expanded = expand_env_vars(&content);

    let config: AppConfig = toml::from_str(&expanded)
        .map_err(|e| anyhow::anyhow!("Failed to parse config file '{}': {}", path, e))?;

    validate_config(&config)?;

    Ok(config)
}

/// Expand ${ENV_VAR} references in config strings
fn expand_env_vars(content: &str) -> String {
    let mut result = content.to_string();
    // Find all ${...} patterns and replace with env values
    while let Some(start) = result.find("${") {
        if let Some(end) = result[start..].find('}') {
            let var_name = &result[start + 2..start + end];
            let value = std::env::var(var_name).unwrap_or_default();
            result = format!(
                "{}{}{}",
                &result[..start],
                value,
                &result[start + end + 1..]
            );
        } else {
            break;
        }
    }
    result
}

/// Validate configuration for logical errors
fn validate_config(config: &AppConfig) -> anyhow::Result<()> {
    if config.servers.is_empty() {
        warn!("⚠️  No MCP servers configured — gateway will have no tools available");
    }

    for server in &config.servers {
        if server.command.is_none() && server.url.is_none() {
            anyhow::bail!(
                "Server '{}' must have either 'command' (for stdio) or 'url' (for HTTP) configured",
                server.name
            );
        }
        if server.command.is_some() && server.url.is_some() {
            warn!(
                "Server '{}' has both 'command' and 'url' — 'url' will take precedence",
                server.name
            );
        }
    }

    if config.router.top_k == 0 {
        anyhow::bail!("router.top_k must be at least 1");
    }

    if config.router.similarity_threshold < 0.0 || config.router.similarity_threshold > 1.0 {
        anyhow::bail!("router.similarity_threshold must be between 0.0 and 1.0");
    }

    Ok(())
}

/// Watch configuration file for changes and hot-reload
pub async fn watch_config(config_path: &str, state: Arc<crate::AppState>) -> anyhow::Result<()> {
    use notify::{Event, EventKind, RecursiveMode, Watcher};
    use std::time::Duration;

    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        if let Ok(event) = res {
            if matches!(event.kind, EventKind::Modify(_)) {
                let _ = tx.send(());
            }
        }
    })?;

    let path = Path::new(config_path);
    if let Some(parent) = path.parent() {
        watcher.watch(parent, RecursiveMode::NonRecursive)?;
    }

    let config_path = config_path.to_string();

    // Move to async context
    tokio::task::spawn_blocking(move || {
        let _watcher = watcher; // Keep watcher alive
        loop {
            match rx.recv_timeout(Duration::from_secs(1)) {
                Ok(()) => {
                    // Debounce: wait a bit for file writes to complete
                    std::thread::sleep(Duration::from_millis(200));
                    // Drain any additional events
                    while rx.try_recv().is_ok() {}

                    info!("🔄 Config file changed, reloading...");
                    match load_config(&config_path) {
                        Ok(new_config) => {
                            let state = state.clone();
                            let new_config_clone = new_config.clone();

                            // Use a runtime handle to update state
                            let rt = tokio::runtime::Handle::current();
                            rt.block_on(async {
                                // Update config
                                *state.config.write().await = new_config_clone.clone();

                                // Update security engine
                                *state.security.write().await =
                                    crate::security::SecurityEngine::new(&new_config_clone);

                                // Update router
                                *state.router.write().await =
                                    crate::router::create_router(&new_config_clone);

                                info!("✅ Configuration reloaded successfully");
                                info!("   Servers: {}", new_config_clone.servers.len());
                                info!("   Router: {:?}", new_config_clone.router.strategy);
                            });
                        }
                        Err(e) => {
                            error!(
                                "❌ Failed to reload config: {} — keeping previous config",
                                e
                            );
                        }
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    })
    .await?;

    Ok(())
}
