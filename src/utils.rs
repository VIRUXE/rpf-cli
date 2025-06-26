pub fn matches_pattern(path: &str, pattern: &str) -> bool {
    // Simple glob-like pattern matching supporting '*' wildcard.
    // Currently supports prefix, suffix, and infix patterns with a single '*'.
    // For anything more sophisticated, consider integrating the glob crate.
    if pattern.contains('*') {
        let parts: Vec<&str> = pattern.split('*').collect();

        match parts.as_slice() {
            // "*suffix" -> ends with suffix
            ["", suffix] => path.ends_with(suffix),
            // "prefix*" -> starts with prefix
            [prefix, ""] => path.starts_with(prefix),
            // "prefix*suffix" -> starts with prefix AND ends with suffix
            [prefix, suffix] => path.starts_with(prefix) && path.ends_with(suffix),
            // For patterns with multiple '*' we fall back to substring containment of all non-empty parts
            _ => {
                let pattern_no_star = pattern.replace('*', "");
                path.contains(&pattern_no_star)
            }
        }
    } else {
        // Exact match or substring match when no wildcard present
        path == pattern || path.contains(pattern)
    }
} 