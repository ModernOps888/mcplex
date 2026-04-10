// MCPlex — Transport Layer
// Handles stdio and Streamable HTTP transports for both client-facing and upstream connections

use std::sync::Arc;
use axum::{
    Router, Json,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use tower_http::cors::CorsLayer;
use tracing::{info, warn, error, debug};

use crate::AppState;
use crate::protocol::*;
use crate::observe::metrics::EventType;

/// Start the main MCP gateway HTTP server
pub async fn start_gateway_server(
    addr: &str,
    state: Arc<AppState>,
) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/", get(health_check))
        .route("/health", get(health_check))
        .route("/mcp", post(handle_mcp_request))
        .route("/sse", get(handle_sse))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("🌐 Gateway server started on {}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}

/// Health check endpoint
async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "service": "mcplex",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// Handle incoming MCP JSON-RPC requests over HTTP
async fn handle_mcp_request(
    State(state): State<Arc<AppState>>,
    Json(request): Json<JsonRpcRequest>,
) -> impl IntoResponse {
    let start = std::time::Instant::now();
    let method = request.method.clone();
    let request_id = request.id.clone();

    debug!("📥 MCP request: method={}", method);

    let response = match method.as_str() {
        "initialize" => handle_initialize(&state, &request).await,
        "initialized" => {
            // Notification — no response needed
            return (StatusCode::OK, Json(JsonRpcResponse::success(
                request_id,
                serde_json::json!({}),
            )));
        }
        "tools/list" => handle_tools_list(&state, &request).await,
        "tools/call" => handle_tools_call(&state, &request).await,
        "resources/list" => handle_resources_list(&state, &request).await,
        "resources/read" => handle_resources_read(&state, &request).await,
        "prompts/list" => handle_prompts_list(&state, &request).await,
        "prompts/get" => handle_prompts_get(&state, &request).await,
        "ping" => JsonRpcResponse::success(request_id.clone(), serde_json::json!({})),
        _ => {
            warn!("Unknown method: {}", method);
            JsonRpcResponse::error(
                request_id.clone(),
                error_codes::METHOD_NOT_FOUND,
                &format!("Method '{}' not found", method),
            )
        }
    };

    let elapsed = start.elapsed();
    state.metrics.record_event(EventType::Request {
        method: method.clone(),
        duration_ms: elapsed.as_millis() as u64,
        success: response.error.is_none(),
    });

    debug!("📤 MCP response: method={} elapsed={}ms", method, elapsed.as_millis());

    (StatusCode::OK, Json(response))
}

/// Handle SSE connections (for streaming transport)
async fn handle_sse(
    State(_state): State<Arc<AppState>>,
) -> impl IntoResponse {
    // SSE endpoint for clients that need event streaming
    (StatusCode::OK, "event: endpoint\ndata: /mcp\n\n")
}

/// Handle initialize request
async fn handle_initialize(
    state: &AppState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    let config = state.config.read().await;

    let result = InitializeResult {
        protocol_version: "2025-03-26".to_string(),
        capabilities: ServerCapabilities {
            tools: Some(ToolsCapability { list_changed: true }),
            resources: Some(serde_json::json!({})),
            prompts: Some(serde_json::json!({})),
        },
        server_info: ServerInfo {
            name: format!("mcplex-{}", config.gateway.name),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
    };

    JsonRpcResponse::success(
        request.id.clone(),
        serde_json::to_value(result).unwrap_or_default(),
    )
}

/// Handle tools/list — the core of MCPlex magic
/// Instead of dumping ALL tools, we route intelligently
async fn handle_tools_list(
    state: &AppState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    let multiplexer = state.multiplexer.read().await;
    let router = state.router.read().await;
    let config = state.config.read().await;

    // Get all tools from all connected servers
    let all_tools = multiplexer.get_all_tools();

    // Check if there's a cursor/query hint for filtering
    let query_hint = request.params
        .as_ref()
        .and_then(|p| p.get("_mcplex_query"))
        .and_then(|q| q.as_str())
        .map(|s| s.to_string());

    // Apply routing if a query hint is provided
    let filtered_tools = if let Some(ref query) = query_hint {
        let routed = router.route(query, &all_tools, config.router.top_k);
        info!("🧠 Routed {} → {} tools (from {} total)",
            query, routed.len(), all_tools.len());
        
        state.metrics.record_event(EventType::Routing {
            query: query.clone(),
            total_tools: all_tools.len(),
            selected_tools: routed.len(),
        });
        
        routed
    } else {
        all_tools.clone()
    };

    // Apply security filtering
    let security = state.security.read().await;
    let visible_tools: Vec<&RegisteredTool> = filtered_tools.iter()
        .filter(|tool| security.is_tool_allowed(&tool.fqn, None))
        .collect();

    // Build the response
    let tool_defs: Vec<ToolDefinition> = visible_tools.iter()
        .map(|t| t.definition.clone())
        .collect();

    let result = ToolsListResult {
        tools: tool_defs,
        next_cursor: None,
    };

    state.metrics.record_event(EventType::ToolsList {
        total: all_tools.len(),
        visible: visible_tools.len(),
    });

    JsonRpcResponse::success(
        request.id.clone(),
        serde_json::to_value(result).unwrap_or_default(),
    )
}

/// Handle tools/call — execute a tool on the appropriate upstream server
async fn handle_tools_call(
    state: &AppState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    let start = std::time::Instant::now();

    // Parse tool call params
    let params: ToolCallParams = match request.params.as_ref()
        .and_then(|p| serde_json::from_value(p.clone()).ok())
    {
        Some(p) => p,
        None => {
            return JsonRpcResponse::error(
                request.id.clone(),
                error_codes::INVALID_PARAMS,
                "Missing or invalid tool call parameters",
            );
        }
    };

    let tool_name = params.name.clone();

    // Security check
    let security = state.security.read().await;
    if !security.is_tool_allowed(&tool_name, None) {
        warn!("🚫 Tool call blocked by security policy: {}", tool_name);
        security.audit_blocked_call(&tool_name, "security_policy");
        return JsonRpcResponse::error(
            request.id.clone(),
            -32001,
            &format!("Tool '{}' is not allowed by security policy", tool_name),
        );
    }

    // Find which server owns this tool
    let multiplexer = state.multiplexer.read().await;
    let server_name = multiplexer.find_tool_server(&tool_name);

    match server_name {
        Some(server) => {
            debug!("🔧 Dispatching tool '{}' to server '{}'", tool_name, server);

            // Execute the tool call on the upstream server
            let result = multiplexer.call_tool(&server, &params).await;
            let elapsed = start.elapsed();

            // Record metrics
            state.metrics.record_event(EventType::ToolCall {
                tool_name: tool_name.clone(),
                server_name: server.clone(),
                duration_ms: elapsed.as_millis() as u64,
                success: result.is_ok(),
            });

            // Audit log
            security.audit_tool_call(&tool_name, &server, &params, elapsed.as_millis() as u64);

            match result {
                Ok(result_value) => JsonRpcResponse::success(request.id.clone(), result_value),
                Err(e) => {
                    error!("Tool call failed: {} — {}", tool_name, e);
                    JsonRpcResponse::error(
                        request.id.clone(),
                        error_codes::INTERNAL_ERROR,
                        &format!("Tool execution failed: {}", e),
                    )
                }
            }
        }
        None => {
            warn!("Tool not found: {}", tool_name);
            JsonRpcResponse::error(
                request.id.clone(),
                error_codes::INVALID_PARAMS,
                &format!("Tool '{}' not found in any connected server", tool_name),
            )
        }
    }
}

/// Handle resources/list
async fn handle_resources_list(
    state: &AppState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    let multiplexer = state.multiplexer.read().await;
    let resources = multiplexer.get_all_resources();
    
    JsonRpcResponse::success(
        request.id.clone(),
        serde_json::json!({ "resources": resources }),
    )
}

/// Handle resources/read
async fn handle_resources_read(
    state: &AppState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    // Forward to appropriate server
    let multiplexer = state.multiplexer.read().await;
    
    if let Some(params) = &request.params {
        if let Some(uri) = params.get("uri").and_then(|u| u.as_str()) {
            if let Some(result) = multiplexer.read_resource(uri).await {
                return JsonRpcResponse::success(request.id.clone(), result);
            }
        }
    }
    
    JsonRpcResponse::error(
        request.id.clone(),
        error_codes::INVALID_PARAMS,
        "Resource not found",
    )
}

/// Handle prompts/list
async fn handle_prompts_list(
    state: &AppState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    let multiplexer = state.multiplexer.read().await;
    let prompts = multiplexer.get_all_prompts();
    
    JsonRpcResponse::success(
        request.id.clone(),
        serde_json::json!({ "prompts": prompts }),
    )
}

/// Handle prompts/get
async fn handle_prompts_get(
    _state: &AppState,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    JsonRpcResponse::error(
        request.id.clone(),
        error_codes::METHOD_NOT_FOUND,
        "Prompt get is not yet supported in MCPlex",
    )
}
