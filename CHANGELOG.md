# Changelog

All notable changes to MCPlex are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
