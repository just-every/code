use crate::git_worktree;
use crate::rollout::SESSIONS_SUBDIR;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use time::{Date, OffsetDateTime};
use tracing::{debug, info, warn};

const DEFAULT_SESSION_RETENTION_DAYS: i64 = 7;
const DEFAULT_WORKTREE_RETENTION_DAYS: i64 = 3;
const DEFAULT_MIN_INTERVAL_HOURS: i64 = 6;
const LOCK_FILE_NAME: &str = "cleanup.lock";
const STATE_FILE_NAME: &str = "cleanup-state.json";

#[derive(Debug, Clone, Default)]
pub struct CleanupOutcome {
    pub session_days_removed: usize,
    pub session_files_removed: usize,
    pub session_bytes_reclaimed: u64,
    pub worktrees_removed: usize,
    pub worktree_files_removed: usize,
    pub worktree_bytes_reclaimed: u64,
    pub worktrees_skipped_active: usize,
    pub errors: usize,
}

#[derive(Debug, Clone)]
struct HousekeepingConfig {
    session_retention_days: Option<i64>,
    worktree_retention_days: Option<i64>,
    min_interval_hours: i64,
    disabled: bool,
}

impl HousekeepingConfig {
    fn from_env() -> Self {
        let disabled = std::env::var("CODE_CLEANUP_DISABLE")
            .map(|value| matches_ignore_case(&value, &["1", "true", "on", "yes"]))
            .unwrap_or(false);

        let session_retention_days = parse_days_env(
            "CODE_CLEANUP_SESSION_RETENTION_DAYS",
            DEFAULT_SESSION_RETENTION_DAYS,
        );
        let worktree_retention_days = parse_days_env(
            "CODE_CLEANUP_WORKTREE_RETENTION_DAYS",
            DEFAULT_WORKTREE_RETENTION_DAYS,
        );
        let min_interval_hours = parse_positive_i64_env(
            "CODE_CLEANUP_MIN_INTERVAL_HOURS",
            DEFAULT_MIN_INTERVAL_HOURS,
        );

        Self {
            session_retention_days,
            worktree_retention_days,
            min_interval_hours,
            disabled,
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
struct CleanupState {
    last_run_unix: Option<i64>,
}

struct HeldLock(File);

impl HeldLock {
    fn new(file: File) -> Self {
        Self(file)
    }
}

impl Drop for HeldLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

pub fn run_housekeeping_if_due(code_home: &Path) -> io::Result<Option<CleanupOutcome>> {
    let config = HousekeepingConfig::from_env();

    if config.disabled {
        debug!("code home housekeeping disabled via CODE_CLEANUP_DISABLE");
        return Ok(None);
    }

    let lock_path = code_home.join(LOCK_FILE_NAME);
    let maybe_lock = acquire_lock(&lock_path)?;
    let Some(lock_file) = maybe_lock else {
        debug!("code home housekeeping skipped; another process holds the lock");
        return Ok(None);
    };
    let _lock_guard = HeldLock::new(lock_file);

    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());

    let state_path = code_home.join(STATE_FILE_NAME);
    let mut state = read_state(&state_path)?;

    if let Some(last_run) = state
        .last_run_unix
        .and_then(|ts| OffsetDateTime::from_unix_timestamp(ts).ok())
    {
        let min_interval = time::Duration::hours(config.min_interval_hours.max(0));
        if !min_interval.is_zero() && now - last_run < min_interval {
            debug!("code home housekeeping skipped; ran recently");
            return Ok(None);
        }
    }

    let outcome = perform_housekeeping(code_home, now, &config)?;
    state.last_run_unix = Some(now.unix_timestamp());
    if let Err(err) = write_state(&state_path, &state) {
        warn!("failed to persist housekeeping state: {err}");
    }

    if outcome.errors > 0 {
        warn!(
            "code home housekeeping completed with {} error(s)",
            outcome.errors
        );
    }

    if outcome.session_days_removed > 0 || outcome.worktrees_removed > 0 {
        info!(
            sessions_pruned = outcome.session_days_removed,
            session_bytes_reclaimed = outcome.session_bytes_reclaimed,
            worktrees_pruned = outcome.worktrees_removed,
            worktree_bytes_reclaimed = outcome.worktree_bytes_reclaimed,
            skipped_active_worktrees = outcome.worktrees_skipped_active,
            "code home housekeeping pruned stale artifacts"
        );
    } else {
        debug!("code home housekeeping completed; nothing to prune");
    }

    Ok(Some(outcome))
}

fn perform_housekeeping(
    code_home: &Path,
    now: OffsetDateTime,
    config: &HousekeepingConfig,
) -> io::Result<CleanupOutcome> {
    let mut outcome = CleanupOutcome::default();

    if let Some(days) = config.session_retention_days {
        if let Some(stats) = cleanup_sessions(code_home, now.date(), days)? {
            outcome.session_days_removed = stats.removed_days;
            outcome.session_files_removed = stats.removed_files;
            outcome.session_bytes_reclaimed = stats.reclaimed_bytes;
            outcome.errors += stats.errors;
        }
    }

    if let Some(days) = config.worktree_retention_days {
        if let Some(stats) = cleanup_worktrees(code_home, now, days)? {
            outcome.worktrees_removed = stats.removed_worktrees;
            outcome.worktree_files_removed = stats.removed_files;
            outcome.worktree_bytes_reclaimed = stats.reclaimed_bytes;
            outcome.worktrees_skipped_active = stats.skipped_active;
            outcome.errors += stats.errors;
        }
    }

    Ok(outcome)
}

fn cleanup_sessions(
    code_home: &Path,
    today: Date,
    retention_days: i64,
) -> io::Result<Option<SessionCleanupStats>> {
    let sessions_root = code_home.join(SESSIONS_SUBDIR);
    if !sessions_root.exists() {
        return Ok(None);
    }

    let mut stats = SessionCleanupStats::default();
    let keep_today_only = retention_days <= 0;

    let year_dirs = list_dir_sorted(&sessions_root);
    for year_entry in year_dirs {
        let year_path = year_entry.path();
        let year = match parse_u16(&year_entry.file_name()) {
            Some(value) => value as i32,
            None => continue,
        };

        let month_dirs = list_dir_sorted(&year_path);
        for month_entry in month_dirs {
            let month_path = month_entry.path();
            let month_num = match parse_u8(&month_entry.file_name()) {
                Some(value @ 1..=12) => value,
                _ => continue,
            };
            let month = match time::Month::try_from(month_num) {
                Ok(month) => month,
                Err(_) => continue,
            };

            let day_dirs = list_dir_sorted(&month_path);
            for day_entry in day_dirs {
                let day_path = day_entry.path();
                let day_num = match parse_u8(&day_entry.file_name()) {
                    Some(value @ 1..=31) => value,
                    _ => continue,
                };

                let date = match Date::from_calendar_date(year, month, day_num) {
                    Ok(date) => date,
                    Err(_) => continue,
                };

                if date >= today {
                    continue;
                }

                let should_remove = if keep_today_only {
                    date < today
                } else {
                    let age = today - date;
                    age.whole_days() >= retention_days
                };

                if !should_remove {
                    continue;
                }

                let dir_stats = directory_stats(&day_path);
                match fs::remove_dir_all(&day_path) {
                    Ok(_) => {
                        stats.removed_days += 1;
                        stats.removed_files += dir_stats.files;
                        stats.reclaimed_bytes += dir_stats.bytes;
                    }
                    Err(err) => {
                        stats.errors += 1;
                        warn!("failed to remove session directory {:?}: {err}", day_path);
                    }
                }
            }

            if dir_is_empty(&month_path) {
                let _ = fs::remove_dir(&month_path);
            }
        }

        if dir_is_empty(&year_path) {
            let _ = fs::remove_dir(&year_path);
        }
    }

    Ok(Some(stats))
}

fn cleanup_worktrees(
    code_home: &Path,
    now: OffsetDateTime,
    retention_days: i64,
) -> io::Result<Option<WorktreeCleanupStats>> {
    let working_root = code_home.join("working");
    if !working_root.exists() {
        return Ok(None);
    }

    let mut stats = WorktreeCleanupStats::default();
    let retention = if retention_days <= 0 {
        Duration::ZERO
    } else {
        Duration::from_secs(retention_days as u64 * 86_400)
    };

    let active = collect_active_worktrees(&working_root.join("_session"));
    let now_system: SystemTime = SystemTime::from(now);

    let repo_dirs = list_dir_sorted(&working_root);
    for repo_entry in repo_dirs {
        let name = repo_entry.file_name();
        if name.to_string_lossy().starts_with('_') {
            continue;
        }

        let repo_path = repo_entry.path();
        if !repo_entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
            continue;
        }

        let branches_dir = repo_path.join("branches");
        if !branches_dir.is_dir() {
            continue;
        }

        let branch_entries = list_dir_sorted(&branches_dir);
        for branch_entry in branch_entries {
            let branch_path = branch_entry.path();
            if !branch_entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                continue;
            }

            let branch_name = branch_entry.file_name().to_string_lossy().to_string();
            let repo_root = detect_repo_root(&branch_path);

            let canonical = canonicalize_or_original(&branch_path);
            if active.contains(&canonical) || active.contains(&branch_path) {
                stats.skipped_active += 1;
                continue;
            }

            let metadata = match fs::metadata(&branch_path) {
                Ok(meta) => meta,
                Err(err) => {
                    stats.errors += 1;
                    warn!("failed to read metadata for {:?}: {err}", branch_path);
                    continue;
                }
            };

            let modified = match metadata.modified() {
                Ok(ts) => ts,
                Err(err) => {
                    stats.errors += 1;
                    warn!("failed to read modified timestamp for {:?}: {err}", branch_path);
                    continue;
                }
            };

            let age = match now_system.duration_since(modified) {
                Ok(duration) => duration,
                Err(_) => Duration::ZERO,
            };

            if !retention.is_zero() && age < retention {
                continue;
            }

            let dir_stats = directory_stats(&branch_path);
            run_git_worktree_remove(&branch_path);

            let removal_result = match fs::remove_dir_all(&branch_path) {
                Ok(()) => Ok(()),
                Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(err) => Err(err),
            };

            match removal_result {
                Ok(()) => {
                    git_worktree::remove_branch_metadata(&branch_path);
                    purge_session_registry(&working_root.join("_session"), &branch_path);

                    if let Some(repo_root) = repo_root.as_deref() {
                        if should_prune_worktree_branch(&branch_name) {
                            run_git_branch_delete_if_merged(repo_root, &branch_name);
                        }
                    }

                    stats.removed_worktrees += 1;
                    stats.removed_files += dir_stats.files;
                    stats.reclaimed_bytes += dir_stats.bytes;
                }
                Err(err) => {
                    stats.errors += 1;
                    warn!("failed to remove worktree {:?}: {err}", branch_path);
                }
            }
        }

        if dir_is_empty(&branches_dir) {
            let _ = fs::remove_dir(&branches_dir);
        }
        if dir_is_empty(&repo_path) {
            let _ = fs::remove_dir(&repo_path);
        }
    }

