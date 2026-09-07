//! Validates repository identity and discovery across linked checkout layouts.

use super::*;
use pretty_assertions::assert_eq;

fn repository_with_linked_checkout() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let root = tempfile::tempdir().expect("temporary repository");
    let primary = root.path().join("primary");
    let linked = root.path().join("linked");
    let admin = primary.join(".git").join("worktrees").join("linked");
    fs::create_dir_all(primary.join("nested")).expect("primary nested directory");
    fs::create_dir_all(linked.join("nested")).expect("linked nested directory");
    fs::create_dir_all(&admin).expect("worktree administrative directory");
    fs::write(primary.join(".git/HEAD"), "ref: refs/heads/main\n")
        .expect("primary repository HEAD");
    fs::write(admin.join("commondir"), "../..\n").expect("common directory");
    fs::write(
        admin.join("gitdir"),
        format!("{}\n", linked.join(".git").display()),
    )
    .expect("linked checkout backlink");
    fs::write(
        linked.join(".git"),
        format!("gitdir: {}\n", admin.display()),
    )
    .expect("linked checkout git file");
    (root, primary, linked)
}

#[test]
fn repository_identity_is_shared_by_primary_and_linked_checkouts() {
    let (_root, primary, linked) = repository_with_linked_checkout();
    let primary_cwd = canonicalize_native(primary.join("nested")).expect("primary cwd");
    let linked_cwd = canonicalize_native(linked.join("nested")).expect("linked cwd");
    let expected = RepositoryIdentity {
        common_dir: AbsolutePathBuf::from_absolute_path_checked(
            canonicalize_native(primary.join(".git")).expect("common git directory"),
        )
        .expect("absolute common directory"),
        relative_cwd: PathBuf::from("nested"),
        primary_root: AbsolutePathBuf::from_absolute_path_checked(
            canonicalize_native(&primary).expect("primary checkout"),
        )
        .expect("absolute primary checkout"),
    };

    assert_eq!(repository_identity(&primary_cwd), Some(expected.clone()));
    assert_eq!(repository_identity(&linked_cwd), Some(expected));
    assert_eq!(
        linked_worktree_cwds(&linked_cwd),
        Some(vec![linked_cwd, primary_cwd])
    );
}

#[cfg(unix)]
#[test]
fn linked_worktree_discovery_preserves_logical_working_directory_aliases() {
    let (root, primary, linked) = repository_with_linked_checkout();
    let alias = root.path().join("primary-alias");
    std::os::unix::fs::symlink(&primary, &alias).expect("checkout alias");
    let logical_cwd = alias.join("nested");
    let canonical_cwd = fs::canonicalize(primary.join("nested")).expect("primary cwd");
    let linked_cwd = fs::canonicalize(linked.join("nested")).expect("linked cwd");

    assert_eq!(
        linked_worktree_cwds(&logical_cwd),
        Some(vec![logical_cwd, canonical_cwd, linked_cwd])
    );
}

#[test]
fn linked_worktree_discovery_rejects_mismatched_backlinks() {
    let (_root, primary, linked) = repository_with_linked_checkout();
    let primary_cwd = canonicalize_native(primary.join("nested")).expect("primary cwd");
    let admin = primary.join(".git").join("worktrees").join("linked");
    fs::write(
        admin.join("gitdir"),
        primary.join(".git").display().to_string(),
    )
    .expect("invalid worktree backlink");

    assert_eq!(repository_identity(&linked.join("nested")), None);
    assert_eq!(linked_worktree_cwds(&primary_cwd), Some(vec![primary_cwd]));
}

#[cfg(unix)]
#[test]
fn linked_worktree_discovery_rejects_relative_directory_symlink_escapes() {
    let (root, primary, linked) = repository_with_linked_checkout();
    let primary_cwd = fs::canonicalize(primary.join("nested")).expect("primary cwd");
    let outside = root.path().join("outside");
    fs::create_dir(&outside).expect("outside directory");
    fs::remove_dir(linked.join("nested")).expect("remove linked nested directory");
    std::os::unix::fs::symlink(&outside, linked.join("nested")).expect("escaping symlink");

    assert_eq!(linked_worktree_cwds(&primary_cwd), Some(vec![primary_cwd]));
}
