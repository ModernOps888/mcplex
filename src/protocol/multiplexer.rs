// MCPlex — Multiplexer
// Aggregates multiple MCP servers into a unified interface.
// Fixes #11: Overlapping tool names are detected at startup and disambiguated
// via fully-qualified names (server_name/tool_name) instead of crashing.
//
// HTTP servers: stateless JSON-RPC requests via reqwest (connection pooling built-in).
// Stdio servers: persistent child processes via StdioConnection (long-lived, multiplexed).

use std::collections::{HashMap, HashSet};
use tracing::{debug, error, info, warn};

use crate::config::{AppConfig, ServerConfig};
use crate::protocol::stdio::StdioConnection;
use crate::protocol::{
    PromptDefinition, RegisteredPrompt, RegisteredResource, RegisteredTool, ResourceDefinition,
    ToolCallParams, ToolDefinition,
};

/// Channel type for stdio server death notifications
pub type DeathReceiver = tokio::sync::mpsc::UnboundedReceiver<String>;
pub type DeathSender = tokio::sync::mpsc::UnboundedSender<String>;

/// Represents a connected upstream MCP server
#[derive(Debug)]
pub struct UpstreamServer {
    pub name: String,
    pub config: ServerConfig,
    pub tools: Vec<ToolDefinition>,
    pub resources: Vec<ResourceDefinition>,
    pub prompts: Vec<PromptDefinition>,
    pub connected: bool,
}

/// The Multiplexer manages connections to all upstream MCP servers
pub struct Multiplexer {
    servers: HashMap<String, UpstreamServer>,
    /// Persistent connections to stdio servers (keyed by server name)
    stdio_connections: HashMap<String, StdioConnection>,
    /// Lookup: tool_name → server_name
    tool_index: HashMap<String, String>,
    /// Lookup: resource_uri → server_name
    resource_index: HashMap<String, String>,
    /// Lookup: prompt_name → server_name
    prompt_index: HashMap<String, String>,
    /// All registered tools with their server origin
    all_tools: Vec<RegisteredTool>,
    /// All registered resources with their server origin
    all_resources: Vec<RegisteredResource>,
    /// All registered prompts with their server origin
    all_prompts: Vec<RegisteredPrompt>,
    /// Death notification sender — cloned to each stdio child watchdog
    death_tx: DeathSender,
}

