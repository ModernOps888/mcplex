// MCPlex — Role-Based Access Control
// Maps roles to tool permissions using glob patterns

use std::collections::HashMap;
use crate::config::RoleConfig;

/// RBAC engine for tool access control
pub struct RbacEngine {
    roles: HashMap<String, RoleConfig>,
}

impl RbacEngine {
    pub fn new(roles: &HashMap<String, RoleConfig>) -> Self {
        Self {
            roles: roles.clone(),
        }
    }

    /// Check if a role is allowed to access a tool
    pub fn is_allowed(&self, role: &str, tool_fqn: &str) -> bool {
        if let Some(role_config) = self.roles.get(role) {
            // Check blocklist first (takes precedence)
            for pattern in &role_config.blocked_tools {
                if glob_match(pattern, tool_fqn) {
                    return false;
                }
            }

            // If allowlist is empty, allow everything
            if role_config.allowed_tools.is_empty() {
                return true;
            }

            // Check allowlist
            for pattern in &role_config.allowed_tools {
                if glob_match(pattern, tool_fqn) {
                    return true;
                }
            }

            false
        } else {
            // Unknown role — deny by default
            false
        }
    }

    /// List all roles
    pub fn list_roles(&self) -> Vec<String> {
        self.roles.keys().cloned().collect()
    }
}

/// Simple glob pattern matching
/// Supports * (any characters) and ? (single character)
fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();

    glob_match_recursive(&pattern_chars, &text_chars, 0, 0)
}

fn glob_match_recursive(
    pattern: &[char],
    text: &[char],
    pi: usize,
    ti: usize,
) -> bool {
    if pi == pattern.len() && ti == text.len() {
        return true;
    }
    if pi == pattern.len() {
        return false;
    }

    if pattern[pi] == '*' {
        // Match zero or more characters
        // Try matching rest of pattern from current position onward
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

    #[test]
    fn test_glob_match() {
        assert!(glob_match("*", "anything"));
        assert!(glob_match("github/*", "github/create_issue"));
        assert!(glob_match("github/*", "github/list_repos"));
        assert!(!glob_match("github/*", "slack/send_message"));
        assert!(glob_match("*/query_*", "database/query_users"));
        assert!(glob_match("?ello", "hello"));
        assert!(!glob_match("?ello", "jello_world"));
        assert!(glob_match("delete_*", "delete_users"));
        assert!(glob_match("delete_*", "delete_everything"));
    }

    #[test]
    fn test_rbac() {
        let mut roles = HashMap::new();
        roles.insert("developer".to_string(), RoleConfig {
            allowed_tools: vec!["github/*".to_string(), "database/query_*".to_string()],
            blocked_tools: vec![],
        });
        roles.insert("admin".to_string(), RoleConfig {
            allowed_tools: vec!["*".to_string()],
            blocked_tools: vec![],
        });
        roles.insert("readonly".to_string(), RoleConfig {
            allowed_tools: vec!["*/list_*".to_string(), "*/get_*".to_string()],
            blocked_tools: vec!["*/delete_*".to_string()],
        });

        let engine = RbacEngine::new(&roles);

        // Developer role
        assert!(engine.is_allowed("developer", "github/create_issue"));
        assert!(engine.is_allowed("developer", "database/query_users"));
        assert!(!engine.is_allowed("developer", "slack/send_message"));

        // Admin role
        assert!(engine.is_allowed("admin", "anything/at_all"));

        // Readonly role
        assert!(engine.is_allowed("readonly", "github/list_repos"));
        assert!(engine.is_allowed("readonly", "database/get_user"));
        assert!(!engine.is_allowed("readonly", "database/delete_user"));

        // Unknown role
        assert!(!engine.is_allowed("unknown", "anything"));
    }
}