    Ok(Some(stats))
}

fn should_prune_worktree_branch(branch_name: &str) -> bool {
    if branch_name == "main" || branch_name == "master" {
        return false;
    }

    branch_name.starts_with("code-") || branch_name == "auto-review"
}

fn run_git_branch_delete_if_merged(repo_root: &Path, branch_name: &str) {
    if !repo_root.exists() {
        return;
    }

    let output = std::process::Command::new("git")
        .current_dir(repo_root)
        .args(["branch", "-d", branch_name])
        .output();

    match output {
        Ok(result) => {
            if !result.status.success() {
                let stderr = String::from_utf8_lossy(&result.stderr);
                debug!(
                    "git branch -d reported error for {:?} in {:?}: {}",
                    branch_name,
                    repo_root,
                    stderr.trim()
                );
            }
        }
        Err(err) => {
            debug!("git branch -d failed for {:?} in {:?}: {err}", branch_name, repo_root);
        }
    }
}

fn run_git_worktree_remove(worktree_path: &Path) {
    let Some(repo_root) = detect_repo_root(worktree_path) else {
        return;
    };

    if !repo_root.exists() {
        return;
    }

    let worktree_str = match worktree_path.to_str() {
        Some(path) => path,
        None => return,
    };

    let output = std::process::Command::new("git")
        .current_dir(&repo_root)
        .args(["worktree", "remove", "--force", worktree_str])
        .output();

    match output {
        Ok(result) => {
            if !result.status.success() {
                let stderr = String::from_utf8_lossy(&result.stderr);
                debug!(
                    "git worktree remove reported error for {:?}: {}",
                    worktree_path,
                    stderr.trim()
                );
            }
        }
        Err(err) => {
            debug!("git worktree remove failed for {:?}: {err}", worktree_path);
        }
    }
}

