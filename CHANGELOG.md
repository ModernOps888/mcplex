# Changelog

All notable changes to MCPlex are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.5.0] — 2026-07-04

### 🔒 Security by Design
- **Constant-time API key comparison** — Gateway key verification no longer leaks timing information
- **Trusted role binding for multi-tenant keys** — Authenticated API keys resolve to their configured RBAC role at the middleware layer; the server-side `X-MCPlex-Role` header now takes precedence over the client-supplied `_mcplex_role` param, closing a role self-assertion hole
- **Tool call input validation** — Tool names restricted to `[A-Za-z0-9/_.-]` (max 128 chars); arguments capped at 64KB with max JSON nesting depth of 16; invalid calls rejected with `-32602` before reaching upstream servers
- **1MB request body limit** on the gateway endpoint (defense against memory-exhaustion payloads)
- **Security event telemetry** — New `EventType::Security` with `security_events`, `blocked_tool_calls`, and `rejected_tool_calls` global counters
- **RBAC enforcement through meta-tools** — `mcplex_call_tool` proxy dispatch now carries the authenticated role end-to-end

### ✨ Features & GUI
- **Dashboard security cards** — "Blocked Calls" and "Rejected Inputs" metric cards with error highlighting
- **Security-posture pill** — Live RBAC/Audit status indicator in the dashboard header
- **Security events in the live event stream** — 🛡️ styled entries for blocks and rejections
- **Dynamic version display** — CLI banner and dashboard header render the crate version automatically (no more stale hardcoded versions)
- **VS Code / GitHub Copilot integration** — Documented `.vscode/mcp.json` setup plus a ready-to-copy template at `examples/vscode-mcp.json`, bringing meta-tool token savings into IDE agent sessions

### 🧪 Testing
- 6 new unit tests covering constant-time comparison, tool-name validation, argument size/depth limits

## [0.4.0] — 2026-06-20

### 🔴 Critical Fixes
- **Wired up Prometheus metrics endpoint** — `/api/prometheus` on dashboard port now returns Prometheus-compatible metrics for scraping (was defined in `export.rs` but never mounted to a route)
- **Wired up AgentLens bridge** — `[agentlens]` config section now actually creates and uses the bridge for event forwarding to the AgentLens timeline UI (was fully implemented but never instantiated)
- **Fixed bridge.mjs protocol version** — Updated from `2024-11-05` to `2025-11-25` to match gateway (was a protocol mismatch causing potential negotiation issues)
- **Fixed RBAC default-allow** — When RBAC is enabled but no role is provided, access is now **denied by default** for security (was incorrectly allowing all access)
- **Removed unused `which` crate** — Eliminated dead dependency from Cargo.toml

### 🟡 Code Quality
- **Extracted shared utilities** — `glob_match()`, `now_iso8601()`, `days_to_ymd()`, and `is_leap_year()` deduplicated into new `src/util.rs` module (was duplicated across rbac.rs, allowlist.rs, metrics.rs, and audit.rs)
- **VecDeque for ring buffers** — Replaced O(n) `Vec::remove(0)` with O(1) `VecDeque::pop_front()` in metrics event buffer and per-tool duration tracking
- **Connection pooling** — Reuse shared `reqwest::Client` across HTTP upstream calls instead of creating a new client per request (better connection reuse and performance)
- **Removed `#![allow(dead_code)]`** — All dead code issues resolved; global suppression no longer needed
- **Fixed `is_multiple_of()` usage** — Replaced nightly-only API with standard modulo operator for broader Rust version compatibility
- **4 new unit tests** — Added tests for shared utility functions (glob_match, ISO 8601 formatting, leap year, epoch date conversion)

### 🟢 Protocol Upgrade
- **MCP spec 2025-11-25** — Updated protocol version from `2025-03-26` to current stable `2025-11-25` across all 4 hardcoded locations (multiplexer.rs, stdio.rs, transport.rs, bridge.mjs)
- **Bridge protocol sync** — Bridge now sends matching protocol version to gateway

### 📝 Documentation
- Updated Dockerfile to Rust 1.85-slim (from 1.82)
- Bumped version to 0.4.0 across Cargo.toml and banner
- Added this CHANGELOG entry

## [0.3.0] — 2026-04-11

### Added
- **Meta-tool pattern** for standard MCP client compatibility ([#2](https://github.com/ModernOps888/mcplex/issues/2))
  - `mcplex_find_tools(query)` — natural-language tool discovery
  - `mcplex_call_tool(name, arguments)` — routed execution via meta-tool
  - `mcplex_list_categories()` — browse server groups and tool counts
  - Works with Claude Code, Claude Desktop, Cursor, Windsurf, and all standard MCP clients
- **IDF-weighted semantic routing** for higher-quality tool matching ([#3](https://github.com/ModernOps888/mcplex/issues/3))
- **Server-name boosting** — queries mentioning a server name get an automatic relevance boost
- **Router mode configuration** — `metatool` (default), `passthrough`, and `legacy` modes

### Fixed
- Noisy log warnings for standard MCP lifecycle methods (`notifications/initialized`, `completion/complete`) now silenced ([#3](https://github.com/ModernOps888/mcplex/issues/3))
- Cosmetic server name duplication in `serverInfo.name` during initialization ([#3](https://github.com/ModernOps888/mcplex/issues/3))

### Changed
- Default router mode changed from `legacy` to `metatool` for out-of-the-box compatibility with all MCP clients

## [0.2.0] — 2026-04-10

### Added
- **Persistent stdio connections** — long-lived child processes with multiplexed JSON-RPC
- **Proper MCP handshake** — `initialize` + `notifications/initialized` for all transports
- **Full resource support** — discovery, listing, and reading from upstream servers
- **Full prompt support** — discovery, listing, and `prompts/get` forwarding
- **Real SSE streaming** — live server status events with keepalive
- **Response caching** — auto-detect read-only tools, configurable TTL
- **Multi-tenant API keys** — key-to-role mapping for shared deployments
- **RBAC + Audit** — role-based tool access control with structured audit logs
- **Dockerfile + docker-compose** for containerised deployments
- **QUICKSTART guide** for rapid onboarding
- **CI/CD pipeline** — GitHub Actions for build, test, and release automation
- **Log rotation** — automatic audit log rotation at configurable size (default 100 MB, 5 backups)
- **Rate limiting** — per-client token-bucket rate limiter with burst allowance

### Changed
- Stdio transport redesigned from spawn-per-request to persistent connection model
- Release workflow triggers on `v*` tags with cross-platform binary builds

## [0.1.0] — 2026-04-09

### Added
- **Initial release** of MCPlex — The MCP Smart Gateway
- Semantic tool routing with character n-gram embeddings and cosine similarity
- Keyword routing via TF-IDF matching
- Security engine with RBAC, tool allowlists/blocklists, and structured audit logging
- Real-time observability dashboard with global metrics and per-tool stats
- Hot-reload configuration (file-watch with zero downtime)
- Streamable HTTP transport support
- API key authentication with environment variable expansion
- Prometheus-compatible `/api/metrics` endpoint
- CLI with `--config`, `--verbose`, `--listen`, `--dashboard`, and `--check` options
- MIT licensed

[0.3.0]: https://github.com/ModernOps888/mcplex/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/ModernOps888/mcplex/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/ModernOps888/mcplex/releases/tag/v0.1.0
