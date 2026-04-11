// MCPlex — Persistent Stdio Connection
// Long-lived child process with stdin/stdout multiplexing via JSON-RPC ID correlation
//
// Instead of spawning a fresh child per RPC call (which breaks MCP servers),
// this module maintains a single long-lived child process per stdio server.
// A background reader task correlates responses by JSON-RPC request ID,
// enabling concurrent multiplexed requests over a single connection.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::ChildStdin;
use tokio::sync::{oneshot, Mutex as AsyncMutex};
use tracing::{debug, error, info, warn};

use crate::config::ServerConfig;
use crate::protocol::multiplexer::DeathSender;

/// Outcome channel type: Ok(result_value) or Err(error_message)
type PendingResult = Result<serde_json::Value, String>;

/// A persistent, multiplexed connection to a stdio MCP server.
///
/// - Holds a long-lived child process with piped stdin/stdout.
/// - A background task reads JSON-RPC responses from stdout and
///   routes them to callers by correlating request IDs.
/// - Multiple concurrent requests are supported via atomic ID generation.
pub struct StdioConnection {
    writer: AsyncMutex<BufWriter<ChildStdin>>,
    pending: Arc<Mutex<HashMap<i64, oneshot::Sender<PendingResult>>>>,
    next_id: AtomicI64,
    server_name: String,
    _reader_handle: tokio::task::JoinHandle<()>,
    _child_handle: tokio::task::JoinHandle<()>,
}

impl StdioConnection {
    /// Spawn a child process, perform the MCP handshake (`initialize` +
    /// `notifications/initialized`), and return the persistent connection
    /// along with the server's declared capabilities.
    ///
    /// The child process remains alive for the lifetime of this connection.
    /// On drop, the background tasks are detached and the child is reaped.
    ///
    /// `death_tx` is used to notify the dead-server monitor when this child exits.
    pub async fn connect(
        config: &ServerConfig,
        death_tx: DeathSender,
    ) -> anyhow::Result<(Self, serde_json::Value)> {
        let command = config
            .command
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No command configured for server '{}'", config.name))?;

        info!(
            "🔌 Spawning stdio server '{}': {} {:?}",
            config.name, command, config.args
        );

        // Build command — treat `command` as the executable path (no split_whitespace).
        // Additional arguments go through config.args.
        let mut cmd = tokio::process::Command::new(command);
        cmd.args(&config.args);

        for (key, value) in &config.env {
            cmd.env(key, value);
        }

        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            anyhow::anyhow!(
                "Failed to spawn '{}' (command: '{}' {:?}): {}",
                config.name,
                command,
                config.args,
                e
            )
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture stdin for '{}'", config.name))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture stdout for '{}'", config.name))?;

        let writer = AsyncMutex::new(BufWriter::new(stdin));
        let pending: Arc<Mutex<HashMap<i64, oneshot::Sender<PendingResult>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let server_name = config.name.clone();

        // Start the background reader task
        let pending_for_reader = Arc::clone(&pending);
        let name_for_reader = server_name.clone();
        let _reader_handle = tokio::spawn(async move {
            reader_loop(stdout, pending_for_reader, name_for_reader).await;
        });

        // Start a task that reaps the child when it exits,
        // drains all pending requests with errors, and notifies
        // the dead-server monitor via the death channel
        let name_for_child = server_name.clone();
        let pending_for_child = Arc::clone(&pending);
        let _child_handle = tokio::spawn(async move {
            let status = child.wait().await;
            match status {
                Ok(s) => warn!("⚠️  Stdio server '{}' exited: {}", name_for_child, s),
                Err(e) => error!("❌ Stdio server '{}' wait error: {}", name_for_child, e),
            }
            // Drain all pending requests with an error
            if let Ok(mut map) = pending_for_child.lock() {
                let count = map.len();
                for (id, sender) in map.drain() {
                    let _ = sender.send(Err(format!(
                        "Server '{}' exited while request id={} was pending",
                        name_for_child, id
                    )));
                }
                if count > 0 {
                    warn!(
                        "⚠️  Drained {} pending requests for '{}'",
                        count, name_for_child
                    );
                }
            }
            // Notify the dead-server monitor so it can clean up
            // multiplexer state and optionally attempt respawn
            let _ = death_tx.send(name_for_child);
        });

        let conn = Self {
            writer,
            pending,
            next_id: AtomicI64::new(10), // 1-9 reserved for handshake
            server_name,
            _reader_handle,
            _child_handle,
        };

        // ── MCP Handshake ──────────────────────────────────
        let init_result = conn
            .send_request(
                "initialize",
                serde_json::json!({
                    "protocolVersion": "2025-03-26",
                    "capabilities": {},
                    "clientInfo": {
                        "name": "mcplex",
                        "version": env!("CARGO_PKG_VERSION"),
                    }
                }),
            )
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "MCP initialize handshake failed for '{}': {}",
                    conn.server_name,
                    e
                )
            })?;

        conn.send_notification("notifications/initialized").await?;

        let capabilities = init_result.get("capabilities").cloned().unwrap_or_default();

        info!(
            "🤝 MCP handshake complete for '{}' — capabilities: {}",
            conn.server_name,
            serde_json::to_string(&capabilities).unwrap_or_default()
        );

        Ok((conn, capabilities))
    }

    /// Send a JSON-RPC request and wait for the correlated response.
    ///
    /// Returns the `result` field on success, or an error if the upstream
    /// returns a JSON-RPC error, the request times out, or the child dies.
    pub async fn send_request(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();

        // Register pending response
        {
            let mut map = self.pending.lock().map_err(|_| {
                anyhow::anyhow!("Pending map lock poisoned for '{}'", self.server_name)
            })?;
            map.insert(id, tx);
        }

        // Build JSON-RPC request
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let request_line = serde_json::to_string(&request)? + "\n";

        // Write to stdin
        {
            let mut writer = self.writer.lock().await;
            if let Err(e) = writer.write_all(request_line.as_bytes()).await {
                self.remove_pending(id);
                return Err(anyhow::anyhow!(
                    "Write to '{}' stdin failed: {}",
                    self.server_name,
                    e
                ));
            }
            if let Err(e) = writer.flush().await {
                self.remove_pending(id);
                return Err(anyhow::anyhow!(
                    "Flush to '{}' stdin failed: {}",
                    self.server_name,
                    e
                ));
            }
        }

        debug!("📤 [{}] → {} (id={})", self.server_name, method, id);

        // Wait for response with timeout
        let result = tokio::time::timeout(Duration::from_secs(30), rx)
            .await
            .map_err(|_| {
                self.remove_pending(id);
                anyhow::anyhow!(
                    "Request to '{}' timed out after 30s (method={}, id={})",
                    self.server_name,
                    method,
                    id
                )
            })?
            .map_err(|_| {
                anyhow::anyhow!(
                    "Response channel for '{}' dropped (server may have died)",
                    self.server_name
                )
            })?;

        match result {
            Ok(value) => {
                debug!("📥 [{}] ← {} (id={}) OK", self.server_name, method, id);
                Ok(value)
            }
            Err(error_msg) => {
                warn!(
                    "📥 [{}] ← {} (id={}) ERROR: {}",
                    self.server_name, method, id, error_msg
                );
                Err(anyhow::anyhow!(
                    "Upstream error from '{}': {}",
                    self.server_name,
                    error_msg
                ))
            }
        }
    }

    /// Send a JSON-RPC notification (no id, no response expected).
    pub async fn send_notification(&self, method: &str) -> anyhow::Result<()> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
        });

        let line = serde_json::to_string(&notification)? + "\n";

        let mut writer = self.writer.lock().await;
        writer.write_all(line.as_bytes()).await?;
        writer.flush().await?;

        debug!("📤 [{}] → {} (notification)", self.server_name, method);
        Ok(())
    }

    /// Remove a pending request entry (used on timeout / write failure).
    fn remove_pending(&self, id: i64) {
        if let Ok(mut map) = self.pending.lock() {
            map.remove(&id);
        }
    }
}