fn detect_repo_root(worktree_path: &Path) -> Option<PathBuf> {
    let git_file = worktree_path.join(".git");
    let data = fs::read_to_string(git_file).ok()?;
    let gitdir_line = data
        .lines()
        .find_map(|line| line.trim().strip_prefix("gitdir:"))?;
    let gitdir_value = gitdir_line.trim();

    let mut gitdir_path = PathBuf::from(gitdir_value);
    if !gitdir_path.is_absolute() {
        gitdir_path = worktree_path.join(gitdir_value);
    }
    gitdir_path = gitdir_path.canonicalize().unwrap_or(gitdir_path);

    let mut current = gitdir_path;
    let mut levels = 0;
    while levels < 5 {
        if current.file_name().map(|f| f == ".git").unwrap_or(false) {
            current.pop();
            return Some(current);
        }
        if !current.pop() {
            break;
        }
        levels += 1;
    }

    None
}

fn collect_active_worktrees(session_dir: &Path) -> HashSet<PathBuf> {
    let mut set = HashSet::new();
    let entries = match fs::read_dir(session_dir) {
        Ok(entries) => entries,
        Err(_) => return set,
    };

    for entry in entries.flatten() {
        if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            continue;
        }

        let file_path = entry.path();
        let is_active = pid_file_is_active(entry.file_name().as_os_str()).unwrap_or(false);
        if !is_active {
            let _ = fs::remove_file(&file_path);
            continue;
        }

        let data = match fs::read_to_string(&file_path) {
            Ok(data) => data,
            Err(_) => continue,
        };

        for line in data.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let worktree = match line.split_once('\t') {
                Some((_, path)) => path,
                None => continue,
            };
            let path = PathBuf::from(worktree);
            if let Ok(canon) = path.canonicalize() {
                set.insert(canon);
            } else {
                set.insert(path);
            }
        }
    }

    set
}

