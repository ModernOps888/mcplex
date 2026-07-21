// MCPlex — Security Engine
// RBAC, audit logging, and tool allowlist/blocklist enforcement

pub mod allowlist;
pub mod audit;
pub mod rbac;

use crate::config::AppConfig;
use crate::protocol::ToolCallParams;

/// Combined security engine
pub struct SecurityEngine {
    rbac: rbac::RbacEngine,
    audit: audit::AuditLogger,
    allowlist: allowlist::AllowlistEngine,
    rbac_enabled: bool,
    audit_enabled: bool,
}

impl SecurityEngine {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            rbac: rbac::RbacEngine::new(&config.roles),
            audit: audit::AuditLogger::with_max_size(
                &config.security.audit_log_path,
                config.security.enable_audit_log,
                config.security.max_log_size_mb,
            ),
            allowlist: allowlist::AllowlistEngine::new(&config.servers),
            rbac_enabled: config.security.enable_rbac,
            audit_enabled: config.security.enable_audit_log,
        }
    }

    /// Check if a tool is allowed for a given role
    pub fn is_tool_allowed(&self, tool_fqn: &str, role: Option<&str>) -> bool {
        // Check allowlist first
        if !self.allowlist.is_allowed(tool_fqn) {
            return false;
        }

        // Check RBAC if enabled
        if self.rbac_enabled {
            if let Some(role) = role {
                return self.rbac.is_allowed(role, tool_fqn);
            }
            // v0.4.0: RBAC enabled but no role provided — deny by default
            // for security. Clients must provide a valid role via API key
            // mapping or the request will be rejected.
            return false;
        }

        true
    }

    /// Record an audit log entry for a tool call
    pub fn audit_tool_call(
        &self,
        tool_name: &str,
        server_name: &str,
        params: &ToolCallParams,
        duration_ms: u64,
    ) {
        if self.audit_enabled {
            self.audit
                .log_tool_call(tool_name, server_name, params, duration_ms);
        }
    }

    /// Record an audit log entry for a blocked call
    pub fn audit_blocked_call(&self, tool_name: &str, reason: &str) {
        if self.audit_enabled {
            self.audit.log_blocked_call(tool_name, reason);
        }
    }

    /// Record an audit log entry for a resource read or prompt fetch
    pub fn audit_access(&self, event: &str, target: &str, duration_ms: u64) {
        if self.audit_enabled {
            self.audit.log_access(event, target, duration_ms);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RoleConfig;

    fn rbac_config_with_admin_role() -> AppConfig {
        let mut config: AppConfig =
            toml::from_str("[gateway]\n").expect("minimal config with defaults must parse");
        config.security.enable_rbac = true;
        config.roles.insert(
            "admin".to_string(),
            RoleConfig {
                allowed_tools: vec!["*".to_string()],
                blocked_tools: vec![],
            },
        );
        config
    }

    /// Regression test: RBAC must deny when no server-verified role is
    /// established, even though an "admin" (allow-all) role exists in
    /// config. This guards the fix that removed the client-controlled
    /// `_mcplex_role` params fallback in `dispatch_real_tool` — a client
    /// could previously self-assert `role: "admin"` and bypass RBAC
    /// whenever no api_key/api_keys were configured.
    #[test]
    fn rbac_denies_when_no_trusted_role_established() {
        let config = rbac_config_with_admin_role();
        let engine = SecurityEngine::new(&config);
        assert!(!engine.is_tool_allowed("some_tool", None));
    }

    #[test]
    fn rbac_allows_with_a_trusted_role() {
        let config = rbac_config_with_admin_role();
        let engine = SecurityEngine::new(&config);
        assert!(engine.is_tool_allowed("some_tool", Some("admin")));
    }
}
