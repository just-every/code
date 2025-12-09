use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const LOCK_FILENAME: &str = "review.lock";
const EPOCH_FILENAME: &str = "snapshot.epoch";

#[derive(Debug, Serialize, Deserialize)]
pub struct ReviewLockInfo {
    pub pid: u32,
    pub started_at: u64,
    pub intent: String,
    pub git_head: Option<String>,
    pub snapshot_epoch: u64,
}

pub struct ReviewGuard {
    lock_path: PathBuf,
}

fn state_dir() -> std::io::Result<PathBuf> {
    let mut dir = crate::config::find_code_home()?;
    dir.push("state");
    dir.push("review");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn scoped_dir(scope: Option<&Path>) -> std::io::Result<PathBuf> {
    let mut dir = state_dir()?;
    if let Some(scope) = scope {
        let normalized_scope = scope
            .canonicalize()
            .unwrap_or_else(|_| scope.to_path_buf());
        let key = crc32fast::hash(normalized_scope.to_string_lossy().as_bytes());
        dir.push(format!("repo-{key:08x}"));
        fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

fn epoch_path(scope: Option<&Path>) -> std::io::Result<PathBuf> {
    let mut dir = scoped_dir(scope)?;
    dir.push(EPOCH_FILENAME);
    Ok(dir)
}

fn lock_path(scope: Option<&Path>) -> std::io::Result<PathBuf> {
    let mut p = scoped_dir(scope)?;
    p.push(LOCK_FILENAME);
    Ok(p)
}

fn read_epoch(scope: Option<&Path>) -> u64 {
    if let Ok(p) = epoch_path(scope) {
        if let Ok(text) = fs::read_to_string(p) {
            if let Ok(v) = text.trim().parse::<u64>() {
                return v;
            }
        }
    }
    0
}

fn write_epoch(scope: Option<&Path>, val: u64) -> std::io::Result<()> {
    let p = epoch_path(scope)?;
    fs::write(p, val.to_string())
}

pub fn bump_snapshot_epoch() {
    if let Some(scope) = scope_from_current_dir() {
        bump_snapshot_epoch_for(&scope);
        return;
    }

    let current = read_epoch(None);
    let _ = write_epoch(None, current.saturating_add(1));
}

pub fn current_snapshot_epoch() -> u64 {
    if let Some(scope) = scope_from_current_dir() {
        return current_snapshot_epoch_for(&scope);
    }

    read_epoch(None)
}

pub fn bump_snapshot_epoch_for(scope: &Path) {
    let current = read_epoch(Some(scope));
    let _ = write_epoch(Some(scope), current.saturating_add(1));
}

pub fn current_snapshot_epoch_for(scope: &Path) -> u64 {
    read_epoch(Some(scope))
}

fn scope_from_current_dir() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    if let Ok(out) = Command::new("git")
        .current_dir(&cwd)
        .args(["rev-parse", "--show-toplevel"])
        .output()
    {
        if out.status.success() {
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(PathBuf::from(path));
            }
        }
    }
    Some(cwd)
}

fn git_head(cwd: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .current_dir(cwd)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn try_acquire_lock(intent: &str, cwd: &Path) -> std::io::Result<Option<ReviewGuard>> {
    let lock_path = lock_path(Some(cwd))?;
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path);

    match file {
        Ok(mut f) => {
            let info = ReviewLockInfo {
                pid: std::process::id(),
                started_at: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                intent: intent.to_string(),
                git_head: git_head(cwd),
                snapshot_epoch: current_snapshot_epoch_for(cwd),
            };
            let body = serde_json::to_string_pretty(&info).unwrap_or_default();
            let _ = f.write_all(body.as_bytes());
            Ok(Some(ReviewGuard { lock_path }))
        }
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => Ok(None),
        Err(err) => Err(err),
    }
}

pub fn read_lock_info(scope: Option<&Path>) -> Option<ReviewLockInfo> {
    let path = lock_path(scope).ok()?;
    let mut buf = String::new();
    File::open(path).ok()?.read_to_string(&mut buf).ok()?;
    serde_json::from_str(&buf).ok()
}

#[cfg(unix)]
fn pid_alive(pid: u32) -> bool {
    // Safety: kill with signal 0 performs permission/aliveness check only
    let res = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if res == 0 {
        true
    } else {
        // ESRCH => no such process; EPERM => process exists but not permitted
        let err = std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or(libc::ESRCH);
        err != libc::ESRCH
    }
}

#[cfg(not(unix))]
fn pid_alive(_pid: u32) -> bool {
    // Best-effort: assume alive to avoid clobbering valid locks on non-Unix platforms
    true
}

/// Remove a stale review lock if the recorded pid is no longer running.
/// Returns true if a stale lock was cleared.
pub fn clear_stale_lock_if_dead(scope: Option<&Path>) -> std::io::Result<bool> {
    let info = match read_lock_info(scope) {
        Some(i) => i,
        None => return Ok(false),
    };
    if pid_alive(info.pid) {
        return Ok(false);
    }
    if let Ok(path) = lock_path(scope) {
        let _ = fs::remove_file(path);
        return Ok(true);
    }
    Ok(false)
}

impl Drop for ReviewGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.lock_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use serial_test::serial;
    use tempfile::TempDir;

    fn set_code_home(path: &Path) {
        // SAFETY: tests run serially and isolate CODE_HOME within a temp dir per test.
        unsafe { std::env::set_var("CODE_HOME", path); }
    }

    #[test]
    #[serial]
    fn lock_contention_and_release() {
        let dir = TempDir::new().unwrap();
        set_code_home(dir.path());
        let cwd = dir.path();

        let g1 = try_acquire_lock("test", cwd).unwrap();
        assert!(g1.is_some());
        let g2 = try_acquire_lock("test2", cwd).unwrap();
        assert!(g2.is_none());
        drop(g1);
        let g3 = try_acquire_lock("test3", cwd).unwrap();
        assert!(g3.is_some());
        drop(g3);
    }

    #[test]
    #[serial]
    fn epoch_bump_changes_value() {
        let dir = TempDir::new().unwrap();
        set_code_home(dir.path());
        let cwd = dir.path();
        let e0 = current_snapshot_epoch_for(cwd);
        bump_snapshot_epoch_for(cwd);
        let e1 = current_snapshot_epoch_for(cwd);
        assert!(e1 > e0);
    }

    #[test]
    #[serial]
    fn lock_records_snapshot_epoch_and_updates_after_bump() {
        let dir = TempDir::new().unwrap();
        set_code_home(dir.path());
        let cwd = dir.path();

        bump_snapshot_epoch_for(cwd);
        let current = current_snapshot_epoch_for(cwd);
        let guard = try_acquire_lock("first", cwd).unwrap().expect("lock available");
        let info = read_lock_info(Some(cwd)).expect("lock info present");
        assert_eq!(info.snapshot_epoch, current);
        drop(guard);

        bump_snapshot_epoch_for(cwd);
        let next = current_snapshot_epoch_for(cwd);
        assert!(next > current);
        let guard2 = try_acquire_lock("second", cwd).unwrap().expect("lock reacquired");
        let info2 = read_lock_info(Some(cwd)).expect("lock info present");
        assert_eq!(info2.snapshot_epoch, next);
        drop(guard2);
    }

    #[test]
    #[serial]
    fn lock_info_survives_epoch_bump_for_stale_detection() {
        let dir = TempDir::new().unwrap();
        set_code_home(dir.path());
        let cwd = dir.path();

        let guard = try_acquire_lock("stale-check", cwd).unwrap().expect("lock available");
        let initial = read_lock_info(Some(cwd)).expect("lock info present");
        bump_snapshot_epoch_for(cwd);
        let now = current_snapshot_epoch_for(cwd);

        // The on-disk lock should still show the snapshot_epoch captured at acquisition time,
        // allowing callers to detect that the world has moved on while the lock holder runs.
        let still_recorded = read_lock_info(Some(cwd)).expect("lock info present");
        assert_eq!(still_recorded.snapshot_epoch, initial.snapshot_epoch);
        assert!(now > initial.snapshot_epoch);
        drop(guard);
    }

    #[test]
    #[serial]
    fn lock_contention_across_components() {
        let dir = TempDir::new().unwrap();
        set_code_home(dir.path());
        let cwd = dir.path();

        let exec_lock = try_acquire_lock("exec-review", cwd).unwrap();
        assert!(exec_lock.is_some());
        let tui_lock = try_acquire_lock("tui-review", cwd).unwrap();
        assert!(tui_lock.is_none(), "TUI should be blocked while exec holds lock");
        drop(exec_lock);
        let auto_lock = try_acquire_lock("auto-drive-review", cwd).unwrap();
        assert!(auto_lock.is_some());
        drop(auto_lock);
    }

    #[test]
    #[serial]
    fn stale_epoch_detected_after_git_mutation() {
        let dir = TempDir::new().unwrap();
        set_code_home(dir.path());
        let cwd = dir.path();

        let before = current_snapshot_epoch_for(cwd);
        let guard = try_acquire_lock("exec", cwd).unwrap().unwrap();
        let captured = read_lock_info(Some(cwd)).unwrap().snapshot_epoch;
        assert_eq!(captured, before);

        bump_snapshot_epoch_for(cwd); // simulate git mutation while lock holder runs
        let after = current_snapshot_epoch_for(cwd);
        assert!(after > captured);

        // A follow-up caller comparing epochs would notice the mismatch.
        assert_ne!(captured, after);
        drop(guard);
    }

    #[test]
    #[serial]
    fn apply_failure_resume_requires_fresh_epoch() {
        let dir = TempDir::new().unwrap();
        set_code_home(dir.path());
        let cwd = dir.path();

        let initial = current_snapshot_epoch_for(cwd);
        // Snapshot taken at initial epoch
        let snapshot_epoch = initial;
        // Simulate apply/patch failure causing repo mutation guard to bump
        bump_snapshot_epoch_for(cwd);
        let resumed = current_snapshot_epoch_for(cwd);
        assert!(resumed > snapshot_epoch);
        // A resume that still uses the old snapshot must be treated as stale
        assert_ne!(snapshot_epoch, resumed);
        let guard = try_acquire_lock("resume-review", cwd).unwrap().unwrap();
        let info = read_lock_info(Some(cwd)).unwrap();
        assert_eq!(info.snapshot_epoch, resumed);
        drop(guard);
    }
}