fn purge_session_registry(session_dir: &Path, worktree_path: &Path) {
    let entries = match fs::read_dir(session_dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    let worktree_str = worktree_path.to_string_lossy().to_string();

    for entry in entries.flatten() {
        if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            continue;
        }
        let file_path = entry.path();
        let data = match fs::read_to_string(&file_path) {
            Ok(data) => data,
            Err(_) => continue,
        };

        let mut changed = false;
        let mut kept: Vec<&str> = Vec::new();
        for line in data.lines() {
            if line.split_once('\t').map_or(false, |(_, path)| path == worktree_str) {
                changed = true;
            } else if !line.trim().is_empty() {
                kept.push(line);
            }
        }

        if !changed {
            continue;
        }

        if let Ok(mut file) = OpenOptions::new().write(true).truncate(true).open(&file_path) {
            if !kept.is_empty() {
                let content = kept.join("\n");
                let _ = file.write_all(content.as_bytes());
                let _ = file.write_all(b"\n");
            }
        }
    }
}

fn pid_file_is_active(file_name: &OsStr) -> Option<bool> {
    let name = file_name.to_string_lossy();
    let rest = name.strip_prefix("pid-")?;
    let pid_str = rest.strip_suffix(".txt").unwrap_or(rest);
    let pid: i32 = pid_str.parse().ok()?;
    check_pid_alive(pid)
}

