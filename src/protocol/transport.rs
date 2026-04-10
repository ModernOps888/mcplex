// MCPlex — Transport Layer
// Handles stdio and Streamable HTTP transports for both client-facing and upstream connections

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing::{debug, error, info, warn};

use crate::observe::metrics::EventType;
use crate::protocol::*;
use crate::AppState;

/// Start the main MCP gateway HTTP server
pub async fn start_gateway_server(addr: &str, state: Arc<AppState>) -> anyhow::Result<()> {
    let config = state.config.read().await;
    let has_api_key = config.gateway.api_key.is_some();
    let api_key = config.gateway.api_key.clone();
    let rate_limit = config.gateway.rate_limit_rps;
    drop(config);

    if has_api_key {
        info!("🔑 API key authentication enabled");
    }
    if rate_limit > 0 {
        info!("🚦 Rate limiting: {} req/s", rate_limit);
    }

    // Build rate limiter state
    let rate_limiter = Arc::new(RateLimiter::new(rate_limit));

    let app = Router::new()
        .route("/", get(health_check))
        .route("/health", get(health_check))
        .route("/mcp", post(handle_mcp_request))
        .route("/sse", get(handle_sse))
        .layer(axum::middleware::from_fn_with_state(
            (api_key, rate_limiter),
            auth_and_rate_limit_middleware,
        ))
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
            return (
                StatusCode::OK,
                Json(JsonRpcResponse::success(request_id, serde_json::json!({}))),
            );
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

    debug!(
        "📤 MCP response: method={} elapsed={}ms",
        method,
        elapsed.as_millis()
    );

    (StatusCode::OK, Json(response))
}

