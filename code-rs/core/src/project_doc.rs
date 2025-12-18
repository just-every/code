//! Project-level documentation discovery.
//!
//! Project-level documentation can be stored in files named `AGENTS.md` and
//! Auto Drive can use specialised guidance stored in `AUTO_AGENTS.md`.
//! We include the concatenation of all files found along the path from the
//! repository root to the current working directory as follows:
//!
//! 1.  Determine the Git repository root by walking upwards from the current
//!     working directory until a `.git` directory or file is found. If no Git
//!     root is found, only the current working directory is considered.
//! 2.  Collect every `AGENTS.md` found from the repository root down to the
//!     current working directory (inclusive) and concatenate their contents in
//!     that order.
//! 3.  We do **not** walk past the Git root.

use crate::config::Config;
use crate::skills::SkillMetadata;
use crate::skills::render_skills_section;
use std::collections::HashSet;
use std::path::PathBuf;
use tokio::io::AsyncReadExt;
use tracing::error;

/// Filenames recognised for standard agent instructions.
const AGENT_FILENAMES: &[&str] = &["AGENTS.md"];

/// Filenames recognised for Auto Drive instructions.
const AUTO_AGENT_FILENAMES: &[&str] = &["AUTO_AGENTS.md"];

/// When both `Config::instructions` and the project doc are present, they will
/// be concatenated with the following separator.
const PROJECT_DOC_SEPARATOR: &str = "\n\n--- project-doc ---\n\n";

/// Combines `Config::instructions` and `AGENTS.md` (if present) into a single
/// string of instructions.
pub(crate) async fn get_user_instructions(
    config: &Config,
    skills: Option<&[SkillMetadata]>,
) -> Option<String> {
    let skills_section = skills.and_then(render_skills_section);

    let project_doc_parts = match read_project_doc_parts(config).await {
        Ok(Some(parts)) => parts,
        Ok(None) => Vec::new(),
        Err(e) => {
            error!("error trying to find project doc: {e:#}");
            Vec::new()
        }
    };

    let mut seen: HashSet<String> = HashSet::new();

    let mut base_instructions: Option<String> = None;
    if let Some(original) = &config.user_instructions {
        let key = original.trim();
        if !key.is_empty() && seen.insert(key.to_string()) {
            base_instructions = Some(original.clone());
        }
    }

    let mut unique_project_docs: Vec<String> = Vec::new();
    for doc in project_doc_parts {
        let key = doc.trim();
        if key.is_empty() {
            continue;
        }

        if seen.insert(key.to_string()) {
            unique_project_docs.push(doc);
        }
    }

    if let Some(skills_section) = skills_section {
        let key = skills_section.trim();
        if !key.is_empty() && seen.insert(key.to_string()) {
            unique_project_docs.push(skills_section);
        }
    }

    match (base_instructions, unique_project_docs.is_empty()) {
        (None, true) => None,
        (Some(base), true) => Some(base),
        (None, false) => Some(unique_project_docs.join("\n\n")),
        (Some(base), false) => {
            let docs = unique_project_docs.join("\n\n");
            Some(format!("{base}{PROJECT_DOC_SEPARATOR}{docs}"))
        }
    }
}

/// Read project documentation files as separate parts while respecting the
/// configured byte budget. Returns a vector of non-empty doc strings in search
/// order; the vector may be empty when no docs are found or the limit is zero.
async fn read_project_doc_parts_with_candidates(
    config: &Config,
    candidate_filenames: &[&str],
) -> std::io::Result<Vec<String>> {
    let max_total = config.project_doc_max_bytes;

    if max_total == 0 {
        return Ok(Vec::new());
    }

    let paths = discover_project_doc_paths_with_candidates(config, candidate_filenames)?;
    if paths.is_empty() {
        return Ok(Vec::new());
    }

    let mut remaining: u64 = max_total as u64;
    let mut parts: Vec<String> = Vec::new();

    for p in paths {
        if remaining == 0 {
            break;
        }

        let file = match tokio::fs::File::open(&p).await {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };

        let size = file.metadata().await?.len();
        let mut reader = tokio::io::BufReader::new(file).take(remaining);
        let mut data: Vec<u8> = Vec::new();
        reader.read_to_end(&mut data).await?;

        if size > remaining {
            tracing::warn!(
                "Project doc `{}` exceeds remaining budget ({} bytes) - truncating.",
                p.display(),
                remaining,
            );
        }

        let text = String::from_utf8_lossy(&data).to_string();
        if !text.trim().is_empty() {
            parts.push(text);
            remaining = remaining.saturating_sub(data.len() as u64);
        }
    }

    Ok(parts)
}

