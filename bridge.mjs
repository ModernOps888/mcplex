#!/usr/bin/env node
/**
 * MCPlex Stdio Bridge
 * Acts as stdio MCP server for Claude, forwards to MCPlex HTTP gateway
 * Usage: node bridge.mjs [gateway_url]
 */

import http from 'http';
import readline from 'readline';

const GATEWAY_URL = process.env.MCPLEX_GATEWAY || process.argv[2] || 'http://127.0.0.1:3100/mcp';

// State
let requestId = 0;
const pendingRequests = new Map();

const rl = readline.createInterface({
  input: process.stdin,
  output: process.stdout,
  terminal: false,
});

// Send JSON line to Claude
function sendMessage(msg) {
  console.log(JSON.stringify(msg));
}

// HTTP POST to MCPlex
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
    const options = {
      hostname: url.hostname,
      port: url.port || 3100,
      path: url.pathname || '/',
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Content-Length': Buffer.byteLength(body),
      },
    };

    const req = http.request(options, (res) => {
      let data = '';
      res.on('data', (chunk) => (data += chunk));
      res.on('end', () => {
        try {
          const response = JSON.parse(data);
          resolve(response);
        } catch (e) {
          reject(new Error(`Invalid JSON from MCPlex: ${data}`));
        }
      });
    });

    req.on('error', reject);
    req.write(body);
    req.end();
  });
}

// Handle init message
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
        version: '1.0.0',
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

// Handle tools/list
async function handleToolsList(id) {
  try {
    const response = await callMCPlex('tools/list', '2.0', id, {});
    sendMessage(response);
  } catch (error) {
    sendMessage({
      jsonrpc: '2.0',
      id,
      error: {
        code: -32603,
        message: `Tools list failed: ${error.message}`,
      },
    });
  }
}

// Handle tools/call
async function handleToolCall(id, params) {
  try {
    const response = await callMCPlex('tools/call', '2.0', id, params);
    sendMessage(response);
  } catch (error) {
    sendMessage({
      jsonrpc: '2.0',
      id,
      error: {
        code: -32603,
        message: `Tool call failed: ${error.message}`,
      },
    });
  }
}

// Handle resources/list
async function handleResourcesList(id) {
  try {
    const response = await callMCPlex('resources/list', '2.0', id, {});
    sendMessage(response);
  } catch (error) {
    sendMessage({
      jsonrpc: '2.0',
      id,
      error: {
        code: -32603,
        message: `Resources list failed: ${error.message}`,
      },
    });
  }
}

// Handle resources/read
async function handleResourceRead(id, params) {
  try {
    const response = await callMCPlex('resources/read', '2.0', id, params);
    sendMessage(response);
  } catch (error) {
    sendMessage({
      jsonrpc: '2.0',
      id,
      error: {
        code: -32603,
        message: `Resource read failed: ${error.message}`,
      },
    });
  }
}

// Handle notifications
function handleNotification(method, params) {
  // Forward notifications (e.g., notifications/initialized)
  sendMessage({
    jsonrpc: '2.0',
    method,
    params,
  });
}

// Process incoming lines from Claude
rl.on('line', async (line) => {
  try {
    const msg = JSON.parse(line);
    let { jsonrpc, id, method, params } = msg;

    // Ensure id is a string for MCPlex compatibility
    if (typeof id === 'number') {
      id = String(id);
    }

    // Handle by method
    switch (method) {
      case 'initialize':
        await handleInitialize(id);
        break;
      case 'initialized':
        handleNotification('notifications/initialized', {});
        break;
      case 'tools/list':
        await handleToolsList(id);
        break;
      case 'tools/call':
        await handleToolCall(id, params);
        break;
      case 'resources/list':
        await handleResourcesList(id);
        break;
      case 'resources/read':
        await handleResourceRead(id, params);
        break;
      default:
        // Forward unknown methods to MCPlex
        try {
          const response = await callMCPlex(method, jsonrpc, id, params);
          sendMessage(response);
        } catch (error) {
          sendMessage({
            jsonrpc: '2.0',
            id,
            error: {
              code: -32601,
              message: `Method not found: ${method}`,
            },
          });
        }
    }
  } catch (error) {
    // Silently ignore malformed JSON
  }
});

rl.on('close', () => {
  process.exit(0);
});

// Log startup to stderr (won't interfere with stdio protocol)
console.error(`[MCPlex Bridge] Connected to ${GATEWAY_URL}`);