/// Handle SSE connections (Streamable HTTP transport)
///
/// Implements the MCP Streamable HTTP transport SSE endpoint.
/// Clients connect here to receive the MCP endpoint URL, then send
/// JSON-RPC requests to that endpoint. The server pushes events
/// for notifications like tools/list_changed.
async fn handle_sse(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    use axum::response::sse::{Event, KeepAlive, Sse};

    // Create a stream that:
    // 1. Sends the endpoint event immediately
    // 2. Sends periodic keepalive pings
    // 3. Could send notifications (tools/list_changed, etc.)

    let initial_event = Event::default().event("endpoint").data("/mcp");

    let stream = async_stream::stream! {
        // Send the endpoint URL first
        yield Ok::<_, std::convert::Infallible>(initial_event);

        // Then send periodic server status events
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));

        loop {
            interval.tick().await;

            let multiplexer = state.multiplexer.read().await;
            let statuses = multiplexer.get_server_statuses();
            let tool_count = multiplexer.get_all_tools().len();
            drop(multiplexer);

            let status_data = serde_json::json!({
                "type": "server_status",
                "servers": statuses,
                "total_tools": tool_count,
            });

            let event = Event::default()
                .event("status")
                .data(serde_json::to_string(&status_data).unwrap_or_default());

            yield Ok(event);
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Handle initialize request
async fn handle_initialize(state: &AppState, request: &JsonRpcRequest) -> JsonRpcResponse {
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
async fn handle_tools_list(state: &AppState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let multiplexer = state.multiplexer.read().await;
    let router = state.router.read().await;
    let config = state.config.read().await;

    // Get all tools from all connected servers
    let all_tools = multiplexer.get_all_tools();

    // Check if there's a cursor/query hint for filtering
    let query_hint = request
        .params
        .as_ref()
        .and_then(|p| p.get("_mcplex_query"))
        .and_then(|q| q.as_str())
        .map(|s| s.to_string());

    // Apply routing if a query hint is provided
    let filtered_tools = if let Some(ref query) = query_hint {
        let routed = router.route(query, &all_tools, config.router.top_k);
        info!(
            "🧠 Routed {} → {} tools (from {} total)",
            query,
            routed.len(),
            all_tools.len()
        );

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
    let visible_tools: Vec<&RegisteredTool> = filtered_tools
        .iter()
        .filter(|tool| security.is_tool_allowed(&tool.fqn, None))
        .collect();

    // Build the response
    let tool_defs: Vec<ToolDefinition> =
        visible_tools.iter().map(|t| t.definition.clone()).collect();

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
async fn handle_tools_call(state: &AppState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let start = std::time::Instant::now();

    // Parse tool call params
    let params: ToolCallParams = match request
        .params
        .as_ref()
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

    // Resolve role from request headers (via _mcplex_role param or default)
    let role = request
        .params
        .as_ref()
        .and_then(|p| p.get("_mcplex_role"))
        .and_then(|r| r.as_str())
        .map(|s| s.to_string());

    // Security check (with role if available)
    let security = state.security.read().await;
    if !security.is_tool_allowed(&tool_name, role.as_deref()) {
        warn!(
            "🚫 Tool call blocked by security policy: {} (role: {:?})",
            tool_name, role
        );
        security.audit_blocked_call(&tool_name, "security_policy");
        return JsonRpcResponse::error(
            request.id.clone(),
            -32001,
            &format!("Tool '{}' is not allowed by security policy", tool_name),
        );
    }

    // Check cache first (if enabled)
    let config = state.config.read().await;
    let cache_enabled = config.cache.enabled;
    drop(config);

    if cache_enabled {
        if let Some(cached_result) = state.cache.get(&tool_name, &params.arguments) {
            let elapsed = start.elapsed();
            state.metrics.record_event(EventType::ToolCall {
                tool_name: tool_name.clone(),
                server_name: "cache".to_string(),
                duration_ms: elapsed.as_millis() as u64,
                success: true,
            });
            return JsonRpcResponse::success(request.id.clone(), cached_result);
        }
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
                Ok(result_value) => {
                    // Store in cache if enabled
                    if cache_enabled {
                        state
                            .cache
                            .put(&tool_name, &params.arguments, result_value.clone());
                    }
                    JsonRpcResponse::success(request.id.clone(), result_value)
                }
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

/// Handle resources/list — aggregate resources from all upstream servers
async fn handle_resources_list(state: &AppState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let multiplexer = state.multiplexer.read().await;
    let all_resources = multiplexer.get_all_resources();

    let resource_defs: Vec<ResourceDefinition> =
        all_resources.iter().map(|r| r.definition.clone()).collect();

    let result = ResourcesListResult {
        resources: resource_defs,
        next_cursor: None,
    };

    JsonRpcResponse::success(
        request.id.clone(),
        serde_json::to_value(result).unwrap_or_default(),
    )
}

/// Handle resources/read — forward to the appropriate upstream server
async fn handle_resources_read(state: &AppState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let uri = match request
        .params
        .as_ref()
        .and_then(|p| p.get("uri"))
        .and_then(|u| u.as_str())
    {
        Some(u) => u.to_string(),
        None => {
            return JsonRpcResponse::error(
                request.id.clone(),
                error_codes::INVALID_PARAMS,
                "Missing 'uri' parameter in resources/read request",
            );
        }
    };

    let multiplexer = state.multiplexer.read().await;

    match multiplexer.read_resource(&uri).await {
        Ok(result) => JsonRpcResponse::success(request.id.clone(), result),
        Err(e) => {
            warn!("Resource read failed for '{}': {}", uri, e);
            JsonRpcResponse::error(
                request.id.clone(),
                error_codes::INVALID_PARAMS,
                &format!("Resource '{}' not found or read failed: {}", uri, e),
            )
        }
    }
}

/// Handle prompts/list — aggregate prompts from all upstream servers
async fn handle_prompts_list(state: &AppState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let multiplexer = state.multiplexer.read().await;
    let all_prompts = multiplexer.get_all_prompts();

    let prompt_defs: Vec<PromptDefinition> =
        all_prompts.iter().map(|p| p.definition.clone()).collect();

    let result = PromptsListResult {
        prompts: prompt_defs,
        next_cursor: None,
    };

    JsonRpcResponse::success(
        request.id.clone(),
        serde_json::to_value(result).unwrap_or_default(),
    )
}

/// Handle prompts/get — forward to the appropriate upstream server
async fn handle_prompts_get(state: &AppState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let (name, arguments) = match request.params.as_ref() {
        Some(params) => {
            let name = params
                .get("name")
                .and_then(|n| n.as_str())
                .map(|s| s.to_string());
            let arguments = params.get("arguments").cloned();
            (name, arguments)
        }
        None => (None, None),
    };

    let name = match name {
        Some(n) => n,
        None => {
            return JsonRpcResponse::error(
                request.id.clone(),
                error_codes::INVALID_PARAMS,
                "Missing 'name' parameter in prompts/get request",
            );
        }
    };

    let multiplexer = state.multiplexer.read().await;

    match multiplexer.get_prompt(&name, &arguments).await {
        Ok(result) => JsonRpcResponse::success(request.id.clone(), result),
        Err(e) => {
            warn!("Prompt get failed for '{}': {}", name, e);
            JsonRpcResponse::error(
                request.id.clone(),
                error_codes::INVALID_PARAMS,
                &format!("Prompt '{}' not found or get failed: {}", name, e),
            )
        }
    }
}

// ─────────────────────────────────────────────
// Authentication & Rate Limiting Middleware
// ─────────────────────────────────────────────

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::Instant;

/// Simple in-memory rate limiter using token bucket per client IP
pub struct RateLimiter {
    /// Max requests per second (0 = unlimited)
    rps: u32,
    /// Buckets per client IP
    buckets: RwLock<HashMap<String, TokenBucket>>,
}

struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
    max_tokens: f64,
    refill_rate: f64, // tokens per second
}

impl RateLimiter {
    pub fn new(rps: u32) -> Self {
        Self {
            rps,
            buckets: RwLock::new(HashMap::new()),
        }
    }

    /// Check if a request from this client should be allowed
    pub fn check(&self, client_id: &str) -> bool {
        if self.rps == 0 {
            return true; // Unlimited
        }

        let max_tokens = self.rps as f64 * 2.0; // Allow burst of 2x
        let refill_rate = self.rps as f64;

        if let Ok(mut buckets) = self.buckets.write() {
            let bucket = buckets
                .entry(client_id.to_string())
                .or_insert_with(|| TokenBucket {
                    tokens: max_tokens,
                    last_refill: Instant::now(),
                    max_tokens,
                    refill_rate,
                });

            // Refill tokens based on elapsed time
            let now = Instant::now();
            let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
            bucket.tokens = (bucket.tokens + elapsed * bucket.refill_rate).min(bucket.max_tokens);
            bucket.last_refill = now;

            // Try to consume a token
            if bucket.tokens >= 1.0 {
                bucket.tokens -= 1.0;
                true
            } else {
                false
            }
        } else {
            true // If lock fails, allow (don't fail closed on internal errors)
        }
    }
}

/// Combined auth + rate limit middleware
async fn auth_and_rate_limit_middleware(
    axum::extract::State((api_key, rate_limiter)): axum::extract::State<(
        Option<String>,
        Arc<RateLimiter>,
    )>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let path = request.uri().path().to_string();

    // Skip auth for health check
    if path == "/" || path == "/health" {
        return next.run(request).await;
    }

    // Check API key if configured
    if let Some(ref expected_key) = api_key {
        let provided_key = request
            .headers()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .or_else(|| {
                request
                    .headers()
                    .get("x-api-key")
                    .and_then(|v| v.to_str().ok())
            });

        match provided_key {
            Some(key) if key == expected_key => {} // OK
            _ => {
                warn!(
                    "🚫 Unauthorized request to {} — invalid or missing API key",
                    path
                );
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({
                        "error": "Unauthorized — provide API key via Authorization: Bearer <key> or X-API-Key header"
                    })),
                ).into_response();
            }
        }
    }

    // Rate limiting
    let client_id = request
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    if !rate_limiter.check(&client_id) {
        warn!("🚦 Rate limited request from {}", client_id);
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "error": "Rate limit exceeded — try again later"
            })),
        )
            .into_response();
    }

    next.run(request).await
}
