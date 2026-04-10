# MCPlex Example Configurations

This directory contains ready-to-use configuration examples for common MCPlex setups.

| File | Description |
|------|-------------|
| [`minimal.toml`](minimal.toml) | Simplest possible config — one server, no auth |
| [`dev-team.toml`](dev-team.toml) | Multi-server setup with RBAC for a dev team |
| [`production.toml`](production.toml) | Production-hardened with caching, rate limits, and audit logs |

## Usage

```bash
# Copy the example that matches your use case
cp examples/dev-team.toml my-config.toml

# Edit with your actual tokens/paths
nano my-config.toml

# Run
mcplex --config my-config.toml
```
