// MCPlex — Metrics Export
// Prometheus-compatible metrics endpoint and JSON export

use crate::AppState;

/// Generate Prometheus-compatible metrics output (text format)
pub fn prometheus_metrics(state: &AppState) -> String {
    let counters = state.metrics.get_counters();
    let tool_stats = state.metrics.get_tool_stats();

    let mut output = String::new();

    output.push_str("# HELP mcplex_requests_total Total number of MCP requests handled\n");
    output.push_str("# TYPE mcplex_requests_total counter\n");
    output.push_str(&format!(
        "mcplex_requests_total {}\n",
        counters.total_requests
    ));

    output.push_str("# HELP mcplex_tool_calls_total Total number of tool calls executed\n");
    output.push_str("# TYPE mcplex_tool_calls_total counter\n");
    output.push_str(&format!(
        "mcplex_tool_calls_total {}\n",
        counters.total_tool_calls
    ));

    output.push_str("# HELP mcplex_errors_total Total number of errors\n");
    output.push_str("# TYPE mcplex_errors_total counter\n");
    output.push_str(&format!("mcplex_errors_total {}\n", counters.total_errors));

    output.push_str("# HELP mcplex_tokens_saved_total Total tokens saved via routing\n");
    output.push_str("# TYPE mcplex_tokens_saved_total counter\n");
    output.push_str(&format!(
        "mcplex_tokens_saved_total {}\n",
        counters.total_tokens_saved
    ));

    output.push_str("# HELP mcplex_routing_queries_total Total routing queries\n");
    output.push_str("# TYPE mcplex_routing_queries_total counter\n");
    output.push_str(&format!(
        "mcplex_routing_queries_total {}\n",
        counters.total_routing_queries
    ));

    output.push_str("\n# HELP mcplex_tool_invocations_total Tool invocations by tool name\n");
    output.push_str("# TYPE mcplex_tool_invocations_total counter\n");
    for (name, stats) in &tool_stats {
        output.push_str(&format!(
            "mcplex_tool_invocations_total{{tool=\"{}\"}} {}\n",
            name, stats.invocation_count
        ));
    }

    output.push_str("\n# HELP mcplex_tool_errors_total Tool errors by tool name\n");
    output.push_str("# TYPE mcplex_tool_errors_total counter\n");
    for (name, stats) in &tool_stats {
        output.push_str(&format!(
            "mcplex_tool_errors_total{{tool=\"{}\"}} {}\n",
            name, stats.error_count
        ));
    }

    output.push_str("\n# HELP mcplex_tool_duration_ms Tool call duration in milliseconds\n");
    output.push_str("# TYPE mcplex_tool_duration_ms summary\n");
    for (name, stats) in &tool_stats {
        output.push_str(&format!(
            "mcplex_tool_duration_ms{{tool=\"{}\",quantile=\"0.5\"}} {}\n",
            name,
            stats.p50()
        ));
        output.push_str(&format!(
            "mcplex_tool_duration_ms{{tool=\"{}\",quantile=\"0.95\"}} {}\n",
            name,
            stats.p95()
        ));
        output.push_str(&format!(
            "mcplex_tool_duration_ms{{tool=\"{}\",quantile=\"0.99\"}} {}\n",
            name,
            stats.p99()
        ));
    }

    output
}
