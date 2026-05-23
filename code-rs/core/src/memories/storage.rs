use std::collections::HashSet;
use std::fmt::Write as _;
use std::io;
use std::path::Path;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::{Stage1Output, Stage1OutputRef, memory_md_path, memory_root};

const ROLLOUT_SUMMARIES_SUBDIR: &str = "rollout_summaries";
const RAW_MEMORIES_FILENAME: &str = "raw_memories.md";

fn rollout_summaries_dir(code_home: &Path) -> std::path::PathBuf {
    memory_root(code_home).join(ROLLOUT_SUMMARIES_SUBDIR)
}

fn raw_memories_file(code_home: &Path) -> std::path::PathBuf {
    memory_root(code_home).join(RAW_MEMORIES_FILENAME)
}

pub(super) async fn rebuild_raw_memories_file(
    code_home: &Path,
    memories: &[Stage1Output],
    max_raw_memories: usize,
) -> io::Result<()> {
    tokio::fs::create_dir_all(rollout_summaries_dir(code_home)).await?;
    let retained = &memories[..memories.len().min(max_raw_memories)];
    let mut body = String::from("# Raw Memories\n\n");
    if retained.is_empty() {
        body.push_str("No raw memories yet.\n");
        return tokio::fs::write(raw_memories_file(code_home), body).await;
    }

    body.push_str("Merged stage-1 raw memories (latest first):\n\n");
    for memory in retained {
        writeln!(body, "## Session `{}`", memory.session_id).map_err(io::Error::other)?;
        writeln!(body, "updated_at: {}", memory.source_updated_at.to_rfc3339())
            .map_err(io::Error::other)?;
        writeln!(body, "cwd: {}", memory.cwd.display()).map_err(io::Error::other)?;
        writeln!(body, "rollout_path: {}", memory.rollout_path.display())
            .map_err(io::Error::other)?;
        writeln!(body, "rollout_summary_file: {}.md", rollout_summary_file_stem(memory))
            .map_err(io::Error::other)?;
        writeln!(body).map_err(io::Error::other)?;
        body.push_str(memory.raw_memory.trim());
        body.push_str("\n\n");
    }

    tokio::fs::write(raw_memories_file(code_home), body).await
}

pub(super) async fn sync_rollout_summaries(
    code_home: &Path,
    memories: &[Stage1Output],
) -> io::Result<()> {
    tokio::fs::create_dir_all(rollout_summaries_dir(code_home)).await?;
    let keep = memories
        .iter()
        .map(rollout_summary_file_stem)
        .collect::<HashSet<_>>();
    prune_rollout_summaries(code_home, &keep).await?;
    for memory in memories {
        write_rollout_summary(code_home, memory).await?;
    }
    if memories.is_empty() {
        let memory_md = memory_md_path(code_home);
        if let Err(err) = tokio::fs::remove_file(memory_md).await
            && err.kind() != io::ErrorKind::NotFound
        {
            return Err(err);
        }
    }
    Ok(())
}

async fn prune_rollout_summaries(code_home: &Path, keep: &HashSet<String>) -> io::Result<()> {
    let mut dir = match tokio::fs::read_dir(rollout_summaries_dir(code_home)).await {
        Ok(dir) => dir,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err),
    };

    while let Some(entry) = dir.next_entry().await? {
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(stem) = file_name.strip_suffix(".md") else {
            continue;
        };
        if !keep.contains(stem) {
            let _ = tokio::fs::remove_file(path).await;
        }
    }
    Ok(())
}

async fn write_rollout_summary(code_home: &Path, memory: &Stage1Output) -> io::Result<()> {
    let path = rollout_summaries_dir(code_home).join(format!("{}.md", rollout_summary_file_stem(memory)));
    let mut body = String::new();
    writeln!(body, "session_id: {}", memory.session_id).map_err(io::Error::other)?;
    writeln!(body, "updated_at: {}", memory.source_updated_at.to_rfc3339())
        .map_err(io::Error::other)?;
    writeln!(body, "rollout_path: {}", memory.rollout_path.display()).map_err(io::Error::other)?;
    writeln!(body, "cwd: {}", memory.cwd.display()).map_err(io::Error::other)?;
    if let Some(branch) = memory.git_branch.as_deref() {
        writeln!(body, "git_branch: {branch}").map_err(io::Error::other)?;
    }
    writeln!(body).map_err(io::Error::other)?;
    body.push_str(memory.rollout_summary.trim());
    body.push('\n');
    tokio::fs::write(path, body).await
}

pub(super) fn rollout_summary_file_stem(memory: &Stage1Output) -> String {
    rollout_summary_file_stem_parts(memory.session_id, memory.source_updated_at, memory.rollout_slug.as_deref())
}

pub(super) fn rollout_summary_file_stem_ref(memory: &Stage1OutputRef) -> String {
    rollout_summary_file_stem_parts(memory.session_id, memory.source_updated_at, memory.rollout_slug.as_deref())
}

fn rollout_summary_file_stem_parts(
    session_id: Uuid,
    source_updated_at: DateTime<Utc>,
    rollout_slug: Option<&str>,
) -> String {
    let prefix = format!(
        "{}-{}",
        source_updated_at.format("%Y-%m-%dT%H-%M-%S"),
        &session_id.simple().to_string()[..4],
    );
    let Some(slug) = rollout_slug else {
        return prefix;
    };
    let slug = slug
        .chars()
        .take(60)
        .map(|ch| if ch.is_ascii_alphanumeric() { ch.to_ascii_lowercase() } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string();
    if slug.is_empty() { prefix } else { format!("{prefix}-{slug}") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn file_stem_uses_timestamp_and_slug() {
        let stem = rollout_summary_file_stem_parts(
            Uuid::nil(),
            Utc.with_ymd_and_hms(2026, 3, 8, 0, 0, 0).unwrap(),
            Some("My Summary!"),
        );
        assert!(stem.starts_with("2026-03-08T00-00-00-0000-my_summary"));
    }
}