/// Attempt to locate and load the project documentation.
///
/// On success returns `Ok(Some(contents))` where `contents` is the
/// concatenation of all discovered docs. If no documentation file is found the
/// function returns `Ok(None)`. Unexpected I/O failures bubble up as `Err` so
/// callers can decide how to handle them.
async fn read_project_docs_with_candidates(
    config: &Config,
    candidate_filenames: &[&str],
) -> std::io::Result<Option<String>> {
    let parts = read_project_doc_parts_with_candidates(config, candidate_filenames).await?;

    if parts.is_empty() {
        Ok(None)
    } else {
        Ok(Some(parts.join("\n\n")))
    }
}

pub async fn read_project_docs(config: &Config) -> std::io::Result<Option<String>> {
    read_project_docs_with_candidates(config, AGENT_FILENAMES).await
}

pub async fn read_auto_drive_docs(config: &Config) -> std::io::Result<Option<String>> {
    read_project_docs_with_candidates(config, AUTO_AGENT_FILENAMES).await
}

/// Return discovered project doc parts (one entry per file) for standard
/// AGENT instructions. `None` indicates that no docs were found.
pub async fn read_project_doc_parts(config: &Config) -> std::io::Result<Option<Vec<String>>> {
    let parts = read_project_doc_parts_with_candidates(config, AGENT_FILENAMES).await?;
    if parts.is_empty() {
        Ok(None)
    } else {
        Ok(Some(parts))
    }
}

