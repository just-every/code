use crate::config_types::GithubConfig;
use codex_apply_patch::{ApplyPatchAction, ApplyPatchFileChange};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn is_workflow_path(path: &Path, cwd: &Path) -> bool {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    let mut saw_dot_github = false;
    let mut saw_workflows = false;
    for component in absolute.components() {
        let segment = component.as_os_str().to_string_lossy();
        if !saw_dot_github && segment == ".github" {
            saw_dot_github = true;
            continue;
        }
        if saw_dot_github && !saw_workflows && segment == "workflows" {
            saw_workflows = true;
            continue;
        }
        if saw_dot_github && saw_workflows {
            return matches!(
                absolute.extension().and_then(|ext| ext.to_str()),
                Some("yml" | "yaml")
            );
        }
    }
    false
}

fn is_in_github_dir(path: &Path, cwd: &Path) -> bool {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    if let Ok(relative) = absolute.strip_prefix(cwd) {
        return relative.starts_with(".github");
    }
    false
}

/// Run `actionlint` against a temporary workspace containing the proposed
/// workflow changes. Returns diagnostic lines when the tool is available and
/// produced output.
pub fn maybe_run_actionlint(
    action: &ApplyPatchAction,
    cwd: &Path,
    config: &GithubConfig,
) -> Option<Vec<String>> {
    if !config.actionlint_on_patch {
        return None;
    }

    let exe = config
        .actionlint_path
        .clone()
        .unwrap_or_else(|| PathBuf::from("actionlint"));
    let executable = if exe.is_absolute() {
        exe.exists().then_some(exe)
    } else {
        which(&exe)
    };
    let Some(executable) = executable else {
        return None;
    };

    let touches_workflow = action
        .changes()
        .keys()
        .any(|path| is_workflow_path(path, cwd));
    if !touches_workflow {
        return None;
    }

    let temp = TempDir::new().ok()?;
    let temp_root = temp.path();
    let temp_github = temp_root.join(".github");
    let temp_workflows = temp_github.join("workflows");
    let _ = fs::create_dir_all(&temp_workflows);

    let source_github = cwd.join(".github");
    if source_github.exists() {
        let _ = copy_dir_recursive(&source_github, &temp_github);
    }

    for (path, change) in action.changes() {
        let path_in_github = is_in_github_dir(path, cwd);
        match change {
            ApplyPatchFileChange::Add { content } => {
                if path_in_github {
                    let _ = stage_file_with_contents(temp_root, cwd, path, content);
                }
            }
            ApplyPatchFileChange::Update {
                new_content,
                move_path,
                ..
            } => {
                let dest_path = move_path.as_ref().unwrap_or(path);
                let dest_in_github = is_in_github_dir(dest_path, cwd);
                if !dest_in_github && !path_in_github {
                    continue;
                }

                if dest_in_github {
                    let _ = stage_file_with_contents(temp_root, cwd, dest_path, new_content);
                }

                if move_path.is_none() && path_in_github && !dest_in_github {
                    let _ = stage_file_with_contents(temp_root, cwd, path, new_content);
                }

                if path_in_github && move_path.is_some() {
                    remove_from_staged(temp_root, cwd, path);
                }
            }
            ApplyPatchFileChange::Delete { .. } => {
                if path_in_github {
                    remove_from_staged(temp_root, cwd, path);
                }
            }
        }
    }

    let output = std::process::Command::new(executable)
        .arg("-color")
        .arg("never")
        .current_dir(temp_root)
        .output()
        .ok()?;

    let mut lines: Vec<String> = Vec::new();
    if !output.stdout.is_empty() {
        lines.extend(
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(|s| s.to_string()),
        );
    }
    if !output.stderr.is_empty() {
        lines.extend(
            String::from_utf8_lossy(&output.stderr)
                .lines()
                .map(|s| s.to_string()),
        );
    }
    if lines.is_empty() { None } else { Some(lines) }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else if file_type.is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            let _ = fs::copy(entry.path(), &target);
        } else if file_type.is_symlink() {
            copy_symlink(&entry.path(), &target)?;
        }
    }
    Ok(())
}

fn which(exe: &Path) -> Option<PathBuf> {
    let name = exe.as_os_str();
    let paths: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect())
        .unwrap_or_else(Vec::new);
    for dir in paths {
        let candidate = dir.join(name);
        if candidate.is_file() && is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .map(|meta| meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

fn stage_file_with_contents(temp_root: &Path, cwd: &Path, target: &Path, contents: &str) -> bool {
    let relative = match target.strip_prefix(cwd) {
        Ok(rel) => rel,
        Err(_) => return false,
    };
    let destination = temp_root.join(relative);
    if let Some(parent) = destination.parent() {
        if fs::create_dir_all(parent).is_err() {
            return false;
        }
    }
    match fs::File::create(&destination) {
        Ok(mut file) => file.write_all(contents.as_bytes()).is_ok(),
        Err(_) => false,
    }
}

fn remove_from_staged(temp_root: &Path, cwd: &Path, target: &Path) {
    if let Ok(relative) = target.strip_prefix(cwd) {
        let destination = temp_root.join(relative);
        let _ = fs::remove_file(&destination);
    }
}

fn copy_symlink(src: &Path, dst: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let target = std::fs::read_link(src)?;
        let _ = fs::remove_file(dst);
        std::os::unix::fs::symlink(target, dst)?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        let _ = fs::copy(src, dst)?;
        Ok(())
    }
}
