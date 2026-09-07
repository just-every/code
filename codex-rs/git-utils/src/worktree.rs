//! Resolves repository identity and sibling checkouts from validated Git metadata.
//! Administrative links must agree before a checkout participates in discovery.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use codex_utils_absolute_path::AbsolutePathBuf;

use crate::get_git_repo_root;

/// The stable on-disk identity shared by a repository and its linked worktrees.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RepositoryIdentity {
    /// Canonical shared Git administrative directory.
    pub common_dir: AbsolutePathBuf,
    /// Directory within the current checkout, preserved across linked worktrees.
    pub relative_cwd: PathBuf,
    /// Canonical root of the repository's primary checkout.
    pub primary_root: AbsolutePathBuf,
}

fn canonicalize_native(path: impl AsRef<Path>) -> Option<PathBuf> {
    AbsolutePathBuf::from_absolute_path(path)
        .ok()?
        .canonicalize()
        .ok()
        .map(AbsolutePathBuf::into_path_buf)
}

/// Identifies a checkout without executing Git or trusting unchecked administrative links.
pub fn repository_identity(cwd: &Path) -> Option<RepositoryIdentity> {
    let canonical_cwd = canonicalize_native(cwd)?;
    if !canonical_cwd.is_dir() {
        return None;
    }

    let checkout_root = canonicalize_native(get_git_repo_root(&canonical_cwd)?)?;
    let relative_cwd = canonical_cwd
        .strip_prefix(&checkout_root)
        .ok()?
        .to_path_buf();
    let git_entry = checkout_root.join(".git");
    let entry_type = fs::symlink_metadata(&git_entry).ok()?.file_type();
    if entry_type.is_symlink() {
        return None;
    }

    let common_dir = if entry_type.is_dir() {
        canonicalize_native(&git_entry)?
    } else if entry_type.is_file() {
        let git_dir = read_git_path(&git_entry, &checkout_root, "gitdir:")?;
        if !git_dir.is_dir() {
            return None;
        }
        let common_dir = read_git_path(&git_dir.join("commondir"), &git_dir, "")?;
        if !common_dir.is_dir() {
            return None;
        }
        let registered_root = canonicalize_native(common_dir.join("worktrees"))?;
        if git_dir.parent()? != registered_root {
            return None;
        }
        let backlink = read_git_path(&git_dir.join("gitdir"), &git_dir, "")?;
        if backlink != canonicalize_native(&git_entry)? {
            return None;
        }
        common_dir
    } else {
        return None;
    };

    let primary_root = common_dir.parent()?.to_path_buf();
    let primary_git_entry = primary_root.join(".git");
    let primary_entry_type = fs::symlink_metadata(&primary_git_entry).ok()?.file_type();
    if !primary_entry_type.is_dir()
        || primary_entry_type.is_symlink()
        || canonicalize_native(primary_git_entry)? != common_dir
    {
        return None;
    }

    Some(RepositoryIdentity {
        common_dir: AbsolutePathBuf::from_absolute_path_checked(common_dir).ok()?,
        relative_cwd,
        primary_root: AbsolutePathBuf::from_absolute_path_checked(primary_root).ok()?,
    })
}

/// Returns corresponding existing directories in the current, primary, and linked checkouts.
pub fn linked_worktree_cwds(cwd: &Path) -> Option<Vec<PathBuf>> {
    let identity = repository_identity(cwd)?;
    let current_cwd = canonicalize_native(cwd)?;
    let mut result = vec![cwd.to_path_buf()];
    let mut seen = HashSet::from([cwd.to_path_buf()]);
    if seen.insert(current_cwd.clone()) {
        result.push(current_cwd);
    }

    append_linked_cwd(&mut result, &mut seen, &identity.primary_root, &identity);

    let worktrees = identity.common_dir.join("worktrees");
    if !worktrees.exists() {
        return Some(result);
    }
    let worktrees = canonicalize_native(worktrees)?;
    let mut registered: Vec<_> = fs::read_dir(&worktrees)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|file_type| file_type.is_dir()))
        .collect();
    registered.sort_by_key(std::fs::DirEntry::file_name);

    for entry in registered {
        let Some(git_dir) = canonicalize_native(entry.path()) else {
            continue;
        };
        if git_dir.parent() != Some(worktrees.as_path()) {
            continue;
        }
        let Some(git_file) = read_git_path(&git_dir.join("gitdir"), &git_dir, "") else {
            continue;
        };
        if git_file.file_name() != Some(OsStr::new(".git")) {
            continue;
        }
        let Some(checkout_root) = git_file.parent() else {
            continue;
        };
        append_linked_cwd(&mut result, &mut seen, checkout_root, &identity);
    }

    Some(result)
}

fn append_linked_cwd(
    result: &mut Vec<PathBuf>,
    seen: &mut HashSet<PathBuf>,
    checkout_root: &Path,
    identity: &RepositoryIdentity,
) {
    let Some(checkout_root) = canonicalize_native(checkout_root) else {
        return;
    };
    let Some(candidate) = canonicalize_native(checkout_root.join(&identity.relative_cwd)) else {
        return;
    };
    if !candidate.is_dir() || !candidate.starts_with(&checkout_root) {
        return;
    }
    let Some(candidate_identity) = repository_identity(&candidate) else {
        return;
    };
    if candidate_identity.common_dir == identity.common_dir
        && candidate_identity.relative_cwd == identity.relative_cwd
        && seen.insert(candidate.clone())
    {
        result.push(candidate);
    }
}

fn read_git_path(path: &Path, relative_to: &Path, prefix: &str) -> Option<PathBuf> {
    if !fs::symlink_metadata(path).ok()?.file_type().is_file() {
        return None;
    }
    let contents = fs::read_to_string(path).ok()?;
    let value = contents.trim().strip_prefix(prefix)?.trim();
    if value.is_empty() || value.contains('\n') || value.contains('\r') {
        return None;
    }
    canonicalize_native(relative_to.join(value))
}

#[cfg(test)]
#[path = "worktree_tests.rs"]
mod tests;