#[cfg(target_os = "linux")]
fn check_pid_alive(pid: i32) -> Option<bool> {
    use std::path::Path;

    Some(Path::new("/proc").join(pid.to_string()).exists())
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn check_pid_alive(pid: i32) -> Option<bool> {
    use libc::{kill, c_int};
    const SIGZERO: c_int = 0;
    let result = unsafe { kill(pid, SIGZERO) };
    if result == 0 {
        return Some(true);
    }
    let errno = std::io::Error::last_os_error().raw_os_error()?;
    Some(errno != libc::ESRCH)
}

#[cfg(target_os = "windows")]
fn check_pid_alive(pid: i32) -> Option<bool> {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    unsafe {
        let handle: HANDLE = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid as u32);
        if handle.is_null() {
            return Some(false);
        }

        let mut status: u32 = 0;
        let ok = GetExitCodeProcess(handle, &mut status as *mut u32);
        CloseHandle(handle);
        if ok == 0 {
            return None;
        }
        Some(status == STILL_ACTIVE as u32)
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "ios", target_os = "windows")))]
fn check_pid_alive(_pid: i32) -> Option<bool> {
    None
}

#[derive(Default)]
struct SessionCleanupStats {
    removed_days: usize,
    removed_files: usize,
    reclaimed_bytes: u64,
    errors: usize,
}

#[derive(Default)]
struct WorktreeCleanupStats {
    removed_worktrees: usize,
    removed_files: usize,
    reclaimed_bytes: u64,
    skipped_active: usize,
    errors: usize,
}

fn directory_stats(path: &Path) -> DirStats {
    let mut stats = DirStats::default();
    let mut stack = vec![path.to_path_buf()];

    while let Some(current) = stack.pop() {
        let entries = match fs::read_dir(&current) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            match entry.file_type() {
                Ok(file_type) if file_type.is_dir() => {
                    stack.push(entry.path());
                }
                Ok(_) => {
                    stats.files += 1;
                    if let Ok(meta) = entry.metadata() {
                        stats.bytes += meta.len();
                    }
                }
                Err(_) => continue,
            }
        }
    }

    stats
}

#[derive(Default)]
struct DirStats {
    bytes: u64,
    files: usize,
}

fn list_dir_sorted(path: &Path) -> Vec<fs::DirEntry> {
    let mut entries: Vec<_> = match fs::read_dir(path) {
        Ok(it) => it.flatten().collect(),
        Err(_) => Vec::new(),
    };
    entries.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    entries
}

fn parse_u8(name: &std::ffi::OsStr) -> Option<u8> {
    name.to_string_lossy().parse::<u8>().ok()
}

fn parse_u16(name: &std::ffi::OsStr) -> Option<u16> {
    name.to_string_lossy().parse::<u16>().ok()
}

fn dir_is_empty(path: &Path) -> bool {
    fs::read_dir(path).map(|mut it| it.next().is_none()).unwrap_or(false)
}

fn canonicalize_or_original(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn acquire_lock(path: &Path) -> io::Result<Option<File>> {
    let mut opts = OpenOptions::new();
    opts.create(true).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let file = opts.open(path)?;
    match file.try_lock_exclusive() {
        Ok(()) => Ok(Some(file)),
        Err(err) if err.kind() == io::ErrorKind::WouldBlock => Ok(None),
        Err(err) => Err(err),
    }
}

fn read_state(path: &Path) -> io::Result<CleanupState> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(CleanupState::default()),
        Err(err) => Err(err),
    }
}

fn write_state(path: &Path, state: &CleanupState) -> io::Result<()> {
    let mut opts = OpenOptions::new();
    opts.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts.open(path)?;
    let data = serde_json::to_vec(state).map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
    file.write_all(&data)?;
    file.write_all(b"\n")?;
    file.sync_all()
}

