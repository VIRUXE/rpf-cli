/// Glob-like match: `*` matches any run of characters (including `/`); a pattern
/// without `*` is a substring match.
pub fn matches_pattern(path: &str, pattern: &str) -> bool {
    if !pattern.contains('*') {
        return path.contains(pattern);
    }

    let parts: Vec<&str> = pattern.split('*').collect();
    let (first, last) = (parts[0], parts[parts.len() - 1]);

    if !path.starts_with(first) { return false; }
    let mut rest = &path[first.len()..];

    for part in &parts[1..parts.len() - 1] {
        if part.is_empty() { continue; }
        match rest.find(part) {
            Some(i) => rest = &rest[i + part.len()..],
            None => return false,
        }
    }

    rest.ends_with(last)
}

#[cfg(test)]
mod tests {
    use super::matches_pattern;

    #[test]
    fn single_star() {
        assert!(matches_pattern("a/b/c.ydr", "*.ydr"));
        assert!(matches_pattern("a/b/c.ydr", "a/*"));
        assert!(matches_pattern("a/b/c.ydr", "a/*.ydr"));
        assert!(!matches_pattern("a/b/c.ytd", "*.ydr"));
        assert!(!matches_pattern("x/b/c.ydr", "a/*"));
    }

    #[test]
    fn multiple_stars_match_in_order() {
        assert!(matches_pattern("x64b.rpf/levels/icons.rpf/prop.ydr", "*icons.rpf/*.ydr"));
        assert!(!matches_pattern("x64b.rpf/levels/icons.rpf/prop.ytd", "*icons.rpf/*.ydr"));
        assert!(!matches_pattern("x64b.rpf/levels/icons.rpf", "*icons.rpf/*"));
        assert!(matches_pattern("a/b/c/d", "a*c*d"));
        assert!(!matches_pattern("a/d/c", "a*c*d"));
        assert!(matches_pattern("anything", "*"));
        assert!(matches_pattern("abc", "a**c"));
    }

    #[test]
    fn no_star_is_substring() {
        assert!(matches_pattern("levels/inner.rpf/x", "inner.rpf"));
        assert!(!matches_pattern("levels/other.rpf", "inner.rpf"));
    }
}
