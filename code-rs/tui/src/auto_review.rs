use std::path::{Path, PathBuf};

use tokio::process::Command;

use code_core::git_worktree::{
    cleanup_review_worktree,
    detect_default_branch,
    get_git_root_from,
    setup_review_worktree,
    ReviewWorktreeCleanupToken,
};
use code_git_tooling::{create_ghost_commit, CreateGhostCommitOptions, GhostCommit, GitToolingError};

/// Owned snapshot of the base ghost commit captured before a turn begins.
#[derive(Debug, Clone)]
pub struct BaseCommitSnapshot {
    pub id: String,
    pub parent: Option<String>,
}

impl BaseCommitSnapshot {
    fn as_ghost(&self) -> GhostCommit {
        GhostCommit::new(self.id.clone(), self.parent.clone())
    }
}

/// Data required to launch a commit-scoped review.
#[derive(Debug)]
pub struct PreparedCommitReview {
    pub commit_sha: String,
    pub short_sha: String,
    pub file_count: usize,
    pub subject: Option<String>,
    pub worktree_path: PathBuf,
    pub cleanup: ReviewWorktreeCleanupToken,
    pub base_branch: Option<String>,
    pub current_branch: Option<String>,
}

#[derive(Debug)]
pub enum AutoReviewOutcome {
    Skip { reason: String },
    Commit(PreparedCommitReview),
    Error(String),
}

/// Prepare a commit review by capturing a snapshot, diffing against the turn base, and
/// provisioning a detached worktree pointed at the resulting commit.
pub async fn prepare_commit_review(
    repo_root: &Path,
    base_commit: Option<BaseCommitSnapshot>,
    name_hint: Option<&str>,
) -> AutoReviewOutcome {
    let git_root = match get_git_root_from(repo_root).await {
        Ok(root) => root,
        Err(err) => {
            return AutoReviewOutcome::Skip {
                reason: format!("Not a git repository: {err}"),
            }
        }
    };

    let base_ghost = base_commit.as_ref().map(BaseCommitSnapshot::as_ghost);
    let base_ghost_ref = base_ghost.as_ref();

    let final_commit = match capture_commit(repo_root, base_ghost_ref).await {
        Ok(commit) => commit,
        Err(err) => {
            return AutoReviewOutcome::Error(format!("Failed to capture workspace snapshot: {err}"));
        }
    };

    let diff_count = match count_changed_paths(&git_root, base_ghost_ref, &final_commit).await {
        Ok(count) => count,
        Err(err) => {
            return AutoReviewOutcome::Error(format!("Failed to diff commit: {err}"));
        }
    };

    if diff_count == 0 {
        return AutoReviewOutcome::Skip {
            reason: "Auto Drive turn produced no file changes".to_string(),
        };
    }

    let (worktree_path, cleanup) = match setup_review_worktree(&git_root, final_commit.id(), name_hint).await {
        Ok(value) => value,
        Err(err) => {
            return AutoReviewOutcome::Error(format!("Failed to provision review worktree: {err}"));
        }
    };

    let subject = match read_commit_subject(&git_root, final_commit.id()).await {
        Ok(value) => value,
        Err(err) => {
            let _ = cleanup_review_worktree(cleanup).await;
            return AutoReviewOutcome::Error(format!("Failed to read commit subject: {err}"));
        }
    };

    let base_branch = detect_default_branch(&git_root).await;
    let current_branch = match read_current_branch(&git_root).await {
        Ok(branch) => branch,
        Err(_) => None,
    };

    let prepared = PreparedCommitReview {
        commit_sha: final_commit.id().to_string(),
        short_sha: final_commit.id()[..final_commit.id().len().min(8)].to_string(),
        file_count: diff_count,
        subject,
        worktree_path,
        cleanup,
        base_branch,
        current_branch,
    };

    AutoReviewOutcome::Commit(prepared)
}

async fn capture_commit(
    repo_root: &Path,
    base_commit: Option<&GhostCommit>,
) -> Result<GhostCommit, GitToolingError> {
    let mut options = CreateGhostCommitOptions::new(repo_root).message("Auto Drive turn snapshot");
    if let Some(parent) = base_commit {
        options = options.parent(parent.id());
    }
    create_ghost_commit(&options)
}

async fn count_changed_paths(
    git_root: &Path,
    base_commit: Option<&GhostCommit>,
    final_commit: &GhostCommit,
) -> Result<usize, String> {
    let mut args: Vec<String> = Vec::new();
    if let Some(base) = base_commit {
        args.extend([
            "diff".to_string(),
            "--name-only".to_string(),
            format!("{}..{}", base.id(), final_commit.id()),
        ]);
    } else {
        args.extend([
            "diff-tree".to_string(),
            "--no-commit-id".to_string(),
            "--name-only".to_string(),
            "-r".to_string(),
            final_commit.id().to_string(),
        ]);
    }

    let output = Command::new("git")
        .current_dir(git_root)
        .args(&args)
        .output()
        .await
        .map_err(|e| format!("git {:?} failed: {e}", args))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git {:?} failed: {}", args, stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count())
}

async fn read_commit_subject(git_root: &Path, commit: &str) -> Result<Option<String>, String> {
    let output = Command::new("git")
        .current_dir(git_root)
        .args(["show", "-s", "--format=%s", commit])
        .output()
        .await
        .map_err(|e| format!("git show failed: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(stderr.trim().to_string());
    }

    let subject = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if subject.is_empty() {
        Ok(None)
    } else {
        Ok(Some(subject))
    }
}

async fn read_current_branch(git_root: &Path) -> Result<Option<String>, String> {
    let output = Command::new("git")
        .current_dir(git_root)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .await
        .map_err(|e| format!("git rev-parse failed: {e}"))?;

    if !output.status.success() {
        return Ok(None);
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch == "HEAD" || branch.is_empty() {
        Ok(None)
    } else {
        Ok(Some(branch))
    }
}