/// Discover the list of AGENTS.md files using the same search rules as
/// `read_project_docs`, but return the file paths instead of concatenated
/// contents. The list is ordered from repository root to the current working
/// directory (inclusive). Symlinks are allowed. When `project_doc_max_bytes`
/// is zero, returns an empty list.
fn discover_project_doc_paths_with_candidates(
    config: &Config,
    candidate_filenames: &[&str],
) -> std::io::Result<Vec<PathBuf>> {
    let mut dir = config.cwd.clone();
    if let Ok(canon) = dir.canonicalize() {
        dir = canon;
    }

    // Build chain from cwd upwards and detect git root.
    let mut chain: Vec<PathBuf> = vec![dir.clone()];
    let mut git_root: Option<PathBuf> = None;
    let mut cursor = dir.clone();
    while let Some(parent) = cursor.parent() {
        let git_marker = cursor.join(".git");
        let git_exists = match std::fs::metadata(&git_marker) {
            Ok(_) => true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(e) => return Err(e),
        };

        if git_exists {
            git_root = Some(cursor.clone());
            break;
        }

        chain.push(parent.to_path_buf());
        cursor = parent.to_path_buf();
    }

    let search_dirs: Vec<PathBuf> = if let Some(root) = git_root {
        let mut dirs: Vec<PathBuf> = Vec::new();
        let mut saw_root = false;
        for p in chain.iter().rev() {
            if !saw_root {
                if p == &root {
                    saw_root = true;
                } else {
                    continue;
                }
            }
            dirs.push(p.clone());
        }
        dirs
    } else {
        vec![config.cwd.clone()]
    };

    let mut found: Vec<PathBuf> = Vec::new();
    for d in search_dirs {
        for name in candidate_filenames {
            let candidate = d.join(name);
            match std::fs::symlink_metadata(&candidate) {
                Ok(md) => {
                    let ft = md.file_type();
                    // Allow regular files and symlinks; opening will later fail for dangling links.
                    if ft.is_file() || ft.is_symlink() {
                        found.push(candidate);
                        break;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e),
            }
        }
    }

    Ok(found)
}

pub fn discover_project_doc_paths(config: &Config) -> std::io::Result<Vec<PathBuf>> {
    discover_project_doc_paths_with_candidates(config, AGENT_FILENAMES)
}

pub fn discover_auto_drive_doc_paths(config: &Config) -> std::io::Result<Vec<PathBuf>> {
    discover_project_doc_paths_with_candidates(config, AUTO_AGENT_FILENAMES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConfigOverrides;
    use crate::config::ConfigToml;
    use std::fs;
    use tempfile::TempDir;

    /// Helper that returns a `Config` pointing at `root` and using `limit` as
    /// the maximum number of bytes to embed from AGENTS.md. The caller can
    /// optionally specify a custom `instructions` string – when `None` the
    /// value is cleared to mimic a scenario where no system instructions have
    /// been configured.
    fn make_config(root: &TempDir, limit: usize, instructions: Option<&str>) -> Config {
        let code_home = TempDir::new().unwrap();
        let mut config = Config::load_from_base_config_with_overrides(
            ConfigToml::default(),
            ConfigOverrides::default(),
            code_home.path().to_path_buf(),
        )
        .expect("defaults for test should always succeed");

        config.cwd = root.path().to_path_buf();
        config.project_doc_max_bytes = limit;

        config.user_instructions = instructions.map(ToOwned::to_owned);
        config
    }

    /// AGENTS.md missing – should yield `None`.
    #[tokio::test]
    async fn no_doc_file_returns_none() {
        let tmp = tempfile::tempdir().expect("tempdir");

        let res = get_user_instructions(&make_config(&tmp, 4096, None), None).await;
        assert!(
            res.is_none(),
            "Expected None when AGENTS.md is absent and no system instructions provided"
        );
        assert!(res.is_none(), "Expected None when AGENTS.md is absent");
    }

    /// Small file within the byte-limit is returned unmodified.
    #[tokio::test]
    async fn doc_smaller_than_limit_is_returned() {
        let tmp = tempfile::tempdir().expect("tempdir");
        fs::write(tmp.path().join("AGENTS.md"), "hello world").unwrap();

        let res = get_user_instructions(&make_config(&tmp, 4096, None), None)
            .await
            .expect("doc expected");

        assert_eq!(
            res, "hello world",
            "The document should be returned verbatim when it is smaller than the limit and there are no existing instructions"
        );
    }

    /// Oversize file is truncated to `project_doc_max_bytes`.
    #[tokio::test]
    async fn doc_larger_than_limit_is_truncated() {
        const LIMIT: usize = 1024;
        let tmp = tempfile::tempdir().expect("tempdir");

        let huge = "A".repeat(LIMIT * 2); // 2 KiB
        fs::write(tmp.path().join("AGENTS.md"), &huge).unwrap();

        let res = get_user_instructions(&make_config(&tmp, LIMIT, None), None)
            .await
            .expect("doc expected");

        assert_eq!(res.len(), LIMIT, "doc should be truncated to LIMIT bytes");
        assert_eq!(res, huge[..LIMIT]);
    }

    /// When `cwd` is nested inside a repo, the search should locate AGENTS.md
    /// placed at the repository root (identified by `.git`).
    #[tokio::test]
    async fn finds_doc_in_repo_root() {
        let repo = tempfile::tempdir().expect("tempdir");

        // Simulate a git repository. Note .git can be a file or a directory.
        std::fs::write(
            repo.path().join(".git"),
            "gitdir: /path/to/actual/git/dir\n",
        )
        .unwrap();

        // Put the doc at the repo root.
        fs::write(repo.path().join("AGENTS.md"), "root level doc").unwrap();

        // Now create a nested working directory: repo/workspace/crate_a
        let nested = repo.path().join("workspace/crate_a");
        std::fs::create_dir_all(&nested).unwrap();

        // Build config pointing at the nested dir.
        let mut cfg = make_config(&repo, 4096, None);
        cfg.cwd = nested;

        let res = get_user_instructions(&cfg, None).await.expect("doc expected");
        assert_eq!(res, "root level doc");
    }

    /// Explicitly setting the byte-limit to zero disables project docs.
    #[tokio::test]
    async fn zero_byte_limit_disables_docs() {
        let tmp = tempfile::tempdir().expect("tempdir");
        fs::write(tmp.path().join("AGENTS.md"), "something").unwrap();

        let res = get_user_instructions(&make_config(&tmp, 0, None), None).await;
        assert!(
            res.is_none(),
            "With limit 0 the function should return None"
        );
    }

    /// When both system instructions *and* a project doc are present the two
    /// should be concatenated with the separator.
    #[tokio::test]
    async fn merges_existing_instructions_with_project_doc() {
        let tmp = tempfile::tempdir().expect("tempdir");
        fs::write(tmp.path().join("AGENTS.md"), "proj doc").unwrap();

        const INSTRUCTIONS: &str = "base instructions";

        let res = get_user_instructions(&make_config(&tmp, 4096, Some(INSTRUCTIONS)), None)
            .await
            .expect("should produce a combined instruction string");

        let expected = format!("{INSTRUCTIONS}{PROJECT_DOC_SEPARATOR}{}", "proj doc");

        assert_eq!(res, expected);
    }

    /// Duplicate content in user instructions and project docs should be
    /// collapsed to avoid wasting tokens.
    #[tokio::test]
    async fn dedupes_identical_base_and_project_doc() {
        let tmp = tempfile::tempdir().expect("tempdir");
        fs::write(tmp.path().join("AGENTS.md"), "same text").unwrap();

        let res = get_user_instructions(&make_config(&tmp, 4096, Some("same text")), None)
            .await
            .expect("instructions expected");

        assert_eq!(res, "same text");
    }

    /// If the base instructions match the root doc but there is additional
    /// project-specific guidance deeper in the tree, only the new content is
    /// appended after the separator.
    #[tokio::test]
    async fn keeps_additional_unique_project_docs_after_deduplication() {
        let repo = tempfile::tempdir().expect("tempdir");

        std::fs::write(repo.path().join(".git"), "gitdir: /dev/null\n").unwrap();
        fs::write(repo.path().join("AGENTS.md"), "shared").unwrap();

        let nested = repo.path().join("workspace/crate_a");
        std::fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("AGENTS.md"), "new info").unwrap();

        let mut cfg = make_config(&repo, 4096, Some("shared"));
        cfg.cwd = nested;

        let res = get_user_instructions(&cfg, None)
            .await
            .expect("instructions expected");

        assert_eq!(res, format!("shared{PROJECT_DOC_SEPARATOR}{}", "new info"));
    }

    /// Duplicate docs discovered along the search path should only appear
    /// once, keeping the first occurrence (closest to the git root).
    #[tokio::test]
    async fn dedupes_duplicate_project_docs_along_path() {
        let repo = tempfile::tempdir().expect("tempdir");

        std::fs::write(repo.path().join(".git"), "gitdir: /dev/null\n").unwrap();
        fs::write(repo.path().join("AGENTS.md"), "root doc").unwrap();

        let nested = repo.path().join("workspace/crate_a");
        std::fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("AGENTS.md"), "root doc").unwrap();

        let mut cfg = make_config(&repo, 4096, None);
        cfg.cwd = nested;

        let res = get_user_instructions(&cfg, None).await.expect("doc expected");
        assert_eq!(res, "root doc");
    }

    /// If there are existing system instructions but the project doc is
    /// missing we expect the original instructions to be returned unchanged.
    #[tokio::test]
    async fn keeps_existing_instructions_when_doc_missing() {
        let tmp = tempfile::tempdir().expect("tempdir");

        const INSTRUCTIONS: &str = "some instructions";

        let res = get_user_instructions(&make_config(&tmp, 4096, Some(INSTRUCTIONS)), None).await;

        assert_eq!(res, Some(INSTRUCTIONS.to_string()));
    }

    /// When both the repository root and the working directory contain
    /// AGENTS.md files, their contents are concatenated from root to cwd.
    #[tokio::test]
    async fn concatenates_root_and_cwd_docs() {
        let repo = tempfile::tempdir().expect("tempdir");

        // Simulate a git repository.
        std::fs::write(
            repo.path().join(".git"),
            "gitdir: /path/to/actual/git/dir\n",
        )
        .unwrap();

        // Repo root doc.
        fs::write(repo.path().join("AGENTS.md"), "root doc").unwrap();

        // Nested working directory with its own doc.
        let nested = repo.path().join("workspace/crate_a");
        std::fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("AGENTS.md"), "crate doc").unwrap();

        let mut cfg = make_config(&repo, 4096, None);
        cfg.cwd = nested;

        let res = get_user_instructions(&cfg, None).await.expect("doc expected");
        assert_eq!(res, "root doc\n\ncrate doc");
    }

    #[tokio::test]
    async fn auto_drive_doc_missing_returns_none() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cfg = make_config(&tmp, 4096, None);
        let res = read_auto_drive_docs(&cfg).await;
        assert!(res.expect("auto docs read").is_none());
    }

    #[tokio::test]
    async fn auto_drive_docs_follow_same_lookup_rules() {
        let repo = tempfile::tempdir().expect("tempdir");

        std::fs::write(
            repo.path().join(".git"),
            "gitdir: /path/to/actual/git/dir\n",
        )
        .unwrap();

        fs::write(repo.path().join("AUTO_AGENTS.md"), "root auto").unwrap();

        let nested = repo.path().join("workspace/crate_a");
        std::fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("AUTO_AGENTS.md"), "nested auto").unwrap();

        let mut cfg = make_config(&repo, 4096, None);
        cfg.cwd = nested;

        let res = read_auto_drive_docs(&cfg)
            .await
            .expect("auto doc expected")
            .expect("auto doc should be present");

        assert_eq!(res, "root auto\n\nnested auto");
    }
}
