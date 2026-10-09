//! Version comparison for the manual "Check for updates" menu item.

/// Parses "v0.2.1" or "0.2.1-beta" into (0, 2, 1). Pre-release suffixes are
/// ignored for ordering.
pub fn parse(tag: &str) -> Option<(u64, u64, u64)> {
    let core = tag.trim().trim_start_matches(['v', 'V']);
    let core = core.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>());
    let major = parts.next()?.ok()?;
    let minor = parts.next().unwrap_or(Ok(0)).ok()?;
    let patch = parts.next().unwrap_or(Ok(0)).ok()?;
    Some((major, minor, patch))
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse(candidate), parse(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_tags() {
        assert_eq!(parse("v0.2.0"), Some((0, 2, 0)));
        assert_eq!(parse("1.10"), Some((1, 10, 0)));
        assert_eq!(parse("0.3.0-beta.1"), Some((0, 3, 0)));
        assert_eq!(parse("latest"), None);
        assert!(is_newer("v0.2.1", "0.2.0"));
        assert!(is_newer("v0.10.0", "0.9.9"));
        assert!(!is_newer("v0.2.0", "0.2.0"));
        assert!(!is_newer("garbage", "0.2.0"));
    }
}
