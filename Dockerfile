# ─────────────────────────────────────
# MCPlex Dockerfile — Multi-stage build
# Produces a minimal ~15MB container
# ─────────────────────────────────────

# Stage 1: Build
FROM rust:1.82-slim AS builder

WORKDIR /build

# Cache dependency compilation
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && \
    echo 'fn main() { println!("placeholder"); }' > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Build the real binary
COPY src/ src/
RUN touch src/main.rs && cargo build --release

# Stage 2: Runtime
FROM debian:bookworm-slim AS runtime

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates curl && \
    rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN groupadd -r mcplex && useradd -r -g mcplex -d /app mcplex

WORKDIR /app

# Copy binary
COPY --from=builder /build/target/release/mcplex /usr/local/bin/mcplex

# Copy default config
COPY mcplex.toml /app/mcplex.toml

# Create log directory
RUN mkdir -p /app/logs && chown -R mcplex:mcplex /app

USER mcplex

# MCP endpoint + Dashboard
EXPOSE 3100 9090

# Health check
HEALTHCHECK --interval=15s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:3100/health || exit 1

ENTRYPOINT ["mcplex"]
CMD ["--config", "/app/mcplex.toml"]
