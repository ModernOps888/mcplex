// MCPlex — Shared Utilities
// Common functions used across the codebase to avoid duplication.
// v0.4.0: Extracted from rbac.rs and allowlist.rs to eliminate code duplication.

/// Match a pattern against a string using glob-style matching.
/// Supports `*` (match any sequence of characters) and `?` (match a single character).
pub fn glob_match(pattern: &str, text: &str) -> bool {
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
        // Match zero or more characters
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

/// Generate ISO 8601 timestamp without chrono dependency.
/// v0.4.0: Extracted from metrics.rs and audit.rs to eliminate duplication.
pub fn now_iso8601() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    // Rough UTC breakdown (not accounting for leap seconds, but good enough for logging)
    let days = secs / 86400;
    let remaining_secs = secs % 86400;
    let hours = remaining_secs / 3600;
    let minutes = (remaining_secs % 3600) / 60;
    let seconds = remaining_secs % 60;

    let (year, month, day) = days_to_ymd(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

/// Convert days since Unix epoch (1970-01-01) to (year, month, day).
pub fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    let mut y = 1970;
    let mut remaining = days;

    loop {
        let days_in_year = if is_leap_year(y) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }

    let days_in_months = if is_leap_year(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut m = 0;
    for (i, &dim) in days_in_months.iter().enumerate() {
        if remaining < dim {
            m = i + 1;
            break;
        }
        remaining -= dim;
    }

    (y, m as u64, remaining + 1)
}

/// Check if a year is a leap year.
pub fn is_leap_year(y: u64) -> bool {
    (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_match_patterns() {
        assert!(glob_match("*", "anything"));
        assert!(glob_match("github/*", "github/create_issue"));
        assert!(glob_match("github/*", "github/list_repos"));
        assert!(!glob_match("github/*", "slack/send_message"));
        assert!(glob_match("*/query_*", "database/query_users"));
        assert!(glob_match("?ello", "hello"));
        assert!(!glob_match("?ello", "jello_world"));
        assert!(glob_match("delete_*", "delete_users"));
    }

    #[test]
    fn test_iso8601_format() {
        let ts = now_iso8601();
        // Should match format YYYY-MM-DDTHH:MM:SSZ
        assert!(ts.ends_with('Z'));
        assert_eq!(ts.len(), 20);
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[7..8], "-");
        assert_eq!(&ts[10..11], "T");
    }

    #[test]
    fn test_leap_year() {
        assert!(is_leap_year(2000));
        assert!(is_leap_year(2024));
        assert!(!is_leap_year(1900));
        assert!(!is_leap_year(2023));
    }

    #[test]
    fn test_days_to_ymd_epoch() {
        // Day 0 = 1970-01-01
        assert_eq!(days_to_ymd(0), (1970, 1, 1));
    }
}
