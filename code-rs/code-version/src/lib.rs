use std::sync::LazyLock;

// Compile-time embedded version string.
// Prefer the CODE_VERSION provided by CI; fall back to the package
// version for local builds.
pub const CODE_VERSION: &str = {
    match option_env!("CODE_VERSION") {
        Some(v) => v,
        None => env!("CARGO_PKG_VERSION"),
    }
};

const ANNOUNCEMENT_TIP: &str = include_str!("../../../announcement_tip.toml");
pub const MIN_WIRE_COMPAT_VERSION_FALLBACK: &str = "0.98.0";

static MIN_WIRE_COMPAT_VERSION: LazyLock<&'static str> = LazyLock::new(|| {
    extract_max_semver(ANNOUNCEMENT_TIP).unwrap_or(MIN_WIRE_COMPAT_VERSION_FALLBACK)
});

fn parse_semver_triplet(version: &str) -> Option<(u64, u64, u64)> {
    let trimmed = version.trim().trim_start_matches('v');
    let core = trimmed
        .split_once(['-', '+'])
        .map_or(trimmed, |(v, _)| v);
    let mut parts = core.split('.');

    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;

    if parts.next().is_some() {
        return None;
    }

    Some((major, minor, patch))
}

fn extract_max_semver(input: &'static str) -> Option<&'static str> {
    let mut max: Option<((u64, u64, u64), &'static str)> = None;

    for token in input.split_whitespace() {
        let candidate = token.trim_matches(|ch: char| {
            !(ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '+' | 'v'))
        });
        if candidate.is_empty() {
            continue;
        }

        let Some(triplet) = parse_semver_triplet(candidate) else {
            continue;
        };

        let should_update = max.as_ref().is_none_or(|(current_max, _)| triplet > *current_max);
        if should_update {
            max = Some((triplet, candidate));
        }
    }

    max.map(|(_, version)| version)
}

fn wire_compatible_version_for<'a>(version: &'a str, minimum: &'a str) -> &'a str {
    let Some(version_triplet) = parse_semver_triplet(version) else {
        return version;
    };
    let Some(min_triplet) = parse_semver_triplet(minimum) else {
        return version;
    };

    if version_triplet < min_triplet {
        minimum
    } else {
        version
    }
}

#[inline]
pub fn version() -> &'static str {
    CODE_VERSION
}

#[inline]
pub fn min_wire_compat_version() -> &'static str {
    *MIN_WIRE_COMPAT_VERSION
}

#[inline]
pub fn wire_compatible_version() -> &'static str {
    wire_compatible_version_for(CODE_VERSION, min_wire_compat_version())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_compat_clamps_old_versions() {
        assert_eq!(
            wire_compatible_version_for("0.0.0", "0.98.0"),
            "0.98.0"
        );
        assert_eq!(
            wire_compatible_version_for("0.6.59", "0.98.0"),
            "0.98.0"
        );
        assert_eq!(
            wire_compatible_version_for("0.6.59-dev+abc123", "0.98.0"),
            "0.98.0"
        );
    }

    #[test]
    fn wire_compat_keeps_new_versions() {
        assert_eq!(wire_compatible_version_for("0.98.0", "0.98.0"), "0.98.0");
        assert_eq!(wire_compatible_version_for("0.99.1", "0.98.0"), "0.99.1");
        assert_eq!(wire_compatible_version_for("1.0.0", "0.98.0"), "1.0.0");
    }

    #[test]
    fn wire_compat_keeps_invalid_versions() {
        assert_eq!(wire_compatible_version_for("dev", "0.98.0"), "dev");
        assert_eq!(wire_compatible_version_for("0.1", "0.98.0"), "0.1");
    }

    #[test]
    fn extract_max_semver_picks_highest_semver() {
        let input = "v0.98.0 preview and later 0.102.1 with regex ^0\\.0\\.0$";
        assert_eq!(extract_max_semver(input), Some("0.102.1"));
    }

    #[test]
    fn configured_minimum_defaults_to_semver() {
        assert!(parse_semver_triplet(min_wire_compat_version()).is_some());
    }

    #[test]
    fn parse_semver_triplet_rejects_four_component_versions() {
        assert_eq!(parse_semver_triplet("1.2.3.4"), None);
    }
}
