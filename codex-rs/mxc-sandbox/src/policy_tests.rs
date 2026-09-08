//! Portable policy and launcher regressions, with Windows identity coverage.

use std::path::Path;
use std::path::PathBuf;

use anyhow::Result;
use codex_protocol::models::PermissionProfile;
use codex_protocol::protocol::FileSystemAccessMode;
use codex_protocol::protocol::FileSystemPath;
use codex_protocol::protocol::FileSystemSandboxEntry;
use codex_protocol::protocol::FileSystemSandboxPolicy;
use codex_protocol::protocol::NetworkSandboxPolicy;
use codex_utils_absolute_path::AbsolutePathBuf;
use codex_utils_path_uri::PathUri;
use pretty_assertions::assert_eq;

use crate::MxcCommand;
use crate::policy::build_request;

fn canonical_root(temp: &tempfile::TempDir) -> Result<PathBuf> {
    Ok(
        PathUri::from_host_native_path(std::fs::canonicalize(temp.path())?)?
            .to_abs_path()?
            .into_path_buf(),
    )
}

fn entry(path: &Path, access: FileSystemAccessMode) -> Result<FileSystemSandboxEntry> {
    Ok(FileSystemSandboxEntry::new(
        AbsolutePathBuf::from_absolute_path(path)?.into(),
        access,
    ))
}

fn command(permissions: &PermissionProfile, cwd: &Path) -> MxcCommand {
    MxcCommand {
        permissions: permissions.clone(),
        sandbox_policy_cwd: cwd.to_owned(),
        command: vec!["program.exe".to_owned(), "--arg".to_owned()],
    }
}

#[test]
fn native_grants_preserve_denies_and_read_only_carveouts() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let canonical = canonical_root(&temp)?;
    let root = canonical.as_path();
    let gitdir = tempfile::tempdir()?;
    std::fs::write(
        root.join(".git"),
        format!("gitdir: {}", gitdir.path().display()),
    )?;
    let readonly = root.join("readonly");
    let writable_child = readonly.join("writable");
    let denied = root.join("secret");
    let fs = FileSystemSandboxPolicy::restricted(vec![
        entry(root, FileSystemAccessMode::Write)?,
        entry(&readonly, FileSystemAccessMode::Read)?,
        entry(&writable_child, FileSystemAccessMode::Write)?,
        entry(&denied, FileSystemAccessMode::Deny)?,
    ]);
    let profile =
        PermissionProfile::from_runtime_permissions(&fs, NetworkSandboxPolicy::Restricted);
    let request = build_request(&command(&profile, root), root, Vec::new())?;
    let mut expected_read = [
        root.join(".agents"),
        root.join(".codex"),
        root.join(".git"),
        readonly,
        writable_child.join(".agents"),
        writable_child.join(".codex"),
        writable_child.join(".git"),
    ];
    expected_read.sort();
    assert_eq!(
        (
            request.policy.readwrite_paths,
            request.policy.readonly_paths,
            request.policy.denied_paths
        ),
        (
            vec![
                root.to_str().unwrap().to_owned(),
                writable_child.to_str().unwrap().to_owned()
            ],
            expected_read
                .iter()
                .map(|path| path.to_str().unwrap().to_owned())
                .collect::<Vec<_>>(),
            vec![denied.to_str().unwrap().to_owned()],
        )
    );
    assert!(!request.policy.fallback.allow_dacl_mutation);
    Ok(())
}

#[test]
fn unsupported_deny_globs_fail_before_launch() -> Result<()> {
    let root = tempfile::tempdir()?;
    let fs = FileSystemSandboxPolicy::restricted(vec![FileSystemSandboxEntry::new(
        FileSystemPath::GlobPattern {
            pattern: "**/*.secret".to_owned(),
        },
        FileSystemAccessMode::Deny,
    )]);
    let profile =
        PermissionProfile::from_runtime_permissions(&fs, NetworkSandboxPolicy::Restricted);
    let error =
        build_request(&command(&profile, root.path()), root.path(), Vec::new()).unwrap_err();
    assert_eq!(
        error.to_string(),
        "MXC does not support filesystem deny glob patterns"
    );
    Ok(())
}

#[test]
fn relative_working_directories_fail_before_launch() -> Result<()> {
    let root = tempfile::tempdir()?;
    let profile = PermissionProfile::from_runtime_permissions(
        &FileSystemSandboxPolicy::restricted(Vec::new()),
        NetworkSandboxPolicy::Restricted,
    );
    for (policy_cwd, command_cwd, kind) in [
        (Path::new("relative"), root.path(), "policy"),
        (root.path(), Path::new("relative"), "command"),
    ] {
        let error =
            build_request(&command(&profile, policy_cwd), command_cwd, Vec::new()).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("MXC requires an absolute {kind} working directory")
        );
    }
    Ok(())
}

#[test]
fn non_unicode_paths_fail_before_launch() -> Result<()> {
    #[cfg(unix)]
    let name = <std::ffi::OsString as std::os::unix::ffi::OsStringExt>::from_vec(vec![0xff]);
    #[cfg(windows)]
    let name = <std::ffi::OsString as std::os::windows::ffi::OsStringExt>::from_wide(&[0xd800]);
    let temp = tempfile::tempdir()?;
    let root = canonical_root(&temp)?;
    let invalid = root.join(name);
    for (policy_path, command_cwd, expected) in [
        (
            root.as_path(),
            invalid.as_path(),
            "MXC requires a Unicode command working directory",
        ),
        (
            invalid.as_path(),
            root.as_path(),
            "MXC requires Unicode filesystem policy paths",
        ),
    ] {
        let fs = FileSystemSandboxPolicy::restricted(vec![entry(
            policy_path,
            FileSystemAccessMode::Deny,
        )?]);
        let profile =
            PermissionProfile::from_runtime_permissions(&fs, NetworkSandboxPolicy::Restricted);
        let error = build_request(&command(&profile, &root), command_cwd, Vec::new()).unwrap_err();
        assert_eq!(error.to_string(), expected);
    }
    Ok(())
}
