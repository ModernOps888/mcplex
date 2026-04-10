// MCPlex — Allowlist/Blocklist Engine
// Per-server tool filtering based on configuration

use crate::config::ServerConfig;

/// Engine for enforcing tool allow/block lists per server
pub struct AllowlistEngine {
    rules: Vec<ServerToolRule>,
}

struct ServerToolRule {
    server_name: String,
    allowed_tools: Vec<String>,
    blocked_tools: Vec<String>,
}

impl AllowlistEngine {
    pub fn new(servers: &[ServerConfig]) -> Self {
        let rules = servers
            .iter()
            .map(|s| ServerToolRule {
                server_name: s.name.clone(),
                allowed_tools: s.allowed_tools.clone(),
                blocked_tools: s.blocked_tools.clone(),
            })
            .collect();

        Self { rules }
    }

    /// Check if a tool is allowed by the allowlist/blocklist rules
    pub fn is_allowed(&self, tool_fqn: &str) -> bool {
        // Parse the FQN into server/tool parts
        let (server, tool) = if let Some(pos) = tool_fqn.find('/') {
            (&tool_fqn[..pos], &tool_fqn[pos + 1..])
        } else {
            // If no server prefix, check tool name against all rules
            ("", tool_fqn)
        };

        for rule in &self.rules {
            // Only check rules for the matching server
            if !server.is_empty() && rule.server_name != server {
                continue;
            }

            // Check blocklist first (takes precedence)
            for pattern in &rule.blocked_tools {
                if glob_match(pattern, tool) || glob_match(pattern, tool_fqn) {
                    return false;
                }
            }

            // If allowlist is defined, tool must match at least one pattern
            if !rule.allowed_tools.is_empty() {
                let allowed = rule
                    .allowed_tools
                    .iter()
                    .any(|pattern| glob_match(pattern, tool) || glob_match(pattern, tool_fqn));
                if !allowed && (server == rule.server_name || server.is_empty()) {
                    return false;
                }
            }
        }

        true
    }
}

/// Simple glob pattern matching (reused from rbac)
fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();
    glob_match_recursive(&pattern_chars, &text_chars, 0, 0)
}

fn glob_match_recursive(pattern: &[char], text: &[char], pi: usize, ti: usize) -> bool {
    if pi == pattern.len() && ti == text.len() {
        return true;
    }
    if pi == pattern.len() {
        return false;
    }
    if pattern[pi] == '*' {
        for i in ti..=text.len() {
            if glob_match_recursive(pattern, text, pi + 1, i) {
                return true;
            }
        }
        return false;
    }
    if ti == text.len() {
        return false;
    }
    if pattern[pi] == '?' || pattern[pi] == text[ti] {
        return glob_match_recursive(pattern, text, pi + 1, ti + 1);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ServerConfig;
    use std::collections::HashMap;

    #[test]
    fn test_allowlist_blocks_dangerous_tools() {
        let servers = vec![ServerConfig {
            name: "database".to_string(),
            command: None,
            args: vec![],
            url: Some("http://localhost:8080".to_string()),
            transport: crate::config::TransportType::Auto,
            env: HashMap::new(),
            allowed_roles: vec![],
            blocked_tools: vec!["drop_*".to_string(), "delete_*".to_string()],
            allowed_tools: vec![],
            enabled: true,
        }];

        let engine = AllowlistEngine::new(&servers);

        assert!(engine.is_allowed("database/query_users"));
        assert!(engine.is_allowed("database/insert_record"));
        assert!(!engine.is_allowed("database/drop_table"));
        assert!(!engine.is_allowed("database/delete_all"));
    }

    #[test]
    fn test_allowlist_only_permits_listed() {
        let servers = vec![ServerConfig {
            name: "github".to_string(),
            command: None,
            args: vec![],
            url: Some("http://localhost:8081".to_string()),
            transport: crate::config::TransportType::Auto,
            env: HashMap::new(),
            allowed_roles: vec![],
            blocked_tools: vec![],
            allowed_tools: vec!["list_*".to_string(), "get_*".to_string()],
            enabled: true,
        }];

        let engine = AllowlistEngine::new(&servers);

        assert!(engine.is_allowed("github/list_repos"));
        assert!(engine.is_allowed("github/get_issue"));
        assert!(!engine.is_allowed("github/delete_repo"));
        assert!(!engine.is_allowed("github/create_issue"));
    }
}
