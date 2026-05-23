#![cfg(target_os = "linux")]

use std::path::PathBuf;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn workspace_arboard_dependency_enables_wayland_data_control() {
    let root_manifest = manifest_dir().join("..").join("Cargo.toml");
    let manifest: toml::Value = toml::from_str(
        &std::fs::read_to_string(&root_manifest)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", root_manifest.display())),
    )
        .unwrap_or_else(|err| panic!("failed to parse {}: {err}", root_manifest.display()));

    let features = manifest
        .get("workspace")
        .and_then(|value| value.get("dependencies"))
        .and_then(|value| value.get("arboard"))
        .and_then(|value| value.get("features"))
        .and_then(toml::Value::as_array)
        .expect("workspace.dependencies.arboard.features should exist");

    assert!(
        features.iter().any(|feature| feature.as_str() == Some("wayland-data-control")),
        "workspace arboard features missing wayland-data-control: {features:?}"
    );
}

#[test]
fn tui_manifest_uses_workspace_arboard_dependency() {
    let tui_manifest = manifest_dir().join("Cargo.toml");
    let manifest: toml::Value = toml::from_str(
        &std::fs::read_to_string(&tui_manifest)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", tui_manifest.display())),
    )
        .unwrap_or_else(|err| panic!("failed to parse {}: {err}", tui_manifest.display()));

    let uses_workspace = manifest
        .get("dependencies")
        .and_then(|value| value.get("arboard"))
        .and_then(|value| value.get("workspace"))
        .and_then(toml::Value::as_bool)
        .unwrap_or(false);

    assert!(uses_workspace, "code-tui should inherit arboard from workspace dependencies");
}

#[test]
fn cargo_lock_arboard_entry_depends_on_wayland_clipboard_backend() {
    let cargo_lock = manifest_dir().join("..").join("Cargo.lock");
    let lockfile: toml::Value = toml::from_str(
        &std::fs::read_to_string(&cargo_lock)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", cargo_lock.display())),
    )
        .unwrap_or_else(|err| panic!("failed to parse {}: {err}", cargo_lock.display()));

    let packages = lockfile
        .get("package")
        .and_then(toml::Value::as_array)
        .expect("Cargo.lock package list should exist");

    let arboard = packages
        .iter()
        .find(|package| package.get("name").and_then(toml::Value::as_str) == Some("arboard"))
        .and_then(toml::Value::as_table)
        .expect("Cargo.lock should contain an arboard entry");

    let dependencies = arboard
        .get("dependencies")
        .and_then(toml::Value::as_array)
        .expect("Cargo.lock arboard entry should list dependencies");

    assert!(
        dependencies.iter().any(|dependency| dependency.as_str() == Some("wl-clipboard-rs")),
        "Cargo.lock arboard entry should include wl-clipboard-rs when Wayland clipboard support is enabled"
    );
}