impl Multiplexer {
    /// Create a new multiplexer from configuration.
    ///
    /// For each enabled server:
    /// - HTTP: sends initialize + tools/list + resources/list + prompts/list
    /// - Stdio: spawns a persistent child, performs the MCP handshake, discovers capabilities
    ///
    /// `connected` is only set to `true` if discovery actually succeeds.
    ///
    /// Returns `(Multiplexer, DeathReceiver)` — the receiver is used by the
    /// dead-server monitor task spawned in main.rs after AppState is built.
    pub async fn new(config: &AppConfig) -> anyhow::Result<(Self, DeathReceiver)> {
        let (death_tx, death_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut servers = HashMap::new();
        let mut stdio_connections = HashMap::new();
        let mut tool_index = HashMap::new();
        let mut resource_index = HashMap::new();
        let mut prompt_index = HashMap::new();
        let mut all_tools = Vec::new();
        let mut all_resources = Vec::new();
        let mut all_prompts = Vec::new();

        for server_config in &config.servers {
            if !server_config.enabled {
                info!("⏭️  Skipping disabled server: {}", server_config.name);
                continue;
            }

            // Discover capabilities — transport-specific
            let (tools, resources, prompts, connected) = if server_config.url.is_some() {
                // ── HTTP transport ──────────────────────────────
                discover_http_server(server_config).await
            } else if server_config.command.is_some() {
                // ── Stdio transport (persistent connection) ────
                discover_stdio_server(server_config, &mut stdio_connections, death_tx.clone()).await
            } else {
                warn!(
                    "Server '{}' has neither 'url' nor 'command' configured",
                    server_config.name
                );
                (Vec::new(), Vec::new(), Vec::new(), false)
            };

            if connected {
                info!(
                    "📡 Server '{}': {} tools, {} resources, {} prompts",
                    server_config.name,
                    tools.len(),
                    resources.len(),
                    prompts.len()
                );
            } else {
                warn!(
                    "⚠️  Server '{}': failed to connect — marked as disconnected",
                    server_config.name
                );
            }

            // Index tools — detect overlapping names across servers (fixes #11).
            // When two servers register the same bare tool name, we:
            // 1. Log a clear WARNING so the operator knows about the collision.
            // 2. Remove the ambiguous bare-name entry so neither server
            //    "wins" silently (which previously caused wrong-server
            //    dispatch → SIGABRT).
            // 3. Keep both tools reachable via their FQN (server/tool).
            for tool in &tools {
                let registered = RegisteredTool::new(tool.clone(), &server_config.name);

                // FQN is always unique (server_name/tool_name) — safe to insert.
                tool_index.insert(registered.fqn.clone(), server_config.name.clone());

                // Bare name: check for collision with an existing server.
                if let Some(existing_server) = tool_index.get(&tool.name) {
                    // If the existing entry points to a *different* server,
                    // we have an overlap. Mark the bare name as ambiguous.
                    if existing_server != &server_config.name {
                        warn!(
                            "⚠️  Overlapping tool name '{}' registered by servers '{}' and '{}' — \
                             bare name removed from index. Use FQN (server/tool) to disambiguate.",
                            tool.name, existing_server, server_config.name
                        );
                        // Remove bare name so dispatch doesn't silently pick
                        // the wrong server (the root cause of the #11 crash).
                        tool_index.remove(&tool.name);
                    }
                    // If same server re-registers the same name (shouldn't
                    // happen, but harmless), the insert below is a no-op.
                } else {
                    // First registration of this bare name — index it.
                    tool_index.insert(tool.name.clone(), server_config.name.clone());
                }

                all_tools.push(registered);
            }

            // Index resources
            for resource in &resources {
                let registered = RegisteredResource::new(resource.clone(), &server_config.name);
                resource_index.insert(resource.uri.clone(), server_config.name.clone());
                resource_index.insert(registered.fqn.clone(), server_config.name.clone());
                all_resources.push(registered);
            }

            // Index prompts
            for prompt in &prompts {
                let registered = RegisteredPrompt::new(prompt.clone(), &server_config.name);
                prompt_index.insert(prompt.name.clone(), server_config.name.clone());
                prompt_index.insert(registered.fqn.clone(), server_config.name.clone());
                all_prompts.push(registered);
            }

            servers.insert(
                server_config.name.clone(),
                UpstreamServer {
                    name: server_config.name.clone(),
                    config: server_config.clone(),
                    tools,
                    resources,
                    prompts,
                    connected,
                },
            );
        }

        // ── Post-indexing: report any remaining ambiguous tool names ──
        // Collect bare names that were removed due to collisions.
        let all_bare_names: HashSet<String> = servers
            .values()
            .flat_map(|s| s.tools.iter().map(|t| t.name.clone()))
            .collect();
        let ambiguous: Vec<&String> = all_bare_names
            .iter()
            .filter(|name| !tool_index.contains_key(*name))
            .collect();
        if !ambiguous.is_empty() {
            error!(
                "🚨 {} tool name(s) are ambiguous across servers and cannot be \
                 called by bare name: {:?}. Use the fully-qualified name \
                 (server_name/tool_name) instead.",
                ambiguous.len(),
                ambiguous
            );
        }

        let connected_count = servers.values().filter(|s| s.connected).count();
        let total_count = servers.len();
        if connected_count < total_count {
            warn!(
                "🔌 Connected to {}/{} MCP server(s) ({} failed)",
                connected_count,
                total_count,
                total_count - connected_count
            );
        }

        Ok((
            Self {
                servers,
                stdio_connections,
                tool_index,
                resource_index,
                prompt_index,
                all_tools,
                all_resources,
                all_prompts,
                death_tx,
            },
            death_rx,
        ))
    }

    /// Get all registered tools across all servers
    pub fn get_all_tools(&self) -> Vec<RegisteredTool> {
        self.all_tools.clone()
    }

    /// Get all registered resources across all servers
    pub fn get_all_resources(&self) -> Vec<RegisteredResource> {
        self.all_resources.clone()
    }

    /// Get all registered prompts across all servers
    pub fn get_all_prompts(&self) -> Vec<RegisteredPrompt> {
        self.all_prompts.clone()
    }

    /// Find which server owns a given tool
    pub fn find_tool_server(&self, tool_name: &str) -> Option<String> {
        self.tool_index.get(tool_name).cloned()
    }

    /// Find which server owns a given resource
    pub fn find_resource_server(&self, resource_uri: &str) -> Option<String> {
        self.resource_index.get(resource_uri).cloned()
    }

    /// Find which server owns a given prompt
    pub fn find_prompt_server(&self, prompt_name: &str) -> Option<String> {
        self.prompt_index.get(prompt_name).cloned()
    }

    // ─────────────────────────────────────────────
    // Tool Calls
    // ─────────────────────────────────────────────

    /// Execute a tool call, routing to the correct upstream server
    pub async fn call_tool(
        &self,
        server_name: &str,
        params: &ToolCallParams,
    ) -> anyhow::Result<serde_json::Value> {
        let server = self
            .servers
            .get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server '{}' not found", server_name))?;

        if !server.connected {
            return Err(anyhow::anyhow!("Server '{}' is not connected", server_name));
        }

        if let Some(ref url) = server.config.url {
            call_tool_http(url, params).await
        } else if let Some(conn) = self.stdio_connections.get(server_name) {
            conn.send_request(
                "tools/call",
                serde_json::json!({
                    "name": params.name,
                    "arguments": params.arguments,
                }),
            )
            .await
        } else {
            Err(anyhow::anyhow!(
                "No active connection for server '{}'",
                server_name
            ))
        }
    }

    // ─────────────────────────────────────────────
    // Resource Reads
    // ─────────────────────────────────────────────

    /// Read a resource, routing to the correct upstream server
    pub async fn read_resource(&self, uri: &str) -> anyhow::Result<serde_json::Value> {
        let server_name = self
            .find_resource_server(uri)
            .ok_or_else(|| anyhow::anyhow!("Resource '{}' not found in any server", uri))?;

        let server = self
            .servers
            .get(&server_name)
            .ok_or_else(|| anyhow::anyhow!("Server '{}' not found", server_name))?;

        if !server.connected {
            return Err(anyhow::anyhow!("Server '{}' is not connected", server_name));
        }

        debug!("📖 Reading resource '{}' from '{}'", uri, server_name);

        if let Some(ref url) = server.config.url {
            rpc_http(url, "resources/read", serde_json::json!({ "uri": uri })).await
        } else if let Some(conn) = self.stdio_connections.get(&server_name) {
            conn.send_request("resources/read", serde_json::json!({ "uri": uri }))
                .await
        } else {
            Err(anyhow::anyhow!(
                "No active connection for server '{}'",
                server_name
            ))
        }
    }

    // ─────────────────────────────────────────────
    // Prompt Gets
    // ─────────────────────────────────────────────

    /// Get a prompt, routing to the correct upstream server
    pub async fn get_prompt(
        &self,
        name: &str,
        arguments: &Option<serde_json::Value>,
    ) -> anyhow::Result<serde_json::Value> {
        let server_name = self
            .find_prompt_server(name)
            .ok_or_else(|| anyhow::anyhow!("Prompt '{}' not found in any server", name))?;

        let server = self
            .servers
            .get(&server_name)
            .ok_or_else(|| anyhow::anyhow!("Server '{}' not found", server_name))?;

        if !server.connected {
            return Err(anyhow::anyhow!("Server '{}' is not connected", server_name));
        }

        debug!("💬 Getting prompt '{}' from '{}'", name, server_name);

        if let Some(ref url) = server.config.url {
            rpc_http(
                url,
                "prompts/get",
                serde_json::json!({
                    "name": name,
                    "arguments": arguments,
                }),
            )
            .await
        } else if let Some(conn) = self.stdio_connections.get(&server_name) {
            conn.send_request(
                "prompts/get",
                serde_json::json!({
                    "name": name,
                    "arguments": arguments,
                }),
            )
            .await
        } else {
            Err(anyhow::anyhow!(
                "No active connection for server '{}'",
                server_name
            ))
        }
    }

    /// Get server status information
    pub fn get_server_statuses(&self) -> Vec<serde_json::Value> {
        self.servers
            .values()
            .map(|s| {
                serde_json::json!({
                    "name": s.name,
                    "connected": s.connected,
                    "tools": s.tools.len(),
                    "resources": s.resources.len(),
                    "prompts": s.prompts.len(),
                    "transport": if s.config.url.is_some() { "http" } else { "stdio" },
                })
            })
            .collect()
    }

    /// Mark a server as disconnected and remove all its tools/resources/prompts
    /// from the routing indexes. Called by the dead-server monitor when a stdio
    /// child exits.
    ///
    /// Returns the number of tools that were removed (for event logging).
    pub fn mark_server_disconnected(&mut self, server_name: &str) -> usize {
        let server = match self.servers.get_mut(server_name) {
            Some(s) => s,
            None => return 0,
        };

        if !server.connected {
            return 0; // Already disconnected
        }

        server.connected = false;
        let tool_count = server.tools.len();
        let resource_count = server.resources.len();
        let prompt_count = server.prompts.len();

        // Remove tool index entries
        for tool in &server.tools {
            self.tool_index.remove(&tool.name);
            let fqn = format!("{}/{}", server_name, tool.name);
            self.tool_index.remove(&fqn);
        }

        // Remove resource index entries
        for resource in &server.resources {
            self.resource_index.remove(&resource.uri);
            let fqn = format!("{}/{}", server_name, resource.uri);
            self.resource_index.remove(&fqn);
        }

        // Remove prompt index entries
        for prompt in &server.prompts {
            self.prompt_index.remove(&prompt.name);
            let fqn = format!("{}/{}", server_name, prompt.name);
            self.prompt_index.remove(&fqn);
        }

        // Remove from all_* vectors
        self.all_tools.retain(|t| t.server_name != server_name);
        self.all_resources.retain(|r| r.server_name != server_name);
        self.all_prompts.retain(|p| p.server_name != server_name);

        // Clear the server's own lists
        server.tools.clear();
        server.resources.clear();
        server.prompts.clear();

        // Remove the dead stdio connection
        self.stdio_connections.remove(server_name);

        warn!(
            "⚠️  Server '{}' marked disconnected — {} tools, {} resources, {} prompts removed from routing",
            server_name, tool_count, resource_count, prompt_count
        );

        tool_count
    }

    /// Reconnect a previously-dead stdio server.
    ///
    /// Re-discovers tools/resources/prompts from the new connection,
    /// re-inserts them into all indexes, and marks the server as connected.
    pub async fn reconnect_server(
        &mut self,
        server_name: &str,
        conn: StdioConnection,
        capabilities: serde_json::Value,
    ) -> usize {
        let has_tools = capabilities.get("tools").is_some();
        let has_resources = capabilities.get("resources").is_some();
        let has_prompts = capabilities.get("prompts").is_some();

        // Discover tools
        let tools: Vec<ToolDefinition> = if has_tools {
            conn.send_request("tools/list", serde_json::json!({}))
                .await
                .ok()
                .and_then(|r| r.get("tools").cloned())
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        // Discover resources
        let resources: Vec<ResourceDefinition> = if has_resources {
            conn.send_request("resources/list", serde_json::json!({}))
                .await
                .ok()
                .and_then(|r| r.get("resources").cloned())
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        // Discover prompts
        let prompts: Vec<PromptDefinition> = if has_prompts {
            conn.send_request("prompts/list", serde_json::json!({}))
                .await
                .ok()
                .and_then(|r| r.get("prompts").cloned())
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        let tool_count = tools.len();

        // Re-index tools (respecting overlap detection from #11)
        for tool in &tools {
            let registered = RegisteredTool::new(tool.clone(), server_name);
            // FQN is always safe to insert
            self.tool_index
                .insert(registered.fqn.clone(), server_name.to_string());
            // Bare name: only insert if no other server already owns it
            if let Some(existing) = self.tool_index.get(&tool.name) {
                if existing != server_name {
                    warn!(
                        "⚠️  Overlapping tool '{}' on reconnect — bare name remains ambiguous",
                        tool.name
                    );
                    self.tool_index.remove(&tool.name);
                }
            } else {
                self.tool_index
                    .insert(tool.name.clone(), server_name.to_string());
            }
            self.all_tools.push(registered);
        }

        // Re-index resources
        for resource in &resources {
            let registered = RegisteredResource::new(resource.clone(), server_name);
            self.resource_index
                .insert(resource.uri.clone(), server_name.to_string());
            self.resource_index
                .insert(registered.fqn.clone(), server_name.to_string());
            self.all_resources.push(registered);
        }

        // Re-index prompts
        for prompt in &prompts {
            let registered = RegisteredPrompt::new(prompt.clone(), server_name);
            self.prompt_index
                .insert(prompt.name.clone(), server_name.to_string());
            self.prompt_index
                .insert(registered.fqn.clone(), server_name.to_string());
            self.all_prompts.push(registered);
        }

        // Update server state
        if let Some(server) = self.servers.get_mut(server_name) {
            server.tools = tools;
            server.resources = resources;
            server.prompts = prompts;
            server.connected = true;
        }

        // Store the new connection
        self.stdio_connections.insert(server_name.to_string(), conn);

        info!(
            "✅ Server '{}' reconnected — {} tools, {} resources, {} prompts restored",
            server_name,
            tool_count,
            self.servers
                .get(server_name)
                .map(|s| s.resources.len())
                .unwrap_or(0),
            self.servers
                .get(server_name)
                .map(|s| s.prompts.len())
                .unwrap_or(0),
        );

        tool_count
    }

    /// Get a server's config (used by the respawn logic)
    pub fn get_server_config(&self, server_name: &str) -> Option<&ServerConfig> {
        self.servers.get(server_name).map(|s| &s.config)
    }

    /// Get the death notification sender (for respawn reconnections)
    pub fn death_tx(&self) -> DeathSender {
        self.death_tx.clone()
    }
}

// ═════════════════════════════════════════════════════════════
// HTTP helpers (stateless — reqwest handles connection pooling)
// ═════════════════════════════════════════════════════════════

/// Send a generic JSON-RPC request over HTTP and return the `result`
async fn rpc_http(
    url: &str,
    method: &str,
    params: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let client = reqwest::Client::new();
    let request_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });

    let response = client.post(url).json(&request_body).send().await?;
    let body: serde_json::Value = response.json().await?;

    if let Some(result) = body.get("result") {
        Ok(result.clone())
    } else if let Some(error) = body.get("error") {
        Err(anyhow::anyhow!("Upstream error ({}): {}", method, error))
    } else {
        Err(anyhow::anyhow!(
            "Invalid response from upstream ({})",
            method
        ))
    }
}

/// Call a tool via HTTP transport
async fn call_tool_http(url: &str, params: &ToolCallParams) -> anyhow::Result<serde_json::Value> {
    rpc_http(
        url,
        "tools/call",
        serde_json::json!({
            "name": params.name,
            "arguments": params.arguments,
        }),
    )
    .await
}

// ═════════════════════════════════════════════════════════════
// Server Discovery
// ═════════════════════════════════════════════════════════════

/// Discover capabilities from an HTTP MCP server.
/// Returns (tools, resources, prompts, connected).
async fn discover_http_server(
    config: &ServerConfig,
) -> (
    Vec<ToolDefinition>,
    Vec<ResourceDefinition>,
    Vec<PromptDefinition>,
    bool,
) {
    let url = match config.url.as_ref() {
        Some(u) => u,
        None => return (Vec::new(), Vec::new(), Vec::new(), false),
    };

    match discover_http_server_inner(url, &config.name).await {
        Ok((tools, resources, prompts)) => (tools, resources, prompts, true),
        Err(e) => {
            warn!(
                "Failed to discover HTTP server '{}' ({}): {}",
                config.name, url, e
            );
            (Vec::new(), Vec::new(), Vec::new(), false)
        }
    }
}

async fn discover_http_server_inner(
    url: &str,
    server_name: &str,
) -> anyhow::Result<(
    Vec<ToolDefinition>,
    Vec<ResourceDefinition>,
    Vec<PromptDefinition>,
)> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    // Initialize
    let init_response: serde_json::Value = client
        .post(url)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": {
                    "name": "mcplex",
                    "version": env!("CARGO_PKG_VERSION"),
                }
            }
        }))
        .send()
        .await?
        .json()
        .await?;

    let capabilities = init_response
        .get("result")
        .and_then(|r| r.get("capabilities"))
        .cloned()
        .unwrap_or_default();

    // Send initialized notification
    let _ = client
        .post(url)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
        }))
        .send()
        .await;

    let has_tools = capabilities.get("tools").is_some();
    let has_resources = capabilities.get("resources").is_some();
    let has_prompts = capabilities.get("prompts").is_some();

    let tools = if has_tools {
        paginated_list_http(&client, url, "tools/list", "tools", server_name)
            .await
            .and_then(|v| serde_json::from_value::<Vec<ToolDefinition>>(v).ok())
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let resources = if has_resources {
        paginated_list_http(&client, url, "resources/list", "resources", server_name)
            .await
            .and_then(|v| serde_json::from_value::<Vec<ResourceDefinition>>(v).ok())
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let prompts = if has_prompts {
        paginated_list_http(&client, url, "prompts/list", "prompts", server_name)
            .await
            .and_then(|v| serde_json::from_value::<Vec<PromptDefinition>>(v).ok())
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    Ok((tools, resources, prompts))
}

/// Paginated HTTP list request — follows nextCursor until exhausted
async fn paginated_list_http(
    client: &reqwest::Client,
    url: &str,
    method: &str,
    result_key: &str,
    server_name: &str,
) -> Option<serde_json::Value> {
    let mut all_items: Vec<serde_json::Value> = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        let mut params = serde_json::json!({});
        if let Some(ref c) = cursor {
            params["cursor"] = serde_json::Value::String(c.clone());
        }

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": method,
            "params": params,
        });

        let response = match client.post(url).json(&request).send().await {
            Ok(r) => r,
            Err(e) => {
                warn!("Failed {} on '{}': {}", method, server_name, e);
                break;
            }
        };

        let body: serde_json::Value = match response.json().await {
            Ok(b) => b,
            Err(e) => {
                warn!("Failed to parse {} from '{}': {}", method, server_name, e);
                break;
            }
        };

        if let Some(result) = body.get("result") {
            if let Some(items) = result.get(result_key).and_then(|v| v.as_array()) {
                all_items.extend(items.clone());
            }
            cursor = result
                .get("nextCursor")
                .and_then(|c| c.as_str())
                .map(|s| s.to_string());
            if cursor.is_none() {
                break;
            }
        } else {
            break;
        }
    }

    if all_items.is_empty() {
        None
    } else {
        Some(serde_json::Value::Array(all_items))
    }
}

