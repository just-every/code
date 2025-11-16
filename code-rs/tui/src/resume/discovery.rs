use code_core::{entry_to_rollout_path, SessionCatalog, SessionIndexEntry, SessionQuery};
use std::path::{Path, PathBuf};
use std::thread;
use tokio::runtime::{Builder, Handle};

/// One candidate session for the picker
pub struct ResumeCandidate {
    pub path: PathBuf,
    pub subtitle: Option<String>,
    pub created_ts: Option<String>,
    pub modified_ts: Option<String>,
    pub message_count: usize,
    pub branch: Option<String>,
    pub snippet: Option<String>,
}

/// Return sessions matching the provided cwd using the SessionCatalog.
/// Includes CLI, VSCode, Exec/model sessions, etc.
pub fn list_sessions_for_cwd(cwd: &Path, code_home: &Path) -> Vec<ResumeCandidate> {
    const MAX_RESULTS: usize = 200;

    let code_home = code_home.to_path_buf();
    let cwd = cwd.to_path_buf();

    let fetch = async move {
        let catalog = SessionCatalog::new(code_home.clone());
        let query = SessionQuery {
            cwd: Some(cwd),
            git_root: None,
            sources: Vec::new(),
            include_archived: false,
            include_deleted: false,
            limit: Some(MAX_RESULTS),
        };

        match catalog.query(&query).await {
            Ok(entries) => entries
                .into_iter()
                .map(|entry| entry_to_candidate(&code_home, entry))
                .collect(),
            Err(err) => {
                tracing::warn!("failed to query session catalog: {err}");
                Vec::new()
            }
        }
    };

    // Execute the async fetch, reusing an existing runtime when available.
    match Handle::try_current() {
        Ok(handle) => {
            let handle = handle.clone();
            match thread::spawn(move || handle.block_on(fetch)).join() {
                Ok(result) => result,
                Err(_) => {
                    tracing::warn!("resume picker thread panicked while querying catalog");
                    Vec::new()
                }
            }
        }
        Err(_) => match Builder::new_current_thread().enable_all().build() {
            Ok(rt) => rt.block_on(fetch),
            Err(err) => {
                tracing::warn!("failed to build tokio runtime for resume picker: {err}");
                Vec::new()
            }
        },
    }
}

fn entry_to_candidate(code_home: &Path, entry: SessionIndexEntry) -> ResumeCandidate {
    let path = entry_to_rollout_path(code_home, &entry);

    ResumeCandidate {
        path,
        subtitle: entry.last_user_snippet.clone(),
        created_ts: Some(entry.created_at.clone()),
        modified_ts: Some(entry.last_event_at.clone()),
        message_count: entry.message_count,
        branch: entry.git_branch.clone(),
        snippet: entry.last_user_snippet.clone(),
    }
}