fn parse_days_env(var: &str, default: i64) -> Option<i64> {
    match std::env::var(var) {
        Ok(value) => {
            if matches_ignore_case(&value, &["off", "disable", "disabled"]) {
                return None;
            }
            match value.trim().parse::<i64>() {
                Ok(days) if days >= 0 => Some(days),
                Ok(_) => None,
                Err(_) => {
                    warn!(
                        "invalid value for {} ({}); falling back to default {}",
                        var,
                        value,
                        default
                    );
                    Some(default)
                }
            }
        }
        Err(std::env::VarError::NotPresent) => Some(default),
        Err(err) => {
            warn!("failed to read {}: {err}; using default {}", var, default);
            Some(default)
        }
    }
}

fn parse_positive_i64_env(var: &str, default: i64) -> i64 {
    match std::env::var(var) {
        Ok(value) => match value.trim().parse::<i64>() {
            Ok(num) if num > 0 => num,
            Ok(_) => default,
            Err(_) => {
                warn!(
                    "invalid value for {} ({}); falling back to default {}",
                    var,
                    value,
                    default
                );
                default
            }
        },
        Err(std::env::VarError::NotPresent) => default,
        Err(err) => {
            warn!("failed to read {}: {err}; using default {}", var, default);
            default
        }
    }
}

