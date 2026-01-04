#[cfg(target_os = "linux")]
use std::path::{Path, PathBuf};

#[cfg(target_os = "linux")]
const CGROUP_MOUNT: &str = "/sys/fs/cgroup";

#[cfg(target_os = "linux")]
const EXEC_CGROUP_SUBDIR: &str = "code-exec";

#[cfg(target_os = "linux")]
const EXEC_CGROUP_OOM_SCORE_ADJ: &str = "500";

#[cfg(target_os = "linux")]
pub(crate) fn default_exec_memory_max_bytes() -> Option<u64> {
    if let Ok(raw) = std::env::var("CODEX_EXEC_MEMORY_MAX_BYTES") {
        if let Ok(value) = raw.trim().parse::<u64>() {
            if value > 0 {
                return Some(value);
            }
        }
    }
    if let Ok(raw) = std::env::var("CODEX_EXEC_MEMORY_MAX_MB") {
        if let Ok(value) = raw.trim().parse::<u64>() {
            if value > 0 {
                return Some(value.saturating_mul(1024 * 1024));
            }
        }
    }

    let available = read_mem_available_bytes()?;
    // Leave headroom for the parent TUI + other background processes.
    // Keep the cap within a reasonable range so we still protect the parent
    // on larger machines.
    let sixty_percent = available.saturating_mul(60) / 100;
    let min = 512_u64.saturating_mul(1024 * 1024);
    let max = 4_u64.saturating_mul(1024 * 1024 * 1024);
    Some(sixty_percent.clamp(min, max))
}

#[cfg(target_os = "linux")]
fn read_mem_available_bytes() -> Option<u64> {
    let contents = std::fs::read_to_string("/proc/meminfo").ok()?;
    for line in contents.lines() {
        let line = line.trim_start();
        if let Some(rest) = line.strip_prefix("MemAvailable:") {
            let kb = rest
                .split_whitespace()
                .next()
                .and_then(|n| n.parse::<u64>().ok())?;
            return Some(kb.saturating_mul(1024));
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn is_cgroup_v2() -> bool {
    std::fs::metadata(Path::new(CGROUP_MOUNT).join("cgroup.controllers")).is_ok()
}

#[cfg(target_os = "linux")]
fn current_cgroup_relative() -> Option<PathBuf> {
    let contents = std::fs::read_to_string("/proc/self/cgroup").ok()?;
    for line in contents.lines() {
        if let Some(path) = line.strip_prefix("0::") {
            let trimmed = path.trim();
            if trimmed.is_empty() {
                return None;
            }
            return Some(PathBuf::from(trimmed.trim_start_matches('/')));
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn exec_cgroup_parent_abs() -> Option<PathBuf> {
    if !is_cgroup_v2() {
        return None;
    }
    let rel = current_cgroup_relative()?;
    Some(Path::new(CGROUP_MOUNT).join(rel).join(EXEC_CGROUP_SUBDIR))
}

#[cfg(target_os = "linux")]
pub(crate) fn exec_cgroup_abs_for_pid(pid: u32) -> Option<PathBuf> {
    exec_cgroup_parent_abs().map(|parent| parent.join(format!("pid-{pid}")))
}

#[cfg(target_os = "linux")]
fn best_effort_enable_memory_controller(parent: &Path) {
    let controllers = std::fs::read_to_string(parent.join("cgroup.controllers")).ok();
    if controllers.as_deref().unwrap_or_default().split_whitespace().all(|c| c != "memory") {
        return;
    }
    let subtree = parent.join("cgroup.subtree_control");
    let _ = std::fs::write(subtree, "+memory");
}

#[cfg(target_os = "linux")]
pub(crate) fn best_effort_attach_self_to_exec_cgroup(pid: u32, memory_max_bytes: u64) {
    let Some(parent) = exec_cgroup_parent_abs() else {
        return;
    };

    let _ = std::fs::create_dir_all(&parent);
    best_effort_enable_memory_controller(&parent);

    let cgroup_dir = parent.join(format!("pid-{pid}"));
    if std::fs::create_dir_all(&cgroup_dir).is_err() {
        return;
    }

    let memory_max_path = cgroup_dir.join("memory.max");
    if memory_max_path.exists() {
        let _ = std::fs::write(&memory_max_path, memory_max_bytes.to_string());
    } else {
        // Memory controller not active for this subtree.
        return;
    }

    let oom_group_path = cgroup_dir.join("memory.oom.group");
    if oom_group_path.exists() {
        let _ = std::fs::write(oom_group_path, "1");
    }

    // Prefer killing the exec subtree first if the host does hit global OOM.
    let _ = std::fs::write("/proc/self/oom_score_adj", EXEC_CGROUP_OOM_SCORE_ADJ);

    let procs_path = cgroup_dir.join("cgroup.procs");
    if procs_path.exists() {
        let _ = std::fs::write(procs_path, pid.to_string());
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn exec_cgroup_oom_killed(pid: u32) -> Option<bool> {
    let dir = exec_cgroup_abs_for_pid(pid)?;
    let contents = std::fs::read_to_string(dir.join("memory.events")).ok()?;
    for line in contents.lines() {
        let mut parts = line.split_whitespace();
        let Some(key) = parts.next() else {
            continue;
        };
        let Some(val) = parts.next() else {
            continue;
        };
        if key == "oom_kill" {
            if let Ok(parsed) = val.parse::<u64>() {
                return Some(parsed > 0);
            }
        }
    }
    None
}

#[cfg(target_os = "linux")]
pub(crate) fn exec_cgroup_memory_max_bytes(pid: u32) -> Option<u64> {
    let dir = exec_cgroup_abs_for_pid(pid)?;
    let raw = std::fs::read_to_string(dir.join("memory.max")).ok()?;
    let trimmed = raw.trim();
    if trimmed == "max" {
        return None;
    }
    trimmed.parse::<u64>().ok()
}

#[cfg(target_os = "linux")]
pub(crate) fn best_effort_cleanup_exec_cgroup(pid: u32) {
    let Some(dir) = exec_cgroup_abs_for_pid(pid) else {
        return;
    };
    // Only remove the per-pid directory. The parent container stays.
    let _ = std::fs::remove_dir(&dir);
}
