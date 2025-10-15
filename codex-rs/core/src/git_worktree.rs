use chrono::Utc;
use std::path::{Path, PathBuf};
use tokio::fs::OpenOptions;
use tokio::process::Command;

/// Sanitize a string to be used as a single git refname component.
///
/// Converts to lowercase, keeps [a-z0-9-], collapses other runs into '-'.
pub fn sanitize_ref_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_dash = false;
    for ch in s.chars() {
        let c = ch.to_ascii_lowercase();
        let valid = c.is_ascii_alphanumeric() || c == '-';
        if valid {
            out.push(c);
            last_dash = c == '-';
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "branch".to_string()
    } else {
        out
    }
}

/// Generate a branch name for a generic task. If `task` is None or cannot
/// produce a meaningful slug, fall back to a timestamp.
pub fn generate_branch_name_from_task(task: Option<&str>) -> String {
    if let Some(task) = task {
        let stop = ["the", "and", "for", "with", "from", "into", "goal"];
        let words: Vec<&str> = task
            .split_whitespace()
            .filter(|w| w.len() > 2 && !stop.contains(&w.to_ascii_lowercase().as_str()))
            .take(4)
            .collect();
        if !words.is_empty() {
            let mut slug = sanitize_ref_component(&words.join("-"));
            if slug.len() > 48 {
                slug.truncate(48);
                slug = slug.trim_matches('-').to_string();
                if slug.is_empty() {
                    slug = "branch".to_string();
                }
            }
            return format!("code-branch-{}", slug);
        }
    }
    // Fallback: timestamped id
    let ts = Utc::now().format("%Y%m%d-%H%M%S");
    format!("code-branch-{}", ts)
}

/// Resolve the git repository root (top-level) for the given cwd.
pub async fn get_git_root_from(cwd: &Path) -> Result<PathBuf, String> {
    let output = Command::new("git")
        .arg("rev-parse")
        .arg("--show-toplevel")
        .current_dir(cwd)
        .output()
        .await
        .map_err(|e| format!("Git not installed or not in a git repository: {}", e))?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(PathBuf::from(path))
    } else {
        Err("Not in a git repository".to_string())
    }
}

/// Create a new worktree for `branch_id` under `<git_root>/.code/branches/<branch_id>`.
/// If a previous worktree directory exists, remove it first.
pub async fn setup_worktree(git_root: &Path, branch_id: &str) -> Result<(PathBuf, String), String> {
    // Global location: ~/.code/working/<repo_name>/branches
    let repo_name = git_root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("repo");
    let mut code_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    code_dir = code_dir
        .join(".code")
        .join("working")
        .join(repo_name)
        .join("branches");
    tokio::fs::create_dir_all(&code_dir)
        .await
        .map_err(|e| format!("Failed to create .code/branches directory: {}", e))?;

    let mut effective_branch = branch_id.to_string();
    let mut worktree_path = code_dir.join(&effective_branch);
    if worktree_path.exists() {
        // If the worktree directory already exists, re-use it to avoid the cost
        // of removing and re-adding a worktree. This makes repeated agent runs
        // start much faster.
        record_worktree_in_session(git_root, &worktree_path).await;
        return Ok((worktree_path, effective_branch));
    }

    let output = Command::new("git")
        .current_dir(git_root)
        .args([
            "worktree",
            "add",
            "-b",
            &effective_branch,
            worktree_path.to_str().unwrap(),
        ])
        .output()
        .await
        .map_err(|e| format!("Failed to create git worktree: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // If the branch already exists, generate a unique name and retry once.
        if stderr.contains("already exists") {
            effective_branch = format!(
                "{}-{}",
                effective_branch,
                Utc::now().format("%Y%m%d-%H%M%S")
            );
            worktree_path = code_dir.join(&effective_branch);
            // Ensure target path is clean
            if worktree_path.exists() {
                let _ = Command::new("git")
                    .arg("worktree")
                    .arg("remove")
                    .arg(worktree_path.to_str().unwrap())
                    .arg("--force")
                    .current_dir(git_root)
                    .output()
                    .await;
            }
            let retry = Command::new("git")
                .current_dir(git_root)
                .args([
                    "worktree",
                    "add",
                    "-b",
                    &effective_branch,
                    worktree_path.to_str().unwrap(),
                ])
                .output()
                .await
                .map_err(|e| format!("Failed to create git worktree (retry): {}", e))?;
            if !retry.status.success() {
                let retry_err = String::from_utf8_lossy(&retry.stderr);
                return Err(format!("Failed to create worktree: {}", retry_err));
            }
            record_worktree_in_session(git_root, &worktree_path).await;
        } else {
            return Err(format!("Failed to create worktree: {}", stderr));
        }
    }

    // Skip remote alias setup for speed; we don't need it during agent runs.

    // Record created worktree for this process; best-effort.
    record_worktree_in_session(git_root, &worktree_path).await;

    Ok((worktree_path, effective_branch))
}

/// Append the created worktree to a per-process session file so the TUI can
/// clean it up on exit without touching worktrees from other processes.
async fn record_worktree_in_session(git_root: &Path, worktree_path: &Path) {
    let pid = std::process::id();
    let mut base = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    // Global session registry: ~/.code/working/_session
    base = base.join(".code").join("working").join("_session");
    if let Err(_e) = tokio::fs::create_dir_all(&base).await {
        return;
    }
    let file = base.join(format!("pid-{}.txt", pid));
    // Store git_root and worktree_path separated by a tab; one entry per line.
    if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file)
        .await
    {
        let line = format!("{}\t{}\n", git_root.display(), worktree_path.display());
        let _ = tokio::io::AsyncWriteExt::write_all(&mut f, line.as_bytes()).await;
    }
}

