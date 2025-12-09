use code_core::review_coord::{
    bump_snapshot_epoch_for,
    current_snapshot_epoch_for,
    read_lock_info,
    try_acquire_lock,
};
use code_git_tooling::{create_ghost_commit, CreateGhostCommitOptions};
use std::process::Command;
use tempfile::TempDir;

fn set_code_home(path: &std::path::Path) {
    // SAFETY: tests run in a single thread and isolate CODE_HOME per test case.
    unsafe { std::env::set_var("CODE_HOME", path); }
}

// Integration-style coverage of lock contention and stale-epoch handling across components.
#[test]
fn lock_contention_and_epoch_refresh_across_components() {
    let dir = TempDir::new().unwrap();
    set_code_home(dir.path());
    let cwd = dir.path();

    // Component A acquires the global lock and captures the epoch.
    let guard_a = try_acquire_lock("component-a", cwd).unwrap().expect("lock available");
    let info_a = read_lock_info(Some(cwd)).expect("lock info present");
    let epoch_a = current_snapshot_epoch_for(cwd);
    assert_eq!(info_a.snapshot_epoch, epoch_a);

    // Component B should be blocked while A holds the lock.
    let guard_b_blocked = try_acquire_lock("component-b", cwd).unwrap();
    assert!(guard_b_blocked.is_none());

    // Simulate a git mutation while A holds the lock; epoch advances.
    bump_snapshot_epoch_for(cwd);
    let epoch_after = current_snapshot_epoch_for(cwd);
    assert!(epoch_after > epoch_a);

    // Release A, then acquire with B; B should see the newer epoch.
    drop(guard_a);
    let guard_b = try_acquire_lock("component-b", cwd).unwrap().expect("lock available");
    let info_b = read_lock_info(Some(cwd)).expect("lock info present");
    assert_eq!(info_b.snapshot_epoch, epoch_after);
    drop(guard_b);
}

#[test]
fn ghost_commit_bumps_epoch_and_stale_resume_is_detectable() {
    let code_home = TempDir::new().unwrap();
    set_code_home(code_home.path());

    let repo_dir = TempDir::new().unwrap();
    let repo = repo_dir.path();

    // Init repo and base commit
    assert!(Command::new("git")
        .current_dir(repo)
        .args(["init", "--initial-branch=main"])
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .current_dir(repo)
        .args(["config", "user.email", "test@example.com"])
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .current_dir(repo)
        .args(["config", "user.name", "Tester"])
        .status()
        .unwrap()
        .success());
    std::fs::write(repo.join("file.txt"), "hello") .unwrap();
    assert!(Command::new("git")
        .current_dir(repo)
        .args(["add", "file.txt"])
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .current_dir(repo)
        .args(["commit", "-m", "base"])
        .status()
        .unwrap()
        .success());

    let before = current_snapshot_epoch_for(repo);
    let bump = || bump_snapshot_epoch_for(repo);
    let options = CreateGhostCommitOptions::new(repo).post_commit_hook(&bump);
    let _ghost = create_ghost_commit(&options).expect("ghost commit");
    let after = current_snapshot_epoch_for(repo);
    assert!(after > before, "ghost commit should bump epoch");

    // Simulate apply/patch failure + resume bumping the epoch
    let captured_epoch = after;
    bump_snapshot_epoch_for(repo);
    let resumed_epoch = current_snapshot_epoch_for(repo);
    assert!(resumed_epoch > captured_epoch, "resume must see newer epoch");
    assert_ne!(captured_epoch, resumed_epoch, "stale snapshot should be detectable");
}
