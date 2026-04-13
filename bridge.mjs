#!/usr/bin/env node
/**
 * MCPlex Stdio Bridge
 * Translates stdio ↔ HTTP so Claude Code / Claude Desktop can talk to
 * the MCPlex HTTP gateway on **any** platform (macOS, Windows, Linux).
 *
 * Usage:
 *   node bridge.mjs [gateway_url]
 *   MCPLEX_GATEWAY=http://127.0.0.1:3100/mcp  node bridge.mjs
 *
 * Fixed in v1.1.0:
 *   - Removed readline output→stdout (prevented potential protocol corruption)
 *   - Added HTTP error handling (non-200 responses no longer silently fail)
 *   - Added prompts/list and prompts/get forwarding
 *   - Handles MCPlex returning id:null for notifications gracefully
 *   - Windows + macOS + Linux compatible
 */

import http from 'http';
import https from 'https';
import readline from 'readline';

const GATEWAY_URL = process.env.MCPLEX_GATEWAY || process.argv[2] || 'http://127.0.0.1:3100/mcp';

// Set process title for easier identification in task managers
process.title = 'mcplex-bridge';

const rl = readline.createInterface({
  input: process.stdin,
  terminal: false,
});

// Track original ID types so we can restore them in responses.
// Claude Desktop sends numeric IDs (id: 1) but MCPlex requires strings.
const idTypeMap = new Map();

/**
 * Send a JSON-RPC message to the client (Claude) via stdout.
 * Restores the original ID type so strict Zod validation passes.
 */
function sendMessage(msg) {
  // Don't forward responses with id:null — these come from MCPlex
  // replying to notifications which should never produce responses.
  if (msg.id === null || msg.id === undefined) {
    // Only forward if it's a server-initiated notification (has method, no id)
    if (msg.method) {
      process.stdout.write(JSON.stringify(msg) + '\n');
    }
    return;
  }

  if (idTypeMap.has(String(msg.id))) {
    const originalType = idTypeMap.get(String(msg.id));
    if (originalType === 'number') {
      msg.id = Number(msg.id);
    }
    idTypeMap.delete(String(msg.id));
  }
  process.stdout.write(JSON.stringify(msg) + '\n');
}

/**
 * HTTP POST to MCPlex gateway.
 * Handles both http:// and https:// URLs.
 * Properly reports HTTP errors instead of silently failing.
 */
async function callMCPlex(method, jsonrpc, id, params) {
  return new Promise((resolve, reject) => {
    // Ensure id is string for MCPlex
    const stringId = typeof id === 'number' ? String(id) : id;

    const body = JSON.stringify({
      jsonrpc: '2.0',
      id: stringId,
      method,
      params: params || {},
    });

    const url = new URL(GATEWAY_URL);
    const transport = url.protocol === 'https:' ? https : http;
    const options = {
      hostname: url.hostname,
      port: url.port || (url.protocol === 'https:' ? 443 : 3100),
      path: url.pathname || '/',
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Content-Length': Buffer.byteLength(body),
      },
    };

    const req = transport.request(options, (res) => {
      let data = '';
      res.on('data', (chunk) => (data += chunk));
      res.on('end', () => {
        // Handle HTTP-level errors (auth failures, rate limits, etc.)
        if (res.statusCode >= 400) {
          reject(new Error(`MCPlex returned HTTP ${res.statusCode}: ${data.slice(0, 200)}`));
          return;
        }

        try {
          const response = JSON.parse(data);
          resolve(response);
        } catch (e) {
          reject(new Error(`Invalid JSON from MCPlex: ${data.slice(0, 200)}`));
        }
      });
    });

    req.on('error', (err) => {
      reject(new Error(`Connection to MCPlex failed (${GATEWAY_URL}): ${err.message}`));
    });

    req.write(body);
    req.end();
  });
}

/**
 * Generic handler: forward a method to MCPlex and relay the response.
 */
async function forwardToMCPlex(id, method, params) {
  try {
    const response = await callMCPlex(method, '2.0', id, params || {});
    sendMessage(response);
  } catch (error) {
    sendMessage({
      jsonrpc: '2.0',
      id,
      error: {
        code: -32603,
        message: `${method} failed: ${error.message}`,
      },
    });
  }
}

/**
 * Handle the initialize handshake — the bridge acts as the MCP client
 * and relays capabilities back to the real client (Claude).
 */
async function handleInitialize(id) {
  try {
    const response = await callMCPlex('initialize', '2.0', id, {
      protocolVersion: '2024-11-05',
      capabilities: {
        experimental: {},
        roots: { listChanged: true },
        sampling: {},
      },
      clientInfo: {
        name: 'mcplex-bridge',
        version: '1.1.0',
      },
    });

    sendMessage(response);
  } catch (error) {
    sendMessage({
      jsonrpc: '2.0',
      id,
      error: {
        code: -32603,
        message: `Initialize failed: ${error.message}`,
      },
    });
  }
}

// ─────────────────────────────────────────────
// Process incoming JSON-RPC messages from Claude
// ─────────────────────────────────────────────

rl.on('line', async (line) => {
  try {
    const msg = JSON.parse(line);
    let { jsonrpc, id, method, params } = msg;

    // Track original ID type, then convert to string for MCPlex
    if (id !== undefined && id !== null) {
      idTypeMap.set(String(id), typeof id);
    }
    if (typeof id === 'number') {
      id = String(id);
    }

    // Handle by method
    switch (method) {
      case 'initialize':
        await handleInitialize(id);
        break;

      case 'initialized':
      case 'notifications/initialized':
        // Client→server notification — no response expected.
        // MCPlex returns id:null which breaks Claude Desktop's Zod validation.
        // Silently swallow these.
        break;

      case 'notifications/cancelled':
        // Cancellation notification — don't forward, no response expected
        break;

      case 'ping':
        // Respond to ping directly for lower latency
        sendMessage({ jsonrpc: '2.0', id, result: {} });
        break;

      // ── Core MCP methods ──────────────────────
      case 'tools/list':
      case 'tools/call':
      case 'resources/list':
      case 'resources/read':
      case 'resources/templates/list':
      case 'prompts/list':
      case 'prompts/get':
      case 'completion/complete':
        await forwardToMCPlex(id, method, params);
        break;

      default:
        // Any notification (no id) — don't forward to avoid id:null responses
        if (id === undefined || id === null) {
          break;
        }
        // Forward unknown methods to MCPlex (future-proofing)
        await forwardToMCPlex(id, method, params);
    }
  } catch (error) {
    // Silently ignore malformed JSON (non-JSON lines from stdin)
  }
});

rl.on('close', () => {
  process.exit(0);
});

// Handle SIGTERM/SIGINT gracefully (Windows compatibility)
process.on('SIGTERM', () => process.exit(0));
process.on('SIGINT', () => process.exit(0));

// Log startup to stderr (won't interfere with stdio protocol)
console.error(`[MCPlex Bridge v1.1.0] Connected to ${GATEWAY_URL}`);
console.error(`[MCPlex Bridge] Platform: ${process.platform} | Node: ${process.version}`);
