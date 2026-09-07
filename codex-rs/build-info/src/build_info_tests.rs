use std::fs;

use codex_install_context::InstallContext;
use pretty_assertions::assert_eq;
use semver::Version;
use tempfile::tempdir;

use crate::BuildInfo;
use crate::build_id;

const BUILD_COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

/// A packaged runtime takes its release identity from its package manifest.
#[test]
fn packaged_runtime_uses_manifest_version() {
    let package = tempdir().expect("create runtime package");
    let bin_dir = package.path().join("bin");
    fs::create_dir(&bin_dir).expect("create runtime binary directory");
    let executable = bin_dir.join("codex");
    fs::write(&executable, b"").expect("create runtime binary");
    fs::write(
        package.path().join("codex-package.json"),
        r#"{"version":"1.2.3-alpha.4"}"#,
    )
    .expect("create runtime package manifest");

    let context = InstallContext::from_exe(
        cfg!(target_os = "macos"),
        Some(&executable),
        /*method_override*/ None,
    );

    assert_eq!(
        BuildInfo::resolve(&context, BUILD_COMMIT),
        BuildInfo {
            version: Version::parse("1.2.3-alpha.4").expect("valid release version"),
            build_commit: BUILD_COMMIT.to_string(),
            target: Some(env!("CODEX_BUILD_TARGET").to_string()),
        },
    );
}

/// Unpackaged builds expose their stamped commit and structured source version.
#[test]
fn unpackaged_runtime_uses_build_commit() {
    let context = InstallContext::from_exe(
        cfg!(target_os = "macos"),
        /*current_exe*/ None,
        /*method_override*/ None,
    );

    assert_eq!(
        BuildInfo::resolve(&context, BUILD_COMMIT),
        BuildInfo {
            version: Version::new(0, 0, 0),
            build_commit: BUILD_COMMIT.to_string(),
            target: Some(env!("CODEX_BUILD_TARGET").to_string()),
        },
    );
}

/// Older package layouts without release metadata retain their build identity.
#[test]
fn legacy_package_without_version_uses_build_commit() {
    let package = tempdir().expect("create runtime package");
    let bin_dir = package.path().join("bin");
    fs::create_dir(&bin_dir).expect("create runtime binary directory");
    let executable = bin_dir.join("codex");
    fs::write(&executable, b"").expect("create runtime binary");
    fs::write(package.path().join("codex-package.json"), "{}")
        .expect("create legacy runtime package manifest");

    let context = InstallContext::from_exe(
        cfg!(target_os = "macos"),
        Some(&executable),
        /*method_override*/ None,
    );

    assert_eq!(
        BuildInfo::resolve(&context, BUILD_COMMIT),
        BuildInfo {
            version: Version::new(0, 0, 0),
            build_commit: BUILD_COMMIT.to_string(),
            target: Some(env!("CODEX_BUILD_TARGET").to_string()),
        },
    );
}

/// Invalid package versions cannot override the executable's stamped commit.
#[test]
fn invalid_package_version_uses_build_commit() {
    let package = tempdir().expect("create runtime package");
    let bin_dir = package.path().join("bin");
    fs::create_dir(&bin_dir).expect("create runtime binary directory");
    let executable = bin_dir.join("codex");
    fs::write(&executable, b"").expect("create runtime binary");
    fs::write(
        package.path().join("codex-package.json"),
        r#"{"version":"not-a-release-version"}"#,
    )
    .expect("create runtime package manifest");

    let context = InstallContext::from_exe(
        cfg!(target_os = "macos"),
        Some(&executable),
        /*method_override*/ None,
    );

    assert_eq!(
        BuildInfo::resolve(&context, BUILD_COMMIT),
        BuildInfo {
            version: Version::new(0, 0, 0),
            build_commit: BUILD_COMMIT.to_string(),
            target: Some(env!("CODEX_BUILD_TARGET").to_string()),
        },
    );
}

/// Serializing build information preserves release version, commit, and target.
#[test]
fn build_info_serialization_preserves_build_provenance() {
    let build_info = BuildInfo {
        version: Version::parse("1.2.3-alpha.4").expect("valid release version"),
        build_commit: BUILD_COMMIT.to_string(),
        target: Some("x86_64-pc-windows-msvc".to_string()),
    };
    let serialized = serde_json::json!({
        "version": "1.2.3-alpha.4",
        "build_commit": BUILD_COMMIT,
        "target": "x86_64-pc-windows-msvc",
    });

    assert_eq!(
        serde_json::to_value(&build_info).expect("serialize build information"),
        serialized,
    );
    assert_eq!(
        serde_json::from_value::<BuildInfo>(serialized).expect("deserialize build information"),
        build_info,
    );
}

#[test]
fn historical_build_info_does_not_infer_the_current_target() {
    let legacy = serde_json::json!({ "version": "1.2.3", "build_commit": BUILD_COMMIT });
    for info in [
        serde_json::from_value::<BuildInfo>(legacy).expect("deserialize legacy build information"),
        BuildInfo::from_version("1.2.3"),
        BuildInfo::from_version(BUILD_COMMIT),
    ] {
        assert_eq!(info.target(), None);
    }
}

#[test]
fn build_id_is_stable_and_uses_the_supplied_commit_and_target() {
    // These vectors also cover targets different from the machine running CI.
    for (target, digest) in [
        (
            "aarch64-apple-darwin",
            "90138d7ee35f3f61cd61eb55e8d64105f856055641af175388b102dffa772594",
        ),
        (
            "x86_64-unknown-linux-gnu",
            "89f3a373036537bde64861b3ad8b1c9494924b0174622e47acda695619630d09",
        ),
        (
            "x86_64-unknown-linux-musl",
            "fb4f62da3e84f6864dcec8ede7bc66f1c96ecaeaf55f8a786b85df994057c8ac",
        ),
    ] {
        let expected = Some(format!("sha256:{digest}"));
        assert_eq!(build_id(BUILD_COMMIT, target), expected);
        assert_eq!(
            build_id(&BUILD_COMMIT.to_ascii_uppercase(), target),
            expected
        );
        assert_ne!(
            build_id("1123456789abcdef0123456789abcdef01234567", target),
            expected
        );
    }
}

#[test]
fn build_id_requires_a_valid_stamp_and_target() {
    for commit in [
        "",
        "dev",
        "unknown",
        "0123456",
        "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz",
    ] {
        assert_eq!(build_id(commit, "x86_64-unknown-linux-musl"), None);
    }
    assert_eq!(build_id(BUILD_COMMIT, ""), None);
}
