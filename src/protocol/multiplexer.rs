// MCPlex — Multiplexer
// Aggregates multiple MCP servers into a unified interface

use std::collections::HashMap;
use tracing::{info, warn, debug};

use crate::config::{AppConfig, ServerConfig};
use crate::protocol::{RegisteredTool, ToolDefinition, ToolCallParams};

/// Represents a connected upstream MCP server
#[derive(Debug)]
pub struct UpstreamServer {
    pub name: String,
    pub config: ServerConfig,
    pub tools: Vec<ToolDefinition>,
    pub resources: Vec<serde_json::Value>,
    pub prompts: Vec<serde_json::Value>,
    pub connected: bool,
}

/// The Multiplexer manages connections to all upstream MCP servers
pub struct Multiplexer {
    servers: HashMap<String, UpstreamServer>,
    /// Lookup: tool_name → server_name
    tool_index: HashMap<String, String>,
    /// All registered tools with their server origin
    all_tools: Vec<RegisteredTool>,
}

impl Multiplexer {
    /// Create a new multiplexer from configuration
    pub async fn new(config: &AppConfig) -> anyhow::Result<Self> {
        let mut servers = HashMap::new();
        let mut tool_index = HashMap::new();
        let mut all_tools = Vec::new();

        for server_config in &config.servers {
            if !server_config.enabled {
                info!("⏭️  Skipping disabled server: {}", server_config.name);
                continue;
            }

            // Try to discover tools from the server
            let tools = discover_tools(server_config).await;

            info!("📡 Server '{}': {} tools discovered",
                server_config.name, tools.len());

            for tool in &tools {
                let registered = RegisteredTool::new(tool.clone(), &server_config.name);
                
                // Index by both short name and FQN
                tool_index.insert(tool.name.clone(), server_config.name.clone());
                tool_index.insert(registered.fqn.clone(), server_config.name.clone());
                
                all_tools.push(registered);
            }

            servers.insert(
                server_config.name.clone(),
                UpstreamServer {
                    name: server_config.name.clone(),
                    config: server_config.clone(),
                    tools,
                    resources: Vec::new(),
                    prompts: Vec::new(),
                    connected: true,
                },
            );
        }

        Ok(Self {
            servers,
            tool_index,
            all_tools,
        })
    }

    /// Get all registered tools across all servers
    pub fn get_all_tools(&self) -> Vec<RegisteredTool> {
        self.all_tools.clone()
    }

    /// Find which server owns a given tool
    pub fn find_tool_server(&self, tool_name: &str) -> Option<String> {
        self.tool_index.get(tool_name).cloned()
    }

    /// Execute a tool call on a specific server
    pub async fn call_tool(
        &self,
        server_name: &str,
        params: &ToolCallParams,
    ) -> anyhow::Result<serde_json::Value> {
        let server = self.servers.get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server '{}' not found", server_name))?;

        // Determine transport and execute
        if let Some(ref url) = server.config.url {
            self.call_tool_http(url, params).await
        } else if let Some(ref _command) = server.config.command {
            self.call_tool_stdio(server, params).await
        } else {
            Err(anyhow::anyhow!("Server '{}' has no transport configured", server_name))
        }
    }

