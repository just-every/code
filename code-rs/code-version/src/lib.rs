// Compile-time embedded version string.
// Prefer the CODE_VERSION provided by CI; fall back to the package
// version for local builds.
pub const CODE_VERSION: &str = {
    match option_env!("CODE_VERSION") {
        Some(v) => v,
        None => env!("CARGO_PKG_VERSION"),
    }
};

pub const MIN_WIRE_COMPAT_VERSION: &str = "0.98.0";

fn parse_semver_triplet(version: &str) -> Option<(u64, u64, u64)> {
    let core = version.split_once(['-', '+']).map_or(version, |(v, _)| v);
    let mut parts = core.split('.');

    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;

    Some((major, minor, patch))
}

fn wire_compatible_version_for(version: &str) -> &str {
    let Some(version_triplet) = parse_semver_triplet(version) else {
        return version;
    };
    let Some(min_triplet) = parse_semver_triplet(MIN_WIRE_COMPAT_VERSION) else {
        return version;
    };

    if version_triplet < min_triplet {
        MIN_WIRE_COMPAT_VERSION
    } else {
        version
    }
}

#[inline]
pub fn version() -> &'static str {
    CODE_VERSION
}

#[inline]
pub fn wire_compatible_version() -> &'static str {
    wire_compatible_version_for(CODE_VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_compat_clamps_old_versions() {
        assert_eq!(wire_compatible_version_for("0.0.0"), MIN_WIRE_COMPAT_VERSION);
        assert_eq!(wire_compatible_version_for("0.6.59"), MIN_WIRE_COMPAT_VERSION);
        assert_eq!(
            wire_compatible_version_for("0.6.59-dev+abc123"),
            MIN_WIRE_COMPAT_VERSION
        );
    }

    #[test]
    fn wire_compat_keeps_new_versions() {
        assert_eq!(wire_compatible_version_for("0.98.0"), "0.98.0");
        assert_eq!(wire_compatible_version_for("0.99.1"), "0.99.1");
        assert_eq!(wire_compatible_version_for("1.0.0"), "1.0.0");
    }

    #[test]
    fn wire_compat_keeps_invalid_versions() {
        assert_eq!(wire_compatible_version_for("dev"), "dev");
        assert_eq!(wire_compatible_version_for("0.1"), "0.1");
    }
}
