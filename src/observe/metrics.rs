// MCPlex — Metrics Collector
// In-memory ring buffer metrics with per-tool latency, token estimates, and cost tracking

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Types of events to record
#[derive(Debug, Clone)]
pub enum EventType {
    Request {
        method: String,
        duration_ms: u64,
        success: bool,
    },
    ToolCall {
        tool_name: String,
        server_name: String,
        duration_ms: u64,
        success: bool,
    },
    ToolsList {
        total: usize,
        visible: usize,
    },
    Routing {
        query: String,
        total_tools: usize,
        selected_tools: usize,
    },
    ServerDisconnect {
        server_name: String,
        tools_removed: usize,
    },
    ServerReconnect {
        server_name: String,
        tools_restored: usize,
    },
}

/// A single metric event with timestamp
#[derive(Debug, Clone, serde::Serialize)]
pub struct MetricEvent {
    pub timestamp: String,
    pub event_type: String,
    pub tool_name: Option<String>,
    pub server_name: Option<String>,
    pub duration_ms: Option<u64>,
    pub success: Option<bool>,
    pub tokens_saved: Option<usize>,
    pub details: Option<serde_json::Value>,
}

/// Per-tool aggregated statistics
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ToolStats {
    pub name: String,
    pub invocation_count: u64,
    pub success_count: u64,
    pub error_count: u64,
    pub total_duration_ms: u64,
    pub min_duration_ms: u64,
    pub max_duration_ms: u64,
    pub durations: Vec<u64>,
}

impl ToolStats {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            min_duration_ms: u64::MAX,
            ..Default::default()
        }
    }

    fn record(&mut self, duration_ms: u64, success: bool) {
        self.invocation_count += 1;
        self.total_duration_ms += duration_ms;
        self.min_duration_ms = self.min_duration_ms.min(duration_ms);
        self.max_duration_ms = self.max_duration_ms.max(duration_ms);

        if success {
            self.success_count += 1;
        } else {
            self.error_count += 1;
        }

        self.durations.push(duration_ms);
        if self.durations.len() > 100 {
            self.durations.remove(0);
        }
    }

    pub fn avg_duration_ms(&self) -> f64 {
        if self.invocation_count == 0 {
            0.0
        } else {
            self.total_duration_ms as f64 / self.invocation_count as f64
        }
    }

    pub fn p50(&self) -> u64 {
        percentile(&self.durations, 50)
    }

    pub fn p95(&self) -> u64 {
        percentile(&self.durations, 95)
    }

    pub fn p99(&self) -> u64 {
        percentile(&self.durations, 99)
    }
}

/// Global metrics collector
pub struct MetricsCollector {
    events: RwLock<Vec<MetricEvent>>,
    tool_stats: RwLock<HashMap<String, ToolStats>>,
    counters: RwLock<GlobalCounters>,
    max_events: usize,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct GlobalCounters {
    pub total_requests: u64,
    pub total_tool_calls: u64,
    pub total_errors: u64,
    pub total_tokens_saved: u64,
    pub total_routing_queries: u64,
    pub started_at_epoch: u64,
}

impl MetricsCollector {
    pub fn new() -> Self {
        let epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            events: RwLock::new(Vec::with_capacity(1000)),
            tool_stats: RwLock::new(HashMap::new()),
            counters: RwLock::new(GlobalCounters {
                started_at_epoch: epoch,
                ..Default::default()
            }),
            max_events: 1000,
        }
    }

