// MCPlex — Audit Logger
// Structured JSON audit logging for compliance and debugging

use tracing::{info, error};
use std::io::Write;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::protocol::ToolCallParams;

/// Structured audit logger
pub struct AuditLogger {
    writer: Mutex<Option<std::io::BufWriter<std::fs::File>>>,
}

/// Audit log entry
#[derive(serde::Serialize)]
struct AuditEntry {
    timestamp: String,
    event: String,
    tool_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    server_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    arguments: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trace_id: Option<String>,
}

impl AuditLogger {
    pub fn new(log_path: &str, enabled: bool) -> Self {
        let writer = if enabled {
            match Self::open_log_file(log_path) {
                Ok(w) => {
                    info!("📝 Audit log: {}", log_path);
                    Mutex::new(Some(w))
                }
                Err(e) => {
                    error!("Failed to open audit log '{}': {} — audit logging disabled", log_path, e);
                    Mutex::new(None)
                }
            }
        } else {
            Mutex::new(None)
        };

        Self { writer }
    }

    fn open_log_file(path: &str) -> anyhow::Result<std::io::BufWriter<std::fs::File>> {
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(std::io::BufWriter::new(file))
    }

    pub fn log_tool_call(
        &self,
        tool_name: &str,
        server_name: &str,
        params: &ToolCallParams,
        duration_ms: u64,
    ) {
        let entry = AuditEntry {
            timestamp: now_iso8601(),
            event: "tool_call".to_string(),
            tool_name: tool_name.to_string(),
            server_name: Some(server_name.to_string()),
            arguments: params.arguments.clone(),
            duration_ms: Some(duration_ms),
            reason: None,
            trace_id: Some(uuid::Uuid::new_v4().to_string()),
        };

        self.write_entry(&entry);
    }

    pub fn log_blocked_call(&self, tool_name: &str, reason: &str) {
        let entry = AuditEntry {
            timestamp: now_iso8601(),
            event: "tool_blocked".to_string(),
            tool_name: tool_name.to_string(),
            server_name: None,
            arguments: None,
            duration_ms: None,
            reason: Some(reason.to_string()),
            trace_id: Some(uuid::Uuid::new_v4().to_string()),
        };

        self.write_entry(&entry);
    }

    fn write_entry(&self, entry: &AuditEntry) {
        if let Ok(json) = serde_json::to_string(entry) {
            if let Ok(mut guard) = self.writer.lock() {
                if let Some(ref mut writer) = *guard {
                    let _ = writeln!(writer, "{}", json);
                    let _ = writer.flush();
                }
            }
        }
    }
}

/// Generate ISO 8601 timestamp without chrono dependency
fn now_iso8601() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let remaining_secs = secs % 86400;
    let hours = remaining_secs / 3600;
    let minutes = (remaining_secs % 3600) / 60;
    let seconds = remaining_secs % 60;
    let days = secs / 86400;
    let (year, month, day) = days_to_ymd(days);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", year, month, day, hours, minutes, seconds)
}

fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    let mut y = 1970u64;
    let mut remaining = days;
    loop {
        let diy = if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 { 366 } else { 365 };
        if remaining < diy { break; }
        remaining -= diy;
        y += 1;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let dim = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut m = 0;
    for (i, &d) in dim.iter().enumerate() {
        if remaining < d { m = i + 1; break; }
        remaining -= d;
    }
    (y, m as u64, remaining + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn test_audit_logger_writes_entries() {
        let temp_dir = std::env::temp_dir().join("mcplex_test_audit");
        let _ = std::fs::create_dir_all(&temp_dir);
        let log_path = temp_dir.join("test_audit.jsonl");
        let log_path_str = log_path.to_str().unwrap();
        let _ = std::fs::remove_file(&log_path);

        let logger = AuditLogger::new(log_path_str, true);
        
        let params = ToolCallParams {
            name: "test_tool".to_string(),
            arguments: Some(serde_json::json!({"key": "value"})),
        };

        logger.log_tool_call("test_tool", "test_server", &params, 42);
        logger.log_blocked_call("blocked_tool", "security_policy");

        let mut content = String::new();
        std::fs::File::open(&log_path)
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();

        let lines: Vec<&str> = content.trim().lines().collect();
        assert_eq!(lines.len(), 2);

        let entry1: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(entry1["event"], "tool_call");
        assert_eq!(entry1["tool_name"], "test_tool");

        let entry2: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(entry2["event"], "tool_blocked");

        let _ = std::fs::remove_file(&log_path);
        let _ = std::fs::remove_dir(&temp_dir);
    }
}