/// Ensure a remote named `origin` exists. If it's missing, choose a likely
/// writable remote (prefer `fork`, then `upstream-push`, then the first push
/// URL we find) and alias it as `origin`. Finally, set the remote HEAD so
/// `origin/HEAD` points at the default branch.
async fn _ensure_origin_remote(git_root: &Path) -> Result<(), String> {
    // Check existing remotes
    let remotes_out = Command::new("git")
        .current_dir(git_root)
        .args(["remote"])
        .output()
        .await
        .map_err(|e| format!("git remote failed: {}", e))?;
    if !remotes_out.status.success() {
        return Err("git remote returned error".to_string());
    }
    let remotes: Vec<String> = String::from_utf8_lossy(&remotes_out.stdout)
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if remotes.iter().any(|r| r == "origin") {
        // Make sure origin/HEAD is set; ignore errors
        let _ = Command::new("git")
            .current_dir(git_root)
            .args(["remote", "set-head", "origin", "-a"])
            .output()
            .await;
        return Ok(());
    }

    // Prefer candidates in this order
    let mut candidates = vec!["fork", "upstream-push", "upstream"]; // typical setups
    // Append any other remotes as fallbacks
    for r in &remotes {
        if !candidates.contains(&r.as_str()) {
            candidates.push(r);
        }
    }

    // Find a candidate with a URL
    for cand in candidates {
        let url_out = Command::new("git")
            .current_dir(git_root)
            .args(["remote", "get-url", cand])
            .output()
            .await;
        if let Ok(out) = url_out {
            if out.status.success() {
                let url = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !url.is_empty() {
                    // Add origin pointing to this URL
                    let add = Command::new("git")
                        .current_dir(git_root)
                        .args(["remote", "add", "origin", &url])
                        .output()
                        .await
                        .map_err(|e| format!("git remote add origin failed: {}", e))?;
                    if !add.status.success() {
                        return Err("failed to add origin".to_string());
                    }
                    let _ = Command::new("git")
                        .current_dir(git_root)
                        .args(["remote", "set-head", "origin", "-a"])
                        .output()
                        .await;
                    return Ok(());
                }
            }
        }
    }
    // No usable remote found; leave as-is
    Err("no suitable remote to alias as origin".to_string())
}

