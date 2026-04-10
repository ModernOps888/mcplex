// MCPlex — Tool Response Cache
// Caches idempotent tool responses to reduce upstream calls and latency

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};
use tracing::debug;

/// A time-based cache for tool call responses
pub struct ToolCache {
    entries: RwLock<HashMap<String, CacheEntry>>,
    /// Default TTL for cached responses
    default_ttl: Duration,
    /// Maximum number of cached entries
    max_entries: usize,
    /// Tools that should be cached (glob patterns)
    cacheable_patterns: Vec<String>,
}

struct CacheEntry {
    value: serde_json::Value,
    inserted_at: Instant,
    ttl: Duration,
    hits: u64,
}

impl CacheEntry {
    fn is_expired(&self) -> bool {
        self.inserted_at.elapsed() > self.ttl
    }
}

impl ToolCache {
    pub fn new(ttl_seconds: u64, max_entries: usize, patterns: Vec<String>) -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            default_ttl: Duration::from_secs(ttl_seconds),
            max_entries,
            cacheable_patterns: patterns,
        }
    }

    /// Check if a tool call result is cached and still valid
    pub fn get(
        &self,
        tool_name: &str,
        arguments: &Option<serde_json::Value>,
    ) -> Option<serde_json::Value> {
        if !self.is_cacheable(tool_name) {
            return None;
        }

        let key = Self::cache_key(tool_name, arguments);

        if let Ok(mut entries) = self.entries.write() {
            if let Some(entry) = entries.get_mut(&key) {
                if !entry.is_expired() {
                    entry.hits += 1;
                    debug!("📦 Cache HIT: {} ({}x)", tool_name, entry.hits);
                    return Some(entry.value.clone());
                } else {
                    // Remove expired entry
                    entries.remove(&key);
                }
            }
        }

        None
    }

    /// Store a tool call result in the cache
    pub fn put(
        &self,
        tool_name: &str,
        arguments: &Option<serde_json::Value>,
        value: serde_json::Value,
    ) {
        if !self.is_cacheable(tool_name) {
            return;
        }

        let key = Self::cache_key(tool_name, arguments);

        if let Ok(mut entries) = self.entries.write() {
            // Evict if at capacity — remove oldest expired entries first
            if entries.len() >= self.max_entries {
                let expired_keys: Vec<String> = entries
                    .iter()
                    .filter(|(_, v)| v.is_expired())
                    .map(|(k, _)| k.clone())
                    .collect();
                for key in expired_keys {
                    entries.remove(&key);
                }

                // If still at capacity, remove oldest entry
                if entries.len() >= self.max_entries {
                    if let Some(oldest_key) = entries
                        .iter()
                        .min_by_key(|(_, v)| v.inserted_at)
                        .map(|(k, _)| k.clone())
                    {
                        entries.remove(&oldest_key);
                    }
                }
            }

            entries.insert(
                key,
                CacheEntry {
                    value,
                    inserted_at: Instant::now(),
                    ttl: self.default_ttl,
                    hits: 0,
                },
            );
        }
    }

    /// Invalidate all cache entries for a specific tool
    pub fn invalidate(&self, tool_name: &str) {
        if let Ok(mut entries) = self.entries.write() {
            let prefix = format!("{}:", tool_name);
            entries.retain(|k, _| !k.starts_with(&prefix));
        }
    }

    /// Invalidate the entire cache
    pub fn invalidate_all(&self) {
        if let Ok(mut entries) = self.entries.write() {
            entries.clear();
        }
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        if let Ok(entries) = self.entries.read() {
            let total = entries.len();
            let expired = entries.values().filter(|e| e.is_expired()).count();
            let total_hits: u64 = entries.values().map(|e| e.hits).sum();
            CacheStats {
                total_entries: total,
                expired_entries: expired,
                total_hits,
            }
        } else {
            CacheStats {
                total_entries: 0,
                expired_entries: 0,
                total_hits: 0,
            }
        }
    }

    /// Check if a tool should be cached based on patterns
    fn is_cacheable(&self, tool_name: &str) -> bool {
        if self.cacheable_patterns.is_empty() {
            // Default: cache read-only tools (list_*, get_*, search_*, query_*, describe_*, show_*)
            let read_prefixes = [
                "list_",
                "get_",
                "search_",
                "query_",
                "describe_",
                "show_",
                "count_",
                "check_",
            ];
            return read_prefixes.iter().any(|p| tool_name.starts_with(p));
        }

        self.cacheable_patterns.iter().any(|pattern| {
            glob::Pattern::new(pattern)
                .map(|p| p.matches(tool_name))
                .unwrap_or(false)
        })
    }

    /// Generate a deterministic cache key from tool name + arguments
    fn cache_key(tool_name: &str, arguments: &Option<serde_json::Value>) -> String {
        match arguments {
            Some(args) => format!(
                "{}:{}",
                tool_name,
                serde_json::to_string(args).unwrap_or_default()
            ),
            None => format!("{}:()", tool_name),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_entries: usize,
    pub expired_entries: usize,
    pub total_hits: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_hit_miss() {
        let cache = ToolCache::new(60, 100, vec![]);

        // list_ prefix is auto-cacheable
        let args = Some(serde_json::json!({"filter": "active"}));

        // Miss
        assert!(cache.get("list_tables", &args).is_none());

        // Put
        let result = serde_json::json!({"tables": ["users", "orders"]});
        cache.put("list_tables", &args, result.clone());

        // Hit
        let cached = cache.get("list_tables", &args);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap(), result);
    }

    #[test]
    fn test_non_cacheable_tools() {
        let cache = ToolCache::new(60, 100, vec![]);

        // create_ prefix is NOT auto-cacheable
        let args = Some(serde_json::json!({"name": "test"}));
        let result = serde_json::json!({"id": 1});

        cache.put("create_issue", &args, result);
        assert!(cache.get("create_issue", &args).is_none());
    }

    #[test]
    fn test_cache_expiration() {
        let cache = ToolCache::new(0, 100, vec![]); // 0 second TTL

        let args = None;
        cache.put("list_tables", &args, serde_json::json!({}));

        // Should be expired immediately
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(cache.get("list_tables", &args).is_none());
    }

    #[test]
    fn test_custom_patterns() {
        let cache = ToolCache::new(60, 100, vec!["my_tool".to_string(), "custom_*".to_string()]);

        let args = None;
        let val = serde_json::json!("ok");

        // Exact match
        cache.put("my_tool", &args, val.clone());
        assert!(cache.get("my_tool", &args).is_some());

        // Glob match
        cache.put("custom_query", &args, val.clone());
        assert!(cache.get("custom_query", &args).is_some());

        // Non-match (list_ wouldn't match with custom patterns set)
        cache.put("list_tables", &args, val);
        assert!(cache.get("list_tables", &args).is_none());
    }

    #[test]
    fn test_invalidation() {
        let cache = ToolCache::new(60, 100, vec![]);

        cache.put("list_tables", &None, serde_json::json!({"a": 1}));
        cache.put("list_repos", &None, serde_json::json!({"b": 2}));

        cache.invalidate("list_tables");
        assert!(cache.get("list_tables", &None).is_none());
        assert!(cache.get("list_repos", &None).is_some());
    }
}