    /// Record a metric event
    pub fn record_event(&self, event: EventType) {
        let now = now_iso8601();

        let metric_event = match &event {
            EventType::Request {
                method,
                duration_ms,
                success,
            } => {
                if let Ok(mut counters) = self.counters.write() {
                    counters.total_requests += 1;
                    if !success {
                        counters.total_errors += 1;
                    }
                }
                MetricEvent {
                    timestamp: now,
                    event_type: "request".to_string(),
                    tool_name: Some(method.clone()),
                    server_name: None,
                    duration_ms: Some(*duration_ms),
                    success: Some(*success),
                    tokens_saved: None,
                    details: None,
                }
            }
            EventType::ToolCall {
                tool_name,
                server_name,
                duration_ms,
                success,
            } => {
                if let Ok(mut counters) = self.counters.write() {
                    counters.total_tool_calls += 1;
                    if !success {
                        counters.total_errors += 1;
                    }
                }
                if let Ok(mut stats) = self.tool_stats.write() {
                    stats
                        .entry(tool_name.clone())
                        .or_insert_with(|| ToolStats::new(tool_name))
                        .record(*duration_ms, *success);
                }
                MetricEvent {
                    timestamp: now,
                    event_type: "tool_call".to_string(),
                    tool_name: Some(tool_name.clone()),
                    server_name: Some(server_name.clone()),
                    duration_ms: Some(*duration_ms),
                    success: Some(*success),
                    tokens_saved: None,
                    details: None,
                }
            }
            EventType::ToolsList { total, visible } => {
                let tokens_saved = (total - visible) * 200;
                if let Ok(mut counters) = self.counters.write() {
                    counters.total_tokens_saved += tokens_saved as u64;
                }
                MetricEvent {
                    timestamp: now,
                    event_type: "tools_list".to_string(),
                    tool_name: None,
                    server_name: None,
                    duration_ms: None,
                    success: None,
                    tokens_saved: Some(tokens_saved),
                    details: Some(serde_json::json!({
                        "total_tools": total,
                        "visible_tools": visible,
                    })),
                }
            }
            EventType::Routing {
                query,
                total_tools,
                selected_tools,
            } => {
                let tokens_saved = (total_tools - selected_tools) * 200;
                if let Ok(mut counters) = self.counters.write() {
                    counters.total_routing_queries += 1;
                    counters.total_tokens_saved += tokens_saved as u64;
                }
                MetricEvent {
                    timestamp: now,
                    event_type: "routing".to_string(),
                    tool_name: None,
                    server_name: None,
                    duration_ms: None,
                    success: None,
                    tokens_saved: Some(tokens_saved),
                    details: Some(serde_json::json!({
                        "query": query,
                        "total_tools": total_tools,
                        "selected_tools": selected_tools,
                    })),
                }
            }
            EventType::ServerDisconnect {
                server_name,
                tools_removed,
            } => {
                if let Ok(mut counters) = self.counters.write() {
                    counters.total_errors += 1;
                }
                MetricEvent {
                    timestamp: now,
                    event_type: "server_disconnect".to_string(),
                    tool_name: None,
                    server_name: Some(server_name.clone()),
                    duration_ms: None,
                    success: Some(false),
                    tokens_saved: None,
                    details: Some(serde_json::json!({
                        "tools_removed": tools_removed,
                    })),
                }
            }
            EventType::ServerReconnect {
                server_name,
                tools_restored,
            } => MetricEvent {
                timestamp: now,
                event_type: "server_reconnect".to_string(),
                tool_name: None,
                server_name: Some(server_name.clone()),
                duration_ms: None,
                success: Some(true),
                tokens_saved: None,
                details: Some(serde_json::json!({
                    "tools_restored": tools_restored,
                })),
            },
        };

        if let Ok(mut events) = self.events.write() {
            if events.len() >= self.max_events {
                events.remove(0);
            }
            events.push(metric_event);
        }
    }

    pub fn get_recent_events(&self, limit: usize) -> Vec<MetricEvent> {
        if let Ok(events) = self.events.read() {
            events.iter().rev().take(limit).cloned().collect()
        } else {
            Vec::new()
        }
    }

    pub fn get_tool_stats(&self) -> HashMap<String, ToolStats> {
        if let Ok(stats) = self.tool_stats.read() {
            stats.clone()
        } else {
            HashMap::new()
        }
    }

    pub fn get_counters(&self) -> GlobalCounters {
        if let Ok(counters) = self.counters.read() {
            counters.clone()
        } else {
            GlobalCounters::default()
        }
    }

    pub fn get_dashboard_data(&self) -> serde_json::Value {
        let counters = self.get_counters();
        let tool_stats = self.get_tool_stats();
        let recent_events = self.get_recent_events(50);

        let tools: Vec<serde_json::Value> = tool_stats
            .values()
            .map(|s| {
                serde_json::json!({
                    "name": s.name,
                    "invocations": s.invocation_count,
                    "successes": s.success_count,
                    "errors": s.error_count,
                    "avg_ms": format!("{:.1}", s.avg_duration_ms()),
                    "p50_ms": s.p50(),
                    "p95_ms": s.p95(),
                    "p99_ms": s.p99(),
                    "min_ms": if s.min_duration_ms == u64::MAX { 0 } else { s.min_duration_ms },
                    "max_ms": s.max_duration_ms,
                })
            })
            .collect();

        let now_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let uptime_secs = now_epoch.saturating_sub(counters.started_at_epoch);
        let uptime = format_duration_secs(uptime_secs);

        serde_json::json!({
            "counters": {
                "total_requests": counters.total_requests,
                "total_tool_calls": counters.total_tool_calls,
                "total_errors": counters.total_errors,
                "total_tokens_saved": counters.total_tokens_saved,
                "total_routing_queries": counters.total_routing_queries,
                "uptime": uptime,
            },
            "tools": tools,
            "recent_events": recent_events,
        })
    }
}

fn percentile(values: &[u64], pct: usize) -> u64 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort();
    let idx = (pct as f64 / 100.0 * sorted.len() as f64).ceil() as usize;
    sorted[idx.saturating_sub(1).min(sorted.len() - 1)]
}

fn format_duration_secs(total_secs: u64) -> String {
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

/// Generate ISO 8601 timestamp without chrono dependency
fn now_iso8601() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    // Rough UTC breakdown (not accounting for leap seconds, but good enough for logging)
    let days = secs / 86400;
    let remaining_secs = secs % 86400;
    let hours = remaining_secs / 3600;
    let minutes = (remaining_secs % 3600) / 60;
    let seconds = remaining_secs % 60;

    // Calculate year/month/day from days since epoch (1970-01-01)
    let (year, month, day) = days_to_ymd(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Simplified date calculation
    let mut y = 1970;
    let mut remaining = days;

    loop {
        let days_in_year = if is_leap_year(y) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }

    let days_in_months = if is_leap_year(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut m = 0;
    for (i, &dim) in days_in_months.iter().enumerate() {
        if remaining < dim {
            m = i + 1;
            break;
        }
        remaining -= dim;
    }

    (y, m as u64, remaining + 1)
}

fn is_leap_year(y: u64) -> bool {
    (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400)
}