/// Copy uncommitted (modified + untracked) files from `src_root` into the `worktree_path`.
/// Returns the number of files copied.
pub async fn copy_uncommitted_to_worktree(
    src_root: &Path,
    worktree_path: &Path,
) -> Result<usize, String> {
    // List modified and other (untracked) files relative to repo root
    let output = Command::new("git")
        .current_dir(src_root)
        .args(["ls-files", "-om", "--exclude-standard", "-z"])
        .output()
        .await
        .map_err(|e| format!("Failed to list changes: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git ls-files failed: {}", stderr));
    }
    let bytes = output.stdout;
    let mut count = 0usize;
    for path_bytes in bytes.split(|b| *b == 0) {
        if path_bytes.is_empty() {
            continue;
        }
        let rel = match String::from_utf8(path_bytes.to_vec()) {
            Ok(s) => s,
            Err(_) => continue,
        };
        // Avoid copying .git files explicitly
        if rel.starts_with(".git/") {
            continue;
        }
        let from = src_root.join(&rel);
        let to = worktree_path.join(&rel);
        let meta = match tokio::fs::metadata(&from).await {
            Ok(m) => m,
            Err(_) => continue,
        };
        if !meta.is_file() {
            continue;
        }
        if let Some(parent) = to.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("Failed to create dir {}: {}", parent.display(), e))?;
        }
        // Use copy for files; skip if it's a directory (shouldn't appear from ls-files)
        match tokio::fs::copy(&from, &to).await {
            Ok(_) => count += 1,
            Err(e) => {
                return Err(format!(
                    "Failed to copy {} -> {}: {}",
                    from.display(),
                    to.display(),
                    e
                ));
            }
        }
    }

    // Opt-in: mirror modified submodule pointers into the worktree index (no checkout/network).
    // Enable via CODEX_BRANCH_INCLUDE_SUBMODULES=1|true|yes.
    let include_submods = std::env::var("CODEX_BRANCH_INCLUDE_SUBMODULES")
        .ok()
        .map(|v| v.to_ascii_lowercase())
        .map(|v| v == "1" || v == "true" || v == "yes")
        .unwrap_or(false);
    if include_submods {
        if let Ok(out) = Command::new("git")
            .current_dir(src_root)
            .args(["submodule", "status", "--recursive"])
            .output()
            .await
        {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout);
                for line in text.lines() {
                    let line = line.trim();
                    if !line.starts_with('+') {
                        continue;
                    }
                    let rest = &line[1..];
                    let mut parts = rest.split_whitespace();
                    let sha = match parts.next() {
                        Some(s) => s,
                        None => continue,
                    };
                    let path = match parts.next() {
                        Some(p) => p,
                        None => continue,
                    };
                    let spec = format!("160000,{},{}", sha, path);
                    let _ = Command::new("git")
                        .current_dir(worktree_path)
                        .args(["update-index", "--add", "--cacheinfo", &spec])
                        .output()
                        .await;
                }
            }
        }
    }
    Ok(count)
}

/// Determine repository default branch. Prefers `origin/HEAD` symbolic ref, then local `main`/`master`.
pub async fn detect_default_branch(cwd: &Path) -> Option<String> {
    // Try origin/HEAD first
    let sym = Command::new("git")
        .current_dir(cwd)
        .args(["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"])
        .output()
        .await
        .ok()?;
    if sym.status.success() {
        if let Ok(s) = String::from_utf8(sym.stdout) {
            if let Some((_, name)) = s.trim().rsplit_once('/') {
                return Some(name.to_string());
            }
        }
    }
    // Fallback to local main/master
    for candidate in ["main", "master"] {
        let out = Command::new("git")
            .current_dir(cwd)
            .args([
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/{candidate}"),
            ])
            .output()
            .await
            .ok()?;
        if out.status.success() {
            return Some(candidate.to_string());
        }
    }
    None
}
