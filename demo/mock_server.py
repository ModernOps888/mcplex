"""
MCPlex Demo — Mock MCP Server
A lightweight HTTP MCP server that exposes fake tools for demonstration.
Run: python mock_server.py [port]
"""

import json
import sys
from http.server import HTTPServer, BaseHTTPRequestHandler

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8081

# Define mock tools for different "servers"
TOOLS = {
    8081: [  # GitHub-like tools
        {"name": "create_issue", "description": "Create a new GitHub issue in a repository", "inputSchema": {"type": "object", "properties": {"repo": {"type": "string", "description": "Repository name"}, "title": {"type": "string", "description": "Issue title"}, "body": {"type": "string", "description": "Issue body"}}, "required": ["repo", "title"]}},
        {"name": "list_repos", "description": "List all repositories for the authenticated user", "inputSchema": {"type": "object", "properties": {"sort": {"type": "string", "description": "Sort order"}}}},
        {"name": "list_pull_requests", "description": "List pull requests in a repository", "inputSchema": {"type": "object", "properties": {"repo": {"type": "string"}, "state": {"type": "string"}}}},
        {"name": "search_code", "description": "Search for code patterns across repositories", "inputSchema": {"type": "object", "properties": {"query": {"type": "string", "description": "Search query"}, "language": {"type": "string"}}}},
        {"name": "create_pull_request", "description": "Create a new pull request", "inputSchema": {"type": "object", "properties": {"repo": {"type": "string"}, "title": {"type": "string"}, "head": {"type": "string"}, "base": {"type": "string"}}}},
        {"name": "get_file_contents", "description": "Get the contents of a file from a repository", "inputSchema": {"type": "object", "properties": {"repo": {"type": "string"}, "path": {"type": "string"}}}},
        {"name": "list_commits", "description": "List commits in a repository branch", "inputSchema": {"type": "object", "properties": {"repo": {"type": "string"}, "branch": {"type": "string"}}}},
        {"name": "create_branch", "description": "Create a new branch from a reference", "inputSchema": {"type": "object", "properties": {"repo": {"type": "string"}, "branch": {"type": "string"}, "from_ref": {"type": "string"}}}},
    ],
    8082: [  # Slack-like tools
        {"name": "send_message", "description": "Send a message to a Slack channel", "inputSchema": {"type": "object", "properties": {"channel": {"type": "string"}, "text": {"type": "string"}}}},
        {"name": "list_channels", "description": "List all Slack channels in the workspace", "inputSchema": {"type": "object", "properties": {}}},
        {"name": "get_channel_history", "description": "Get recent messages from a channel", "inputSchema": {"type": "object", "properties": {"channel": {"type": "string"}, "limit": {"type": "integer"}}}},
        {"name": "create_channel", "description": "Create a new Slack channel", "inputSchema": {"type": "object", "properties": {"name": {"type": "string"}, "is_private": {"type": "boolean"}}}},
        {"name": "upload_file", "description": "Upload a file to a Slack channel", "inputSchema": {"type": "object", "properties": {"channel": {"type": "string"}, "content": {"type": "string"}, "filename": {"type": "string"}}}},
        {"name": "search_messages", "description": "Search for messages across all channels", "inputSchema": {"type": "object", "properties": {"query": {"type": "string"}}}},
    ],
    8083: [  # Database-like tools
        {"name": "query_users", "description": "Query user records from the database", "inputSchema": {"type": "object", "properties": {"filter": {"type": "string"}, "limit": {"type": "integer"}}}},
        {"name": "insert_record", "description": "Insert a new record into a database table", "inputSchema": {"type": "object", "properties": {"table": {"type": "string"}, "data": {"type": "object"}}}},
        {"name": "update_record", "description": "Update existing records in a database table", "inputSchema": {"type": "object", "properties": {"table": {"type": "string"}, "filter": {"type": "string"}, "data": {"type": "object"}}}},
        {"name": "delete_records", "description": "Delete records from a database table (DANGEROUS)", "inputSchema": {"type": "object", "properties": {"table": {"type": "string"}, "filter": {"type": "string"}}}},
        {"name": "list_tables", "description": "List all tables in the database", "inputSchema": {"type": "object", "properties": {}}},
        {"name": "get_schema", "description": "Get the schema of a database table", "inputSchema": {"type": "object", "properties": {"table": {"type": "string"}}}},
        {"name": "run_migration", "description": "Run a database migration script", "inputSchema": {"type": "object", "properties": {"migration": {"type": "string"}}}},
        {"name": "drop_table", "description": "Drop a database table (EXTREMELY DANGEROUS)", "inputSchema": {"type": "object", "properties": {"table": {"type": "string"}}}},
        {"name": "export_csv", "description": "Export query results as CSV", "inputSchema": {"type": "object", "properties": {"query": {"type": "string"}, "output": {"type": "string"}}}},
        {"name": "backup_database", "description": "Create a backup of the entire database", "inputSchema": {"type": "object", "properties": {"destination": {"type": "string"}}}},
    ],
}

# Pick tools based on port
tools = TOOLS.get(PORT, TOOLS[8081])
server_names = {8081: "github-mock", 8082: "slack-mock", 8083: "database-mock"}
server_name = server_names.get(PORT, f"mock-{PORT}")

class MCPHandler(BaseHTTPRequestHandler):
    def do_POST(self):
        content_length = int(self.headers.get('Content-Length', 0))
        body = self.rfile.read(content_length)
        request = json.loads(body) if body else {}
        
        method = request.get("method", "")
        req_id = request.get("id")
        
        if method == "initialize":
            result = {
                "protocolVersion": "2025-03-26",
                "capabilities": {"tools": {"listChanged": False}},
                "serverInfo": {"name": server_name, "version": "1.0.0"}
            }
        elif method == "tools/list":
            result = {"tools": tools}
        elif method == "tools/call":
            params = request.get("params", {})
            tool_name = params.get("name", "unknown")
            # Simulate work with a small delay
            import time
            import random
            time.sleep(random.uniform(0.01, 0.1))
            result = {
                "content": [{"type": "text", "text": f"✅ Tool '{tool_name}' executed successfully on {server_name}. Args: {json.dumps(params.get('arguments', {}))}"}],
                "isError": False
            }
        elif method == "ping":
            result = {}
        else:
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.end_headers()
            self.wfile.write(json.dumps({"jsonrpc": "2.0", "id": req_id, "error": {"code": -32601, "message": f"Method '{method}' not found"}}).encode())
            return
        
        response = {"jsonrpc": "2.0", "id": req_id, "result": result}
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        self.wfile.write(json.dumps(response).encode())
    
    def log_message(self, format, *args):
        pass  # Suppress request logging

if __name__ == "__main__":
    httpd = HTTPServer(("127.0.0.1", PORT), MCPHandler)
    print(f"[MOCK] MCP server '{server_name}' running on http://127.0.0.1:{PORT} ({len(tools)} tools)")
    httpd.serve_forever()
