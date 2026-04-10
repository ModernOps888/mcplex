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
            // If RBAC is enabled but no role provided, use default behavior
            // (allow for now — in production you'd want to deny)
            return true;
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
}