// ─────────────────────────────────────────────
// Background Reader Task
// ─────────────────────────────────────────────

/// Continuously reads JSON-RPC responses from a child's stdout and
/// resolves pending futures by matching the `id` field.
///
/// Also handles:
/// - Server-initiated notifications (logged, no id)
/// - Non-JSON lines from stdout (ignored)
/// - EOF (child stdout closed — all pending requests are failed)
async fn reader_loop(
    stdout: tokio::process::ChildStdout,
    pending: Arc<Mutex<HashMap<i64, oneshot::Sender<PendingResult>>>>,
    server_name: String,
) {
    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();

    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                let trimmed = line.trim();
                if trimmed.is_empty() || !trimmed.starts_with('{') {
                    continue;
                }

                let parsed: serde_json::Value = match serde_json::from_str(trimmed) {
                    Ok(v) => v,
                    Err(_) => {
                        // Non-JSON output from the server (e.g. startup banners)
                        continue;
                    }
                };

                // Response (has id) or notification (no id)?
                if let Some(id) = parsed.get("id").and_then(|i| i.as_i64()) {
                    // It's a response — build the result and resolve the pending future
                    let result = if let Some(result_val) = parsed.get("result") {
                        Ok(result_val.clone())
                    } else if let Some(error_val) = parsed.get("error") {
                        let message = error_val
                            .get("message")
                            .and_then(|m| m.as_str())
                            .unwrap_or("Unknown error");
                        let code = error_val.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
                        Err(format!("{} (code: {})", message, code))
                    } else {
                        Err("Response has neither 'result' nor 'error'".to_string())
                    };

                    if let Ok(mut map) = pending.lock() {
                        if let Some(sender) = map.remove(&id) {
                            let _ = sender.send(result);
                        } else {
                            debug!(
                                "[{}] Response for unknown id={} (possibly timed out)",
                                server_name, id
                            );
                        }
                    }
                } else if let Some(method) = parsed.get("method").and_then(|m| m.as_str()) {
                    // Server-initiated notification
                    debug!("[{}] 📣 Server notification: {}", server_name, method);
                }
            }
            Ok(None) => {
                // EOF — child's stdout has closed
                info!("📡 [{}] stdout closed (server process ended)", server_name);
                break;
            }
            Err(e) => {
                error!("[{}] Error reading stdout: {}", server_name, e);
                break;
            }
        }
    }

    // Drain remaining pending requests with errors
    if let Ok(mut map) = pending.lock() {
        let count = map.len();
        for (id, sender) in map.drain() {
            let _ = sender.send(Err(format!(
                "Connection to '{}' lost while request id={} was pending",
                server_name, id
            )));
        }
        if count > 0 {
            warn!(
                "[{}] Drained {} pending request(s) after connection loss",
                server_name, count
            );
        }
    }
}
