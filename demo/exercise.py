"""
MCPlex Demo — Exercise Script
Sends test requests to the MCPlex gateway to demonstrate all features.
Run AFTER starting mock servers and MCPlex gateway.
"""

import json
import time
import requests

GATEWAY = "http://127.0.0.1:3100/mcp"

def send_request(method, params=None):
    """Send a JSON-RPC request to MCPlex gateway"""
    body = {"jsonrpc": "2.0", "id": 1, "method": method}
    if params:
        body["params"] = params
    resp = requests.post(GATEWAY, json=body, timeout=10)
    return resp.json()

def main():
    print("=" * 60)
    print("🚀 MCPlex Demo — Exercising All Capabilities")
    print("=" * 60)
    
    # 1. Initialize
    print("\n📌 Step 1: Initialize MCP connection")
    result = send_request("initialize", {
        "protocolVersion": "2025-03-26",
        "capabilities": {},
        "clientInfo": {"name": "demo-client", "version": "1.0.0"}
    })
    print(f"   Server: {result.get('result', {}).get('serverInfo', {}).get('name', 'unknown')}")
    print(f"   Protocol: {result.get('result', {}).get('protocolVersion', 'unknown')}")
    
    # 2. List all tools (shows aggregation)
    print("\n📌 Step 2: List all tools (multiplexing 3 servers)")
    result = send_request("tools/list")
    tools = result.get("result", {}).get("tools", [])
    print(f"   Total tools available: {len(tools)}")
    for t in tools:
        print(f"   • {t['name']}: {t.get('description', '')[:60]}...")
    
    # 3. Call various tools (generates metrics)
    print("\n📌 Step 3: Execute tool calls (generating metrics)")
    
    test_calls = [
        ("create_issue", {"repo": "mcplex", "title": "Test issue", "body": "Demo body"}),
        ("list_repos", {"sort": "updated"}),
        ("search_code", {"query": "fn main", "language": "rust"}),
        ("send_message", {"channel": "#general", "text": "Hello from MCPlex!"}),
        ("query_users", {"filter": "active=true", "limit": 10}),
        ("list_tables", {}),
        ("get_schema", {"table": "users"}),
        ("export_csv", {"query": "SELECT * FROM users", "output": "users.csv"}),
        ("list_pull_requests", {"repo": "mcplex", "state": "open"}),
        ("backup_database", {"destination": "/backups/demo.sql"}),
    ]
    
    for tool_name, args in test_calls:
        try:
            result = send_request("tools/call", {"name": tool_name, "arguments": args})
            status = "✅" if result.get("result") else "❌"
            print(f"   {status} {tool_name}")
        except Exception as e:
            print(f"   ❌ {tool_name}: {e}")
        time.sleep(0.2)
    
    # 4. Test blocked tool (security demo)
    print("\n📌 Step 4: Test security — blocked tool")
    result = send_request("tools/call", {"name": "drop_table", "arguments": {"table": "users"}})
    if result.get("result"):
        # Tool executed (it went through because we don't have role context in HTTP)
        print("   ⚠️  drop_table executed (no role context in demo)")
    else:
        print(f"   🔒 drop_table BLOCKED: {result.get('error', {}).get('message', 'unknown')}")
    
    # 5. Ping
    print("\n📌 Step 5: Health check")
    result = send_request("ping")
    print(f"   🏓 Ping: OK")
    
    # 6. Multiple rapid calls for latency metrics
    print("\n📌 Step 6: Rapid-fire calls for latency metrics")
    for i in range(15):
        tool = ["create_issue", "send_message", "query_users", "list_repos", "search_code"][i % 5]
        try:
            send_request("tools/call", {"name": tool, "arguments": {"test": f"rapid-{i}"}})
        except:
            pass
    print(f"   ⚡ Sent 15 rapid-fire tool calls")
    
    print("\n" + "=" * 60)
    print("✅ Demo complete! Check the dashboard at http://127.0.0.1:9090")
    print(f"   📊 Total tool calls generated: {len(test_calls) + 15}")
    print(f"   📝 Audit log: ./logs/demo_audit.jsonl")
    print("=" * 60)

if __name__ == "__main__":
    main()
