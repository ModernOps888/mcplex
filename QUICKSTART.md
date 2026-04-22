# ⚡ MCPlex Quickstart

Get MCPlex running in **under 2 minutes**. Pick your preferred method:

| Method | Time | Best For |
|--------|------|----------|
| [🐳 Docker](#docker) | ~30s | Production, CI/CD |
| [📦 Pre-built Binary](#pre-built-binary) | ~60s | Local development |
| [🔧 Build from Source](#build-from-source) | ~90s | Contributors |
| [🚀 Service Deployment](README.md#running-as-a-service-deployment) | ~10m | Persistent Background Services |

---

## 🐳 Docker

The fastest way to get started. Zero dependencies required.

```bash
# 1. Clone the repo
git clone https://github.com/ModernOps888/mcplex.git && cd mcplex

# 2. Start with Docker Compose (includes example MCP servers)
docker compose up -d

# 3. Verify it's running
curl http://localhost:3100/health
# → {"status":"ok","uptime":"2s","servers":2}

# 4. Open the dashboard
open http://localhost:9090
```

**That's it.** Your gateway is live at `localhost:3100` and the observability dashboard at `localhost:9090`.

### Docker tips

```bash
# View logs
docker compose logs -f mcplex

# Stop everything
docker compose down

# Rebuild after config changes
docker compose up -d --build

# Run with a custom config
docker run -p 3100:3100 -p 9090:9090 \
  -v $(pwd)/my-config.toml:/app/mcplex.toml \
  mcplex:latest
```

---

## 📦 Pre-built Binary

Download from [GitHub Releases](https://github.com/ModernOps888/mcplex/releases):

```bash
# Linux/macOS
curl -fsSL https://github.com/ModernOps888/mcplex/releases/latest/download/mcplex-$(uname -s)-$(uname -m) -o mcplex
chmod +x mcplex

# Create your config
cp mcplex.toml my-config.toml
# Edit my-config.toml — uncomment the servers you want

# Run
./mcplex --config my-config.toml
```

---

## 🔧 Build from Source

Requires [Rust 1.75+](https://rustup.rs/).

```bash
git clone https://github.com/ModernOps888/mcplex.git && cd mcplex
cargo build --release
./target/release/mcplex --config mcplex.toml
```

---

## 🔌 Connect Your Agent

Once MCPlex is running, point any MCP client at it:

### Claude Desktop / Claude Code

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

### Cursor / Windsurf / Any Streamable-HTTP Client

```json
{
  "mcpServers": {
    "mcplex": {
      "url": "http://127.0.0.1:3100/mcp"
    }
  }
}
```

### Python SDK

```python
from mcp import ClientSession
from mcp.client.streamable_http import streamablehttp_client

async with streamablehttp_client("http://127.0.0.1:3100/mcp") as (r, w, _):
    async with ClientSession(r, w) as session:
        await session.initialize()
        tools = await session.list_tools()
        print(f"Available tools: {len(tools.tools)}")
```

---

## 📋 Minimal Config Example

The simplest working config — one MCP server, no auth:

```toml
[gateway]
listen = "127.0.0.1:3100"
dashboard = "127.0.0.1:9090"
hot_reload = true
name = "dev-gateway"

[router]
strategy = "keyword"
top_k = 5

[security]
enable_rbac = false
enable_audit_log = false

[[servers]]
name = "filesystem"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "./"]
```

## 🔒 Production Config Example

Locked-down config with RBAC, audit logs, caching, and multi-tenant keys:

```toml
[gateway]
listen = "0.0.0.0:3100"
dashboard = "127.0.0.1:9090"
hot_reload = true
name = "prod-gateway"
rate_limit_rps = 100

[router]
strategy = "keyword"
top_k = 5
similarity_threshold = 0.15

[security]
enable_rbac = true
enable_audit_log = true
audit_log_path = "./logs/audit.jsonl"

[[servers]]
name = "github"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
env = { GITHUB_TOKEN = "${GITHUB_TOKEN}" }
allowed_roles = ["developer", "admin"]

[[servers]]
name = "database"
url = "http://db-mcp:8080/mcp"
transport = "streamable-http"
blocked_tools = ["drop_table", "delete_*", "truncate_*"]
allowed_roles = ["admin"]

[roles.developer]
allowed_tools = ["github/*", "filesystem/*"]

[roles.admin]
allowed_tools = ["*"]

[cache]
enabled = true
ttl_seconds = 300
max_entries = 1000

[api_keys."sk-dev-team-abc123"]
role = "developer"
description = "Dev team key"

[api_keys."sk-admin-xyz789"]
role = "admin"
description = "Admin key"
```

---

## 🆘 Troubleshooting

| Symptom | Fix |
|---------|-----|
| `Connection refused` | Check that `listen` address matches your client config |
| `No tools returned` | Ensure at least one `[[servers]]` block is uncommented |
| `RBAC denied` | Verify API key role has access to the requested tools |
| `Server timeout` | Increase timeout in your MCP client; check server health |
| Config not reloading | Ensure `hot_reload = true` and file is saved (not just buffered) |

**Need help?** [Open an issue](https://github.com/ModernOps888/mcplex/issues) or join the community.

---

<div align="center">

**[📖 Full Documentation](README.md)** · **[🐛 Report Bug](https://github.com/ModernOps888/mcplex/issues)** · **[💡 Request Feature](https://github.com/ModernOps888/mcplex/issues)**

Built with ❤️ by [Infinity Tech Stack](https://infinitytechstack.uk)

</div>
