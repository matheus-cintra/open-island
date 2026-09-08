pub mod cache;
pub mod release;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Version {
    major: u64,
    minor: u64,
    patch: u64,
    stable: bool,
}

pub fn is_newer(published_tag: &str, running: &str) -> bool {
    match (parse(published_tag), parse(running)) {
        (Some(published), Some(local)) => published > local,
        _ => false,
    }
}

fn parse(text: &str) -> Option<Version> {
    let text = text.trim().strip_prefix('v').unwrap_or(text.trim());
    let (core, suffix) = match text.split_once('-') {
        Some((core, suffix)) => (core, Some(suffix)),
        None => (text, None),
    };
    if suffix.is_some_and(str::is_empty) {
        return None;
    }
    let mut parts = core.split('.').map(number);
    let version = Version {
        major: parts.next()??,
        minor: parts.next()??,
        patch: parts.next()??,
        stable: suffix.is_none(),
    };
    parts.next().is_none().then_some(version)
}

fn number(text: &str) -> Option<u64> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_higher_minor_is_newer() {
        assert!(is_newer("v0.2.0", "0.1.0"));
    }

    #[test]
    fn the_same_version_is_not_newer() {
        assert!(!is_newer("v0.1.0", "0.1.0"));
    }

    #[test]
    fn an_older_patch_is_not_newer() {
        assert!(!is_newer("v0.1.9", "0.2.0"));
    }

    #[test]
    fn a_prerelease_of_the_running_version_is_older() {
        assert!(!is_newer("v0.1.0-rc1", "0.1.0"));
    }

    #[test]
    fn the_stable_release_of_a_running_prerelease_is_newer() {
        assert!(is_newer("v0.1.0", "0.1.0-rc1"));
    }

    #[test]
    fn the_comparison_is_numeric_not_lexicographic() {
        assert!(!is_newer("v0.9.0", "0.10.0"));
        assert!(is_newer("v0.10.0", "0.9.0"));
    }

    #[test]
    fn a_tag_that_is_not_a_version_is_never_newer() {
        assert!(!is_newer("nightly", "0.1.0"));
        assert!(!is_newer("v1.2", "0.1.0"));
        assert!(!is_newer("v1.2.3.4", "0.1.0"));
        assert!(!is_newer("v1.2.3-", "0.1.0"));
        assert!(!is_newer("", "0.1.0"));
        assert!(!is_newer("v9.9.9", "garbage"));
    }

    #[test]
    fn the_tag_may_come_without_the_leading_v() {
        assert!(is_newer("0.2.0", "0.1.0"));
    }
}
