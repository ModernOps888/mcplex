// MCPlex — AgentLens Bridge
// Optional, non-blocking forwarder that sends tool call events to AgentLens
// for visualization in the timeline replay UI.
//
// This module is entirely opt-in. If [agentlens] is missing from the config
// or enabled=false, the bridge is never instantiated and MCPlex works exactly
// as it always has — zero overhead, zero coupling.

use crate::config::AgentLensConfig;
use serde_json::json;
use std::sync::Arc;
use tracing::{debug, info};
use uuid::Uuid;

/// Non-blocking forwarder that sends events to AgentLens.
/// All HTTP calls are fire-and-forget on a background task —
/// they never block the gateway's critical path.
pub struct AgentLensBridge {
    config: AgentLensConfig,
    client: reqwest::Client,
    session_id: String,
}

impl AgentLensBridge {
    /// Create a new bridge. Returns None if the config is disabled.
    pub fn new(config: &AgentLensConfig) -> Option<Arc<Self>> {
        if !config.enabled {
            return None;
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .ok()?;

        let session_id = Uuid::new_v4().to_string();

        info!(
            "🔗 AgentLens bridge enabled → forwarding events to {}",
            config.url
        );
        info!("   Session ID: {}", session_id);
        info!("   Session name: {}", config.session_name);

        Some(Arc::new(Self {
            config: config.clone(),
            client,
            session_id,
        }))
    }

    /// Forward a tool call event to AgentLens (fire-and-forget).
    pub fn forward_tool_call(
        self: &Arc<Self>,
        tool_name: &str,
        server_name: &str,
        duration_ms: u64,
        success: bool,
    ) {
        if !self.config.forward_tool_calls {
            return;
        }

        let payload = json!({
            "session_id": self.session_id,
            "session_name": self.config.session_name,
            "agent_name": format!("MCPlex/{}", server_name),
            "step_type": "mcp_invoke",
            "status": if success { "success" } else { "error" },
            "mcp_server": server_name,
            "mcp_tool": tool_name,
            "mcp_params": {},
            "duration_ms": duration_ms,
        });

        let client = self.client.clone();
        let url = self.config.url.clone();
        let tool_name_owned = tool_name.to_string();

        // Fire-and-forget: spawn a background task, don't await
        tokio::spawn(async move {
            match client.post(&url).json(&payload).send().await {
                Ok(resp) if resp.status().is_success() => {
                    debug!("🔗 AgentLens: forwarded tool_call {}", tool_name_owned);
                }
                Ok(resp) => {
                    debug!("🔗 AgentLens: forward returned status {}", resp.status());
                }
                Err(e) => {
                    // AgentLens might be offline — that's fine, don't spam logs
                    debug!("🔗 AgentLens: forward failed (offline?): {}", e);
                }
            }
        });
    }

    /// Forward a security event (RBAC block, rate limit) to AgentLens.
    pub fn forward_security_event(
        self: &Arc<Self>,
        event_type: &str,
        tool_name: &str,
        reason: &str,
    ) {
        if !self.config.forward_security_events {
            return;
        }

        let payload = json!({
            "session_id": self.session_id,
            "session_name": self.config.session_name,
            "agent_name": "MCPlex/Security",
            "step_type": "error",
            "status": "error",
            "error_message": format!("{}: {} ({})", event_type, tool_name, reason),
            "error_type": event_type,
            "duration_ms": 0,
        });

        let client = self.client.clone();
        let url = self.config.url.clone();

        tokio::spawn(async move {
            let _ = client.post(&url).json(&payload).send().await;
        });
    }

    /// Forward a routing/discovery event to AgentLens.
    pub fn forward_routing_event(
        self: &Arc<Self>,
        query: &str,
        total_tools: usize,
        selected_tools: usize,
    ) {
        let payload = json!({
            "session_id": self.session_id,
            "session_name": self.config.session_name,
            "agent_name": "MCPlex/Router",
            "step_type": "decision",
            "decision_reason": format!(
                "Routing query '{}': {} → {} tools (saved {} tool definitions)",
                query, total_tools, selected_tools, total_tools - selected_tools
            ),
            "duration_ms": 0,
        });

        let client = self.client.clone();
        let url = self.config.url.clone();

        tokio::spawn(async move {
            let _ = client.post(&url).json(&payload).send().await;
        });
    }
}