fn matches_ignore_case(value: &str, options: &[&str]) -> bool {
    options
        .iter()
        .any(|candidate| value.eq_ignore_ascii_case(candidate))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;
    use time::macros::datetime;

    #[test]
    fn removes_sessions_outside_retention_window() {
        let temp = TempDir::new().unwrap();
        let code_home = temp.path();
        let old_path = code_home.join("sessions/2025/09/25");
        let recent_path = code_home.join("sessions/2025/10/09");
        fs::create_dir_all(&old_path).unwrap();
        fs::create_dir_all(&recent_path).unwrap();
        fs::write(old_path.join("rollout-old.jsonl"), b"{}").unwrap();
        fs::write(recent_path.join("rollout-new.jsonl"), b"{}").unwrap();

        let config = HousekeepingConfig {
            session_retention_days: Some(7),
            worktree_retention_days: None,
            min_interval_hours: 1,
            disabled: false,
        };

        let now = datetime!(2025-10-10 12:00:00 UTC);
        let outcome = perform_housekeeping(code_home, now, &config).unwrap();

        assert_eq!(outcome.session_days_removed, 1);
        assert!(!old_path.exists());
        assert!(!old_path.parent().unwrap().exists());
        assert!(recent_path.exists());
    }

    #[test]
    fn removes_worktrees_not_in_session_registry() {
        let temp = TempDir::new().unwrap();
        let code_home = temp.path();
        let worktree_path = code_home.join("working/demo/branches/test-branch");
        fs::create_dir_all(&worktree_path).unwrap();
        fs::write(worktree_path.join(".git"), "gitdir: /tmp/nonexistent/.git/worktrees/test-branch\n").unwrap();
        fs::write(worktree_path.join("README.md"), b"placeholder").unwrap();

        let config = HousekeepingConfig {
            session_retention_days: None,
            worktree_retention_days: Some(0),
            min_interval_hours: 1,
            disabled: false,
        };

        let now = datetime!(2025-10-10 12:00:00 UTC);
        let outcome = perform_housekeeping(code_home, now, &config).unwrap();

        assert_eq!(outcome.worktrees_removed, 1);
        assert!(!worktree_path.exists());
    }

    #[test]
    fn keeps_active_worktrees() {
        let temp = TempDir::new().unwrap();
        let code_home = temp.path();
        let worktree_path = code_home.join("working/demo/branches/active-branch");
        fs::create_dir_all(&worktree_path).unwrap();
        fs::write(worktree_path.join(".git"), "gitdir: /tmp/nonexistent/.git/worktrees/active-branch\n").unwrap();
        let session_dir = code_home.join("working/_session");
        fs::create_dir_all(&session_dir).unwrap();
        let pid = std::process::id();
        let registry_path = session_dir.join(format!("pid-{pid}.txt"));
        let line = format!("/tmp/nonexistent\t{}\n", worktree_path.display());
        fs::write(&registry_path, line).unwrap();

        let config = HousekeepingConfig {
            session_retention_days: None,
            worktree_retention_days: Some(0),
            min_interval_hours: 1,
            disabled: false,
        };

        let now = datetime!(2025-10-10 12:00:00 UTC);
        let outcome = perform_housekeeping(code_home, now, &config).unwrap();

        assert_eq!(outcome.worktrees_removed, 0);
        assert_eq!(outcome.worktrees_skipped_active, 1);
        assert!(worktree_path.exists());
    }

    #[test]
    fn prunes_merged_code_branches_after_worktree_removal() {
        let temp = TempDir::new().unwrap();
        let code_home = temp.path();

        let repo_dir = temp.path().join("repo");
        fs::create_dir_all(&repo_dir).unwrap();

        run_git(&repo_dir, ["init"]).unwrap();
        fs::write(repo_dir.join("README.md"), b"hello").unwrap();
        run_git(&repo_dir, ["add", "."]).unwrap();
        run_git(
            &repo_dir,
            [
                "-c",
                "user.name=code",
                "-c",
                "user.email=code@example.com",
                "commit",
                "-m",
                "init",
            ],
        )
        .unwrap();

        let worktree_path = code_home.join("working/repo/branches/code-branch-test");
        fs::create_dir_all(worktree_path.parent().unwrap()).unwrap();
        run_git(&repo_dir, ["worktree", "add", "-b", "code-branch-test", worktree_path.to_str().unwrap()])
            .unwrap();

        assert!(branch_exists(&repo_dir, "code-branch-test"));

        let config = HousekeepingConfig {
            session_retention_days: None,
            worktree_retention_days: Some(0),
            min_interval_hours: 1,
            disabled: false,
        };

        let now = datetime!(2025-10-10 12:00:00 UTC);
        let outcome = perform_housekeeping(code_home, now, &config).unwrap();

        assert_eq!(outcome.worktrees_removed, 1);
        assert!(!worktree_path.exists());
        assert!(!branch_exists(&repo_dir, "code-branch-test"));
    }

    fn run_git(repo_root: &Path, args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>) -> io::Result<()> {
        let output = std::process::Command::new("git")
            .current_dir(repo_root)
            .args(args)
            .output()?;
        if output.status.success() {
            Ok(())
        } else {
            Err(io::Error::other(String::from_utf8_lossy(&output.stderr).trim().to_string()))
        }
    }

    fn branch_exists(repo_root: &Path, branch_name: &str) -> bool {
        let output = std::process::Command::new("git")
            .current_dir(repo_root)
            .args(["show-ref", "--verify", "--quiet", &format!("refs/heads/{branch_name}")])
            .status();
        output.map(|status| status.success()).unwrap_or(false)
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn drops_registry_entries_for_dead_pids() {
        let temp = TempDir::new().unwrap();
        let session_dir = temp.path().join("working/_session");
        fs::create_dir_all(&session_dir).unwrap();
        let registry_path = session_dir.join("pid-999999.txt");
        fs::write(&registry_path, "/tmp/repo\t/tmp/worktree\n").unwrap();

        let active = super::collect_active_worktrees(&session_dir);

        assert!(active.is_empty());
        assert!(!registry_path.exists());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn process_still_running_reports_status() {
        use std::process::{Command, Stdio};
        use std::thread;
        use std::time::Duration;

        let mut cmd = Command::new("cmd");
        cmd.args(["/C", "timeout /T 2 >NUL"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = crate::spawn::spawn_std_command_with_retry(&mut cmd)
            .expect("failed to spawn cmd timeout helper");

        thread::sleep(Duration::from_millis(100));

        let pid = child.id() as i32;
        assert_eq!(check_pid_alive(pid), Some(true));

        child.wait().expect("failed to wait for cmd child");

        assert_eq!(check_pid_alive(pid), Some(false));
    }
}