    /// Call a tool via HTTP transport
    async fn call_tool_http(
        &self,
        url: &str,
        params: &ToolCallParams,
    ) -> anyhow::Result<serde_json::Value> {
        let client = reqwest::Client::new();
        let request_body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": params.name,
                "arguments": params.arguments,
            }
        });

        let response = client
            .post(url)
            .json(&request_body)
            .send()
            .await?;

        let response_body: serde_json::Value = response.json().await?;
        
        if let Some(result) = response_body.get("result") {
            Ok(result.clone())
        } else if let Some(error) = response_body.get("error") {
            Err(anyhow::anyhow!("Upstream error: {}", error))
        } else {
            Err(anyhow::anyhow!("Invalid response from upstream server"))
        }
    }

    /// Call a tool via stdio transport
    async fn call_tool_stdio(
        &self,
        server: &UpstreamServer,
        params: &ToolCallParams,
    ) -> anyhow::Result<serde_json::Value> {
        let command = server.config.command.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No command configured"))?;

        // Build the JSON-RPC request
        let request_body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": params.name,
                "arguments": params.arguments,
            }
        });

        // Parse command parts
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return Err(anyhow::anyhow!("Empty command"));
        }

        let mut cmd = tokio::process::Command::new(parts[0]);
        if parts.len() > 1 {
            cmd.args(&parts[1..]);
        }

        // Add args from config
        cmd.args(&server.config.args);

        // Set environment variables
        for (key, value) in &server.config.env {
            cmd.env(key, value);
        }

        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn()?;

        // Write request to stdin
        if let Some(ref mut stdin) = child.stdin {
            use tokio::io::AsyncWriteExt;
            let request_str = serde_json::to_string(&request_body)? + "\n";
            stdin.write_all(request_str.as_bytes()).await?;
            stdin.shutdown().await?;
        }

        // Read response from stdout
        let output = child.wait_with_output().await?;
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Parse the last JSON line (in case there's initialization output)
        for line in stdout.lines().rev() {
            let trimmed = line.trim();
            if trimmed.starts_with('{') {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(trimmed) {
                    if let Some(result) = parsed.get("result") {
                        return Ok(result.clone());
                    }
                    if let Some(error) = parsed.get("error") {
                        return Err(anyhow::anyhow!("Upstream error: {}", error));
                    }
                }
            }
        }

        Err(anyhow::anyhow!("No valid response from stdio server"))
    }

    /// Get all resources from all servers
    pub fn get_all_resources(&self) -> Vec<serde_json::Value> {
        self.servers.values()
            .flat_map(|s| s.resources.clone())
            .collect()
    }

    /// Read a specific resource
    pub async fn read_resource(&self, _uri: &str) -> Option<serde_json::Value> {
        // TODO: Route resource read to appropriate server
        None
    }

    /// Get all prompts from all servers
    pub fn get_all_prompts(&self) -> Vec<serde_json::Value> {
        self.servers.values()
            .flat_map(|s| s.prompts.clone())
            .collect()
    }

    /// Get server status information
    pub fn get_server_statuses(&self) -> Vec<serde_json::Value> {
        self.servers.values()
            .map(|s| serde_json::json!({
                "name": s.name,
                "connected": s.connected,
                "tools": s.tools.len(),
                "transport": if s.config.url.is_some() { "http" } else { "stdio" },
            }))
            .collect()
    }
}

/// Discover tools from an MCP server
async fn discover_tools(config: &ServerConfig) -> Vec<ToolDefinition> {
    // For HTTP servers, try to initialize and list tools
    if let Some(ref url) = config.url {
        match discover_tools_http(url).await {
            Ok(tools) => return tools,
            Err(e) => {
                warn!("Failed to discover tools from '{}' ({}): {}", config.name, url, e);
            }
        }
    }

    // For stdio servers, we'll discover tools lazily when the server starts
    // For now, return empty and log
    if config.command.is_some() {
        debug!("Stdio server '{}' — tools will be discovered on first connection", config.name);
    }

    Vec::new()
}

/// Discover tools from an HTTP MCP server
async fn discover_tools_http(url: &str) -> anyhow::Result<Vec<ToolDefinition>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    // Initialize
    let init_request = serde_json::json!({
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
    });

    client.post(url).json(&init_request).send().await?;

    // List tools
    let list_request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    });

    let response = client.post(url).json(&list_request).send().await?;
    let body: serde_json::Value = response.json().await?;

    if let Some(result) = body.get("result") {
        if let Some(tools) = result.get("tools") {
            let tools: Vec<ToolDefinition> = serde_json::from_value(tools.clone())?;
            return Ok(tools);
        }
    }

    Ok(Vec::new())
}
