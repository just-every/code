use pretty_assertions::assert_eq;

#[test]
fn discovers_package_and_legacy_installs() {
    let home = tempfile::TempDir::new().expect("home");
    let current = home.path().join("packages/standalone/current");
    let legacy = current.join(super::managed_codex_file_name());
    let expected = if cfg!(windows) {
        current.join("bin").join("codex.exe")
    } else {
        legacy.clone()
    };
    assert_eq!(super::managed_codex_bin(home.path()), expected);
    std::fs::create_dir_all(&current).expect("current directory");
    std::fs::write(&legacy, b"legacy").expect("legacy executable");
    assert_eq!(super::managed_codex_bin(home.path()), legacy);
    let packaged = current.join("bin").join(super::managed_codex_file_name());
    std::fs::create_dir(current.join("bin")).expect("bin directory");
    std::fs::write(&packaged, b"packaged").expect("packaged executable");
    assert_eq!(super::managed_codex_bin(home.path()), packaged);
}

#[cfg(unix)]
#[test]
fn updater_only_runs_for_stable_installer_owned_releases() {
    let home = tempfile::TempDir::new().expect("home");
    let standalone = home.path().join("packages/standalone");
    let current = standalone.join("current");
    let release = standalone.join("releases/0.150.0-aarch64-apple-darwin");
    let managed = release.join("bin/codex");
    std::fs::create_dir_all(managed.parent().expect("bin parent")).expect("release");
    std::fs::write(&managed, b"stable").expect("managed bin");
    std::os::unix::fs::symlink(&release, &current).expect("current release");
    assert!(!super::is_stable_standalone_release(home.path(), &managed));
    let marker = standalone.join("auto-update-version");
    let release_name = release.file_name().expect("release name");
    std::fs::write(&marker, release_name.as_encoded_bytes()).expect("latest selection");
    assert!(super::is_stable_standalone_release(home.path(), &managed));
    std::fs::write(&marker, b"0.149.0-aarch64-apple-darwin").expect("stale selection");
    assert!(!super::is_stable_standalone_release(home.path(), &managed));
    std::fs::remove_file(&marker).expect("pinned selection");
    assert!(!super::is_stable_standalone_release(home.path(), &managed));

    let alpha = standalone.join("releases/0.151.0-alpha.1-aarch64-apple-darwin");
    let alpha_managed = alpha.join("bin/codex");
    std::fs::create_dir_all(alpha_managed.parent().expect("alpha bin parent"))
        .expect("alpha release");
    std::fs::write(&alpha_managed, b"alpha").expect("alpha bin");
    std::fs::remove_file(&current).expect("remove current");
    std::os::unix::fs::symlink(alpha, &current).expect("current alpha");
    assert!(!super::is_stable_standalone_release(
        home.path(),
        &alpha_managed
    ));

    let local = standalone.join("local-main");
    let local_managed = local.join("bin/codex");
    std::fs::create_dir_all(local_managed.parent().expect("local bin parent"))
        .expect("local build");
    std::fs::write(&local_managed, b"local").expect("local bin");
    std::fs::remove_file(&current).expect("remove current");
    std::os::unix::fs::symlink(local, &current).expect("current local build");
    assert!(!super::is_stable_standalone_release(
        home.path(),
        &local_managed
    ));
}

#[cfg(unix)]
#[tokio::test]
async fn older_managed_binary_does_not_claim_updater_support() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::TempDir::new().expect("home");
    let binary = temp.path().join("codex");
    std::fs::write(&binary, b"#!/bin/sh\nexit 2\n").expect("older binary");
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755))
        .expect("executable binary");
    assert!(!super::supports_daemon_update_loop(&binary).await);
    std::fs::write(&binary, b"#!/bin/sh\nexit 0\n").expect("newer binary");
    assert!(super::supports_daemon_update_loop(&binary).await);
}
