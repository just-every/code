use std::fmt::Write as _;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::Duration;
use std::time::Instant;

use chrono::Utc;
use code_protocol::protocol::SessionSource;

use crate::rollout::catalog::SessionCatalog;

const MEMORIES_DIR: &str = "memories";
const MEMORY_SUMMARY_FILENAME: &str = "memory_summary.md";
const MAX_RECENT_SESSIONS: usize = 50;
const MAX_LAST_USER_SNIPPET_CHARS: usize = 280;
const MAX_MEMORY_PROMPT_CHARS: usize = 12_000;
const REFRESH_INTERVAL: Duration = Duration::from_secs(300);
const READ_PATH_TEMPLATE: &str = include_str!("../templates/memories/read_path.md");

static LAST_REFRESH_AT: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();

pub(crate) fn memory_root(code_home: &Path) -> PathBuf {
    code_home.join(MEMORIES_DIR)
}

fn memory_summary_path(code_home: &Path) -> PathBuf {
    memory_root(code_home).join(MEMORY_SUMMARY_FILENAME)
}

fn should_refresh_now() -> bool {
    let now = Instant::now();
    let mutex = LAST_REFRESH_AT.get_or_init(|| Mutex::new(None));
    let mut guard = match mutex.lock() {
        Ok(guard) => guard,
        Err(_) => return false,
    };

    let should_refresh = guard
        .as_ref()
        .is_none_or(|last| now.duration_since(*last) >= REFRESH_INTERVAL);
    if should_refresh {
        *guard = Some(now);
    }
    should_refresh
}

pub(crate) fn maybe_spawn_memory_summary_refresh(code_home: PathBuf) {
    if !should_refresh_now() {
        return;
    }

    tokio::spawn(async move {
        if let Err(err) = rebuild_memory_summary_from_catalog(&code_home).await {
            tracing::debug!("memory summary refresh skipped: {err}");
        }
    });
}

pub(crate) async fn build_memory_tool_developer_instructions(code_home: &Path) -> Option<String> {
    let summary_path = memory_summary_path(code_home);
    let summary = tokio::fs::read_to_string(&summary_path).await.ok()?;
    let summary = summary.trim();
    if summary.is_empty() {
        return None;
    }

    let truncated = truncate_to_char_boundary(summary, MAX_MEMORY_PROMPT_CHARS);
    let base_path = memory_root(code_home).display().to_string();
    let prompt = READ_PATH_TEMPLATE
        .replace("{{ base_path }}", &base_path)
        .replace("{{ memory_summary }}", &truncated);
    Some(prompt)
}

async fn rebuild_memory_summary_from_catalog(code_home: &Path) -> io::Result<()> {
    let code_home = code_home.to_path_buf();
    let render_home = code_home.clone();
    let rendered = tokio::task::spawn_blocking(move || render_memory_summary(&render_home))
        .await
        .map_err(|err| io::Error::other(format!("memory summary task join failed: {err}")))??;

    let root = memory_root(code_home.as_path());
    tokio::fs::create_dir_all(&root).await?;
    tokio::fs::write(memory_summary_path(code_home.as_path()), rendered).await
}

fn render_memory_summary(code_home: &Path) -> io::Result<String> {
    let catalog = SessionCatalog::load(code_home)?;
    let mut body = String::new();
    writeln!(body, "# Memory Summary").map_err(io::Error::other)?;
    writeln!(body, "Generated: {}", Utc::now().to_rfc3339()).map_err(io::Error::other)?;
    writeln!(
        body,
        "This summary captures recent interactive sessions and their last user requests."
    )
    .map_err(io::Error::other)?;
    writeln!(body).map_err(io::Error::other)?;

    let mut included = 0usize;
    for entry in catalog.all_ordered() {
        if included >= MAX_RECENT_SESSIONS {
            break;
        }
        if entry.deleted || entry.archived {
            continue;
        }
        if !matches!(entry.session_source, SessionSource::Cli | SessionSource::VSCode) {
            continue;
        }

        included += 1;
        let snippet = entry
            .last_user_snippet
            .as_deref()
            .map(|value| truncate_to_char_boundary(value, MAX_LAST_USER_SNIPPET_CHARS))
            .unwrap_or_else(|| "(no user snippet)".to_string());
        let branch = entry.git_branch.as_deref().unwrap_or("(no git branch)");
        writeln!(
            body,
            "## {} | {}",
            entry.last_event_at,
            entry.session_id
        )
        .map_err(io::Error::other)?;
        writeln!(body, "cwd: {}", entry.cwd_display).map_err(io::Error::other)?;
        writeln!(body, "git_branch: {branch}").map_err(io::Error::other)?;
        writeln!(body, "last_user_request: {snippet}").map_err(io::Error::other)?;
        writeln!(body).map_err(io::Error::other)?;
    }

    if included == 0 {
        writeln!(body, "No prior interactive sessions found.").map_err(io::Error::other)?;
    }

    Ok(body)
}

fn truncate_to_char_boundary(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }
    input.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::truncate_to_char_boundary;

    #[test]
    fn truncate_preserves_short_input() {
        assert_eq!(truncate_to_char_boundary("abc", 10), "abc");
    }

    #[test]
    fn truncate_respects_char_boundaries() {
        let truncated = truncate_to_char_boundary("hello-world", 5);
        assert_eq!(truncated, "hello");
    }
}
