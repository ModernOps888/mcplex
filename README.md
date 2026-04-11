<div align="center">

# 🚀 MCPlex — The MCP Smart Gateway

**Semantic tool routing • Security guardrails • Real-time observability**

[![CI](https://github.com/ModernOps888/mcplex/actions/workflows/ci.yml/badge.svg)](https://github.com/ModernOps888/mcplex/actions/workflows/ci.yml)
[![Release](https://github.com/ModernOps888/mcplex/actions/workflows/release.yml/badge.svg)](https://github.com/ModernOps888/mcplex/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-cyan.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![MCP](https://img.shields.io/badge/MCP-2025--03--26-blue.svg)](https://modelcontextprotocol.io)

*Stop dumping 50k tokens of tool definitions into your LLM's context window.*
*MCPlex intelligently routes only the tools your agent actually needs.*

</div>

---

## The Problem

Every developer building multi-agent AI systems with MCP hits the same wall:

| Pain Point | Impact |
|-----------|--------|
| 🧠 **Context Bloat** | 20+ MCP servers = 50k+ tokens of tool definitions consuming your context window |
| 🔓 **No Security** | No RBAC, no audit trails, tool poisoning vulnerabilities |
| 👁️ **Blind Operations** | Can't track costs, latency, or debug wrong tool selection |
| 🔄 **Restart Required** | Config changes require full restart in production |
| 🕸️ **N×M Complexity** | Orchestrating dozens of servers is an integration nightmare |

## The Solution

MCPlex is a **single-binary Rust gateway** that sits between your AI agent and MCP servers:

```
Your Agent ──→ MCPlex Gateway ──→ GitHub MCP     (stdio — persistent)
                    │           ──→ Slack MCP      (stdio — persistent)
                    │           ──→ Database MCP   (HTTP)
                    │           ──→ Filesystem MCP  (stdio — persistent)
                    ▼
            🧠 Smart Routing (70-90% token savings)
            🔒 RBAC + Audit Logs + API Key Auth
            📊 Real-time Dashboard + Prometheus
            📦 Response Caching (auto-detect read-only)
            🔑 Multi-Tenant (API key → role mapping)
            🔥 Hot-reload Config
```

### Transport Support

MCPlex supports **both** MCP transport types as a first-class citizen:

| Transport | Discovery | Runtime Calls | Connection Model |
|-----------|-----------|---------------|-----------------|
| **Stdio** | ✅ Full MCP handshake | ✅ Multiplexed JSON-RPC | Persistent child process (long-lived) |
| **Streamable HTTP** | ✅ Full MCP handshake | ✅ Standard HTTP POST | Stateless (connection pooling) |

Stdio servers are spawned at startup and kept alive for the gateway's lifetime. The MCP handshake (`initialize` → `notifications/initialized`) runs once, then all subsequent `tools/call`, `resources/read`, and `prompts/get` requests are multiplexed over the same stdin/stdout pipe using JSON-RPC ID correlation.

## ⚡ Quick Start

### 1. Install (Pre-built Binary)

Download the latest release from [GitHub Releases](https://github.com/ModernOps888/mcplex/releases):

```bash
# Linux / macOS
curl -LO https://github.com/ModernOps888/mcplex/releases/latest/download/mcplex-linux-x86_64
chmod +x mcplex-linux-x86_64
sudo mv mcplex-linux-x86_64 /usr/local/bin/mcplex
```

### 2. Build from Source

```bash
git clone https://github.com/modernops888/mcplex.git
cd mcplex
cargo build --release
```

### 3. Configure

```bash
cp mcplex.toml my-config.toml
# Edit my-config.toml with your MCP servers
```

**Minimal config for stdio servers:**

```toml
[gateway]
listen = "127.0.0.1:3100"
dashboard = "127.0.0.1:9090"

[router]
strategy = "semantic"

[[servers]]
name = "filesystem"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]

[[servers]]
name = "memory"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-memory"]
```

### 4. Run

```bash
./target/release/mcplex --config my-config.toml

# Expected output:
# 🔌 Spawning stdio server 'filesystem': npx ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
# 🤝 MCP handshake complete for 'filesystem'
# 📡 Server 'filesystem': 11 tools, 0 resources, 0 prompts
# 🔌 Spawning stdio server 'memory': npx ["-y", "@modelcontextprotocol/server-memory"]
# 🤝 MCP handshake complete for 'memory'
# 📡 Server 'memory': 3 tools, 0 resources, 0 prompts
# ⚡ MCPlex gateway listening on 127.0.0.1:3100
```

### 5. Connect Your Agent

Point your MCP client to `http://127.0.0.1:3100/mcp` and open the dashboard at `http://127.0.0.1:9090`.

## 🔌 How to Connect Your Agent

MCPlex is a **transparent MCP proxy** — any MCP client that supports Streamable HTTP can connect to it. Your agent talks to MCPlex as if it were a single MCP server, and MCPlex handles multiplexing, routing, and security behind the scenes.

### Claude Desktop

Add to your Claude Desktop config (`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "mcplex": {
      "url": "http://127.0.0.1:3100/mcp",
      "headers": {
        "Authorization": "Bearer YOUR_API_KEY"
      }
    }
  }
}
```

### Cursor / Windsurf / Any MCP Client

Any client that supports streamable HTTP MCP servers:

```json
{
  "mcpServers": {
    "mcplex-gateway": {
      "url": "http://127.0.0.1:3100/mcp"
    }
  }
}
```

### Custom Python Agent

```python
import requests

GATEWAY = "http://127.0.0.1:3100/mcp"
HEADERS = {"Authorization": "Bearer YOUR_API_KEY"}  # Optional

# Initialize
resp = requests.post(GATEWAY, json={
    "jsonrpc": "2.0", "id": 1, "method": "initialize",
    "params": {"protocolVersion": "2025-03-26", "capabilities": {},
               "clientInfo": {"name": "my-agent", "version": "1.0"}}
}, headers=HEADERS)

# List all tools (MCPlex aggregates from all servers)
resp = requests.post(GATEWAY, json={
    "jsonrpc": "2.0", "id": 2, "method": "tools/list"
}, headers=HEADERS)
tools = resp.json()["result"]["tools"]

# Call a tool (MCPlex routes to the right server automatically)
resp = requests.post(GATEWAY, json={
    "jsonrpc": "2.0", "id": 3, "method": "tools/call",
    "params": {"name": "create_issue", "arguments": {"repo": "my-repo", "title": "Bug fix"}}
}, headers=HEADERS)
```

### How It Catches Your Agent's Calls

MCPlex acts as a **man-in-the-middle proxy** for all MCP traffic:

```
Your Agent ──POST /mcp──→ MCPlex Gateway ──→ Upstream MCP Server
                              │                (persistent stdio or HTTP)
                              ├─ ✅ Auth check (API key)
                              ├─ 🚦 Rate limit check
                              ├─ 🔒 RBAC + allowlist/blocklist
                              ├─ 📝 Audit log (every call)
                              └─ 📊 Metrics (latency, tokens)
```

Every `tools/call` goes through the security engine and is logged. Every `tools/list` goes through the semantic router. There's no way to bypass it — if your agent uses MCPlex as its MCP endpoint, **all calls are intercepted, checked, and logged**.

## 🧠 Semantic Tool Routing

The killer feature. Instead of dumping all tool definitions into your LLM's context, MCPlex uses a **meta-tool pattern** that works with **every standard MCP client** — no custom extensions needed:

| Scenario | Without MCPlex | With MCPlex | Savings |
|----------|---------------|-------------|---------|
| 5 servers, 50 tools | ~10,000 tokens | ~200 tokens | **98%** |
| 10 servers, 100 tools | ~20,000 tokens | ~200 tokens | **99%** |
| 20 servers, 200 tools | ~40,000 tokens | ~200 tokens | **99.5%** |

### How It Works

When your agent calls `tools/list`, MCPlex returns **3 lightweight meta-tools** (~200 tokens) instead of all real tools:

```
Agent                        MCPlex Gateway
  │                               │
  ├──tools/list──────────────────►│  Returns: mcplex_find_tools, mcplex_call_tool,
  │                               │           mcplex_list_categories (~200 tokens)
  │                               │
  ├──mcplex_find_tools────────────►│  "store a memory"
  │◄──────────────────────────────┤  → [{name: "create_memory", desc: "...", inputSchema: {...}},
  │                               │     {name: "save_note", desc: "...", inputSchema: {...}}]
  │                               │
  ├──mcplex_call_tool─────────────►│  {name: "create_memory", arguments: {...}}
  │◄──────────────────────────────┤  → tool result (routed through security + cache + audit)
```

- **`mcplex_find_tools(query)`** — Search for tools by natural language intent. Returns matching tools with full schemas.
- **`mcplex_call_tool(name, arguments)`** — Execute a discovered tool. Routes through the full security/audit/cache pipeline.
- **`mcplex_list_categories()`** — Browse available tool categories (server groups) with tool counts.

This works with Claude Code, Claude Desktop, Cursor, Windsurf, and any other MCP client — no custom extensions or client-side plugins required.

### Routing Mode

MCPlex supports three routing modes via `router.mode`:

| Mode | Behavior | Client Compatibility |
|------|----------|---------------------|
| **`metatool`** (default) | Returns 3 gateway meta-tools; agent discovers real tools via `mcplex_find_tools` | ✅ All standard MCP clients |
| **`passthrough`** | Returns all real tools directly (no routing indirection) | ✅ All standard MCP clients |
| **`legacy`** | Uses `_mcplex_query` param extension for filtering | ❌ Custom clients only |

### Routing Strategy

Within `metatool` and `legacy` modes, MCPlex uses a routing strategy to rank tools:

- **`semantic`** — Character n-gram embeddings with cosine similarity (recommended)
- **`keyword`** — TF-IDF keyword matching (zero ML dependency)
- **`passthrough`** — No filtering (baseline)

```toml
[router]
mode = "metatool"            # "metatool", "passthrough", or "legacy"
strategy = "semantic"        # "semantic", "keyword", or "passthrough"
top_k = 5                    # Return top 5 most relevant tools
similarity_threshold = 0.3   # Minimum relevance score
cache_embeddings = true       # Cache for faster repeated queries
```

## 🔒 Security Engine

### Role-Based Access Control (RBAC)

```toml
[security]
enable_rbac = true

[roles.developer]
allowed_tools = ["github/*", "database/query_*"]

[roles.admin]
allowed_tools = ["*"]

[roles.readonly]
allowed_tools = ["*/list_*", "*/get_*"]
blocked_tools = ["*/delete_*", "*/drop_*"]
```

### Per-Server Tool Blocklists

```toml
[[servers]]
name = "database"
url = "http://localhost:8080/mcp"
blocked_tools = ["drop_table", "delete_*", "truncate_*"]
```

### Structured Audit Logging

Every tool invocation is logged as JSON Lines:

```json
{"timestamp":"2026-04-10T10:00:00Z","event":"tool_call","tool_name":"github/create_issue","server_name":"github","duration_ms":342,"trace_id":"a1b2c3d4"}
{"timestamp":"2026-04-10T10:00:01Z","event":"tool_blocked","tool_name":"database/drop_table","reason":"security_policy","trace_id":"e5f6g7h8"}
```

## 📊 Real-time Dashboard

Built-in observability dashboard at `http://localhost:9090`:

- **Global Metrics** — Total requests, tool calls, errors, tokens saved
- **Per-Tool Stats** — Invocation count, avg/p50/p95/p99 latency
- **Server Status** — Connected servers, transport type, tool/resource/prompt counts
- **Live Event Feed** — Real-time stream of all gateway activity

The dashboard auto-refreshes every 3 seconds with zero configuration.

## 📦 Response Caching

Avoid redundant upstream calls for read-only tools:

```toml
[cache]
enabled = true
ttl_seconds = 300     # 5 minute TTL
max_entries = 1000    # Max cached responses
```

MCPlex **auto-detects** read-only tools by prefix (`list_*`, `get_*`, `search_*`, `query_*`, `describe_*`, `show_*`). You can override with custom patterns:

```toml
[cache]
patterns = ["my_custom_tool", "expensive_*"]
```

Write operations (`create_*`, `update_*`, `delete_*`) are **never cached** by default.

## 🔑 Multi-Tenant API Keys

Map API keys to RBAC roles for team-based access:

```toml
[api_keys."sk-dev-team-abc123"]
role = "developer"
description = "Dev team key"

[api_keys."sk-admin-xyz789"]
role = "admin"
description = "Admin key"
```

When a request comes in with `Authorization: Bearer sk-dev-team-abc123`, MCPlex automatically applies the `developer` role's RBAC policies.

## 🔥 Hot-Reload Configuration

Config changes apply instantly — no restart needed:

```bash
# Edit config while MCPlex is running
vim mcplex.toml

# MCPlex automatically detects changes:
# 🔄 Config file changed, reloading...
# ✅ Configuration reloaded successfully
```

## 📖 Full MCP Capability Support

MCPlex aggregates and forwards **all three** MCP capability types from upstream servers:

| Capability | List | Execute/Read | Routing |
|-----------|------|-------------|---------|
| **Tools** | `tools/list` → aggregated | `tools/call` → routed to owner | ✅ Semantic / Keyword |
| **Resources** | `resources/list` → aggregated | `resources/read` → routed by URI | Direct routing |
| **Prompts** | `prompts/list` → aggregated | `prompts/get` → routed by name | Direct routing |

## Configuration Reference

### `[gateway]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `listen` | string | `127.0.0.1:3100` | MCP client connection address |
| `dashboard` | string | — | Dashboard address (disabled if not set) |
| `hot_reload` | bool | `true` | Auto-reload config on file change |
| `name` | string | `mcplex` | Gateway instance name |
| `api_key` | string | — | API key for client auth (supports `${ENV}`) |
| `rate_limit_rps` | int | `0` | Max requests/sec per client (0 = unlimited) |

### `[router]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `mode` | string | `metatool` | `metatool`, `passthrough`, or `legacy` |
| `strategy` | string | `keyword` | `semantic`, `keyword`, or `passthrough` |
| `top_k` | int | `5` | Maximum tools returned per query |
| `similarity_threshold` | float | `0.3` | Minimum relevance score (0.0-1.0) |
| `cache_embeddings` | bool | `true` | Cache tool embeddings |

### `[security]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enable_rbac` | bool | `false` | Enable role-based access control |
| `enable_audit_log` | bool | `false` | Enable structured audit logging |
| `audit_log_path` | string | `./logs/audit.jsonl` | Audit log file path |
| `max_log_size_mb` | int | `100` | Max log file size before rotation (keeps 5 backups) |

### `[[servers]]`

| Key | Type | Required | Description |
|-----|------|----------|-------------|
| `name` | string | ✅ | Unique server name |
| `command` | string | ⚡ | Executable path for stdio transport |
| `args` | list | — | Command arguments |
| `url` | string | ⚡ | URL for HTTP transport |
| `env` | map | — | Environment variables (supports `${VAR}`) |
| `allowed_roles` | list | — | Roles allowed to access this server |
| `blocked_tools` | list | — | Tool blocklist patterns (glob) |
| `allowed_tools` | list | — | Tool allowlist patterns (glob) |
| `enabled` | bool | `true` | Enable/disable this server |

⚡ = One of `command` or `url` is required

> **Note:** For stdio servers, `command` should be the **executable path** (e.g. `npx`, `/usr/bin/python3`). Additional arguments go in the `args` array.

### `[roles.<name>]`

| Key | Type | Description |
|-----|------|-------------|
| `allowed_tools` | list | Tool patterns this role can access (glob) |
| `blocked_tools` | list | Tool patterns this role cannot access (glob) |

**Glob Patterns:** `*` matches any characters, `?` matches a single character.
Examples: `github/*`, `*/query_*`, `database/get_?ser`

## Environment Variables

MCPlex supports `${ENV_VAR}` syntax in configuration:

```toml
[[servers]]
name = "github"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
env = { GITHUB_TOKEN = "${GITHUB_TOKEN}" }
```

## Architecture

```
┌───────────────────────────────────────────────────────────┐
│                      MCPlex Gateway                        │
│                                                           │
│  ┌─────────────┐  ┌──────────────┐  ┌──────────────────┐ │
│  │  Semantic    │  │   Security   │  │   Observability  │ │
│  │  Router      │  │   Engine     │  │   Collector      │ │
│  │             │  │              │  │                  │ │
│  │ • Embeddings │  │ • RBAC       │  │ • Token Savings  │ │
│  │ • TopK Match │  │ • Allowlist  │  │ • Latency (p99)  │ │
│  │ • Caching   │  │ • Audit Log  │  │ • Dashboard      │ │
│  └──────┬──────┘  └──────┬───────┘  └────────┬─────────┘ │
│         └────────────┬───┘                    │           │
│                      ▼                        │           │
│      ┌───────────────────────────────┐        │           │
│      │     MCP Protocol Multiplexer  │◄───────┘           │
│      │     + Response Cache          │                    │
│      └──────────┬────────────────────┘                    │
└─────────────────┼─────────────────────────────────────────┘
                  │
    ┌─────────────┼─────────────────────┐
    ▼             ▼                     ▼
MCP Server    MCP Server           MCP Server
(stdio —      (stdio —             (HTTP —
 persistent)   persistent)          stateless)
```

## CLI Reference

```bash
mcplex [OPTIONS]

Options:
  -c, --config <FILE>    Config file path [default: mcplex.toml]
  -v, --verbose          Enable verbose logging
  --listen <ADDR>        Override gateway listen address
  --dashboard <ADDR>     Override dashboard listen address
  --check                Validate config and exit
  -h, --help             Print help
  -V, --version          Print version
```

## 🛡️ Production Hardening

### API Key Authentication

Secure your gateway so only authorized agents can connect:

```toml
[gateway]
api_key = "${MCPLEX_API_KEY}"  # Set via environment variable
```

Clients authenticate via header:
```
Authorization: Bearer your-secret-key
# or
X-API-Key: your-secret-key
```

Health checks (`/health`) are always unauthenticated for load balancer probes.

### Rate Limiting

Prevent runaway agents from overwhelming your servers:

```toml
[gateway]
rate_limit_rps = 50  # Max 50 requests/sec per client (burst: 100)
```

Uses a per-client token bucket with 2x burst allowance. Returns `429 Too Many Requests` when exceeded.

### Log Rotation

Audit logs rotate automatically — they won't fill your disk:

```toml
[security]
enable_audit_log = true
audit_log_path = "./logs/audit.jsonl"
max_log_size_mb = 100  # Rotates at 100MB, keeps 5 backups
```

Files rotate as: `audit.jsonl` → `audit.jsonl.1` → ... → `audit.jsonl.5` (oldest deleted).

### Network Security

Bind to localhost only (default) for local agents:
```toml
[gateway]
listen = "127.0.0.1:3100"     # Localhost only
dashboard = "127.0.0.1:9090"  # Dashboard also localhost
```

For network access, use a reverse proxy (nginx/caddy) with TLS.

### Prometheus Monitoring

MCPlex exposes a Prometheus-compatible `/api/metrics` endpoint on the dashboard port for external monitoring:

```
mcplex_requests_total 1234
mcplex_tool_calls_total 567
mcplex_errors_total 3
mcplex_tokens_saved_total 45000
mcplex_tool_duration_ms{tool="create_issue",quantile="0.95"} 142
```

## Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Run `cargo fmt && cargo clippy -- -D warnings && cargo test`
4. Commit your changes (`git commit -m 'Add amazing feature'`)
5. Push to the branch (`git push origin feature/amazing-feature`)
6. Open a Pull Request

## License

MIT License — see [LICENSE](LICENSE) for details.

---

<div align="center">

**Built with 🦀 Rust for the AI agent community**

*If MCPlex saves your context window, give it a ⭐*

</div>