/// Discover capabilities from a stdio MCP server by establishing a persistent connection.
/// The connection is kept alive in `stdio_connections` for runtime use.
/// Returns (tools, resources, prompts, connected).
async fn discover_stdio_server(
    config: &ServerConfig,
    stdio_connections: &mut HashMap<String, StdioConnection>,
    death_tx: DeathSender,
) -> (
    Vec<ToolDefinition>,
    Vec<ResourceDefinition>,
    Vec<PromptDefinition>,
    bool,
) {
    match StdioConnection::connect(config, death_tx).await {
        Ok((conn, capabilities)) => {
            let has_tools = capabilities.get("tools").is_some();
            let has_resources = capabilities.get("resources").is_some();
            let has_prompts = capabilities.get("prompts").is_some();

            // Discover tools
            let tools = if has_tools {
                conn.send_request("tools/list", serde_json::json!({}))
                    .await
                    .ok()
                    .and_then(|r| r.get("tools").cloned())
                    .and_then(|v| serde_json::from_value::<Vec<ToolDefinition>>(v).ok())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };

            // Discover resources
            let resources = if has_resources {
                conn.send_request("resources/list", serde_json::json!({}))
                    .await
                    .ok()
                    .and_then(|r| r.get("resources").cloned())
                    .and_then(|v| serde_json::from_value::<Vec<ResourceDefinition>>(v).ok())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };

            // Discover prompts
            let prompts = if has_prompts {
                conn.send_request("prompts/list", serde_json::json!({}))
                    .await
                    .ok()
                    .and_then(|r| r.get("prompts").cloned())
                    .and_then(|v| serde_json::from_value::<Vec<PromptDefinition>>(v).ok())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };

            // Keep the connection alive for runtime use
            stdio_connections.insert(config.name.clone(), conn);

            (tools, resources, prompts, true)
        }
        Err(e) => {
            warn!("Failed to connect to stdio server '{}': {}", config.name, e);
            (Vec::new(), Vec::new(), Vec::new(), false)
        }
    }
}
