use std::collections::HashMap;
use std::sync::LazyLock;

use serde_json::Value;

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
const MODELS_MANIFEST: &str = include_str!("../../../codex-rs/models-manager/models.json");
pub const MIN_WIRE_COMPAT_VERSION_FALLBACK: &str = "0.101.0";

static MIN_WIRE_COMPAT_VERSION: LazyLock<String> = LazyLock::new(|| {
    let mut minimum = MIN_WIRE_COMPAT_VERSION_FALLBACK.to_string();

    if let Some(extracted) = extract_max_semver(ANNOUNCEMENT_TIP) {
        minimum = max_semver(&minimum, extracted).to_string();
    }

    minimum
});

static MODEL_MINIMUM_CLIENT_VERSIONS: LazyLock<HashMap<String, String>> =
    LazyLock::new(|| parse_model_minimum_client_versions(MODELS_MANIFEST));

fn max_semver<'a>(current: &'a str, candidate: &'a str) -> &'a str {
    let Some(current_triplet) = parse_semver_triplet(current) else {
        return candidate;
    };
    let Some(candidate_triplet) = parse_semver_triplet(candidate) else {
        return current;
    };

    if candidate_triplet > current_triplet {
        candidate
    } else {
        current
    }
}

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
            !(ch.is_ascii_alphanumeric() || matches!(ch, '-' | '+' | 'v'))
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

fn parse_model_minimum_client_versions(input: &str) -> HashMap<String, String> {
    let Ok(root) = serde_json::from_str::<Value>(input) else {
        return HashMap::new();
    };
    let Some(models) = root.get("models").and_then(Value::as_array) else {
        return HashMap::new();
    };
    let mut versions = HashMap::new();

    for model in models {
        let Some(slug) = model.get("slug").and_then(Value::as_str) else {
            continue;
        };
        let Some(candidate) = model.get("minimal_client_version").and_then(Value::as_str) else {
            continue;
        };

        if parse_semver_triplet(candidate).is_none() {
            continue;
        };

        versions.insert(slug.to_ascii_lowercase(), candidate.to_string());
    }

    versions
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
    MIN_WIRE_COMPAT_VERSION.as_str()
}

#[inline]
pub fn wire_compatible_version() -> &'static str {
    wire_compatible_version_for(CODE_VERSION, min_wire_compat_version())
}

pub fn wire_compatible_version_for_model(model: &str) -> String {
    let canonical_model = model.rsplit('/').next().unwrap_or(model).trim();
    let Some(required_version) = MODEL_MINIMUM_CLIENT_VERSIONS
        .get(&canonical_model.to_ascii_lowercase())
    else {
        return wire_compatible_version().to_string();
    };

    max_semver(wire_compatible_version(), required_version).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_compat_clamps_old_versions() {
        assert_eq!(
            wire_compatible_version_for("0.0.0", "0.101.0"),
            "0.101.0"
        );
        assert_eq!(
            wire_compatible_version_for("0.6.59", "0.101.0"),
            "0.101.0"
        );
        assert_eq!(
            wire_compatible_version_for("0.6.59-dev+abc123", "0.101.0"),
            "0.101.0"
        );
    }

    #[test]
    fn wire_compat_keeps_new_versions() {
        assert_eq!(
            wire_compatible_version_for("0.101.0", "0.101.0"),
            "0.101.0"
        );
        assert_eq!(
            wire_compatible_version_for("0.101.1", "0.101.0"),
            "0.101.1"
        );
        assert_eq!(wire_compatible_version_for("1.0.0", "0.101.0"), "1.0.0");
    }

    #[test]
    fn wire_compat_keeps_invalid_versions() {
        assert_eq!(wire_compatible_version_for("dev", "0.101.0"), "dev");
        assert_eq!(wire_compatible_version_for("0.1", "0.101.0"), "0.1");
    }

    #[test]
    fn extract_max_semver_picks_highest_semver() {
        let input = "v0.98.0 preview and later 0.102.1 with regex ^0\\.0\\.0$";
        assert_eq!(extract_max_semver(input), Some("0.102.1"));
    }

    #[test]
    fn extract_max_semver_strips_sentence_punctuation() {
        let input = "Upgrade to 0.99.0. Then 0.98.1.";
        assert_eq!(extract_max_semver(input), Some("0.99.0"));
    }

    #[test]
    fn configured_minimum_defaults_to_semver() {
        assert!(parse_semver_triplet(min_wire_compat_version()).is_some());
    }

    #[test]
    fn configured_minimum_is_at_least_fallback() {
        let configured = parse_semver_triplet(min_wire_compat_version()).expect("configured semver");
        let fallback =
            parse_semver_triplet(MIN_WIRE_COMPAT_VERSION_FALLBACK).expect("fallback semver");
        assert!(configured >= fallback);
    }

    #[test]
    fn parse_model_minimum_client_versions_extracts_versions() {
        let input = r#"{
            "models": [
                {"slug": "gpt-5.4", "minimal_client_version": "0.98.0"},
                {"slug": "gpt-5.5", "minimal_client_version": "0.124.0"},
                {"slug": "legacy", "minimal_client_version": "0.0.1"}
            ]
        }"#;

        assert_eq!(
            parse_model_minimum_client_versions(input).get("gpt-5.5"),
            Some(&"0.124.0".to_string())
        );
    }

    #[test]
    fn wire_compatible_version_for_model_raises_for_gpt_5_5() {
        assert_eq!(wire_compatible_version_for_model("gpt-5.5"), "0.124.0");
    }

    #[test]
    fn wire_compatible_version_for_model_uses_base_version_for_unknown_models() {
        assert_eq!(
            wire_compatible_version_for_model("unknown-model"),
            wire_compatible_version()
        );
    }

    #[test]
    fn parse_semver_triplet_rejects_four_component_versions() {
        assert_eq!(parse_semver_triplet("1.2.3.4"), None);
    }
}
