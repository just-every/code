use codex_core::Cursor;
use codex_core::RolloutRecorder;
use codex_core::protocol::EventMsg;
use codex_protocol::models::{ContentItem, ResponseItem};
use codex_protocol::protocol::{RolloutItem, RolloutLine};
use serde::Deserialize;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::thread;
use tokio::runtime::{Builder, Handle};

/// One candidate session for the picker
pub struct ResumeCandidate {
    pub path: PathBuf,
    pub subtitle: Option<String>,
    pub sort_key: String,
    pub created_ts: Option<String>,
    pub modified_ts: Option<String>,
    pub message_count: usize,
    pub branch: Option<String>,
    pub snippet: Option<String>,
}

// No fallback scan: meta parsing for rollout headers no longer needed here.

/// Return rollout files under ~/.code/sessions matching the provided cwd (Code
/// still reads legacy ~/.codex/sessions directories).
/// Reads only the first line of each file to avoid heavy IO.
pub fn list_sessions_for_cwd(cwd: &Path, codex_home: &Path) -> Vec<ResumeCandidate> {
    use std::collections::HashMap;

    const MAX_RESULTS: usize = 200;

    let mut by_path: HashMap<PathBuf, ResumeCandidate> = HashMap::new();

    if let Some(index_rows) = read_dir_index(codex_home, cwd) {
        for row in index_rows {
            by_path.entry(row.path.clone()).or_insert(row);
        }
    }

    for row in fallback_scan_sessions_for_cwd(cwd, codex_home) {
        by_path.entry(row.path.clone()).or_insert(row);
    }

    let mut results: Vec<ResumeCandidate> = by_path.into_values().collect();
    results.sort_by(|a, b| b.sort_key.cmp(&a.sort_key));
    if results.len() > MAX_RESULTS {
        results.truncate(MAX_RESULTS);
    }
    results
}

#[derive(Deserialize)]
struct DirIndexLine {
    record_type: String,
    cwd: String,
    session_file: String,
    created_ts: Option<String>,
    modified_ts: Option<String>,
    message_count_delta: Option<usize>,
    model: Option<String>,
    branch: Option<String>,
    last_user_snippet: Option<String>,
}

fn read_dir_index(codex_home: &Path, cwd: &Path) -> Option<Vec<ResumeCandidate>> {
    let index_path = super_sanitize_dir_index_path(codex_home, cwd);
    let f = fs::File::open(index_path).ok()?;
    let reader = BufReader::new(f);
    use std::collections::HashMap;
    struct Accum {
        created: Option<String>,
        modified: Option<String>,
        count: usize,
        model: Option<String>,
        branch: Option<String>,
        snippet: Option<String>,
    }
    let mut map: HashMap<String, Accum> = HashMap::new();
    for line in reader.lines() {
        let Ok(l) = line else { continue };
        if l.trim().is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<DirIndexLine>(&l) else {
            continue;
        };
        if v.record_type != "dir_index" {
            continue;
        }
        if v.cwd.is_empty() {
            continue;
        }
        let e = map.entry(v.session_file.clone()).or_insert(Accum {
            created: v.created_ts.clone(),
            modified: v.modified_ts.clone(),
            count: 0,
            model: v.model.clone(),
            branch: v.branch.clone(),
            snippet: None,
        });
        if e.created.is_none() {
            e.created = v.created_ts.clone();
        }
        e.modified = v.modified_ts.clone().or(e.modified.take());
        e.count = e.count.saturating_add(v.message_count_delta.unwrap_or(0));
        if let Some(s) = v.last_user_snippet {
            if !s.is_empty() {
                e.snippet = Some(s);
            }
        }
        if e.model.is_none() {
            e.model = v.model.clone();
        }
        if e.branch.is_none() {
            e.branch = v.branch.clone();
        }
    }
    let mut out = Vec::new();
    for (path, a) in map.into_iter() {
        if a.count == 0 {
            continue;
        }
        let subtitle = a.snippet.clone();
        out.push(ResumeCandidate {
            path: PathBuf::from(path),
            subtitle: subtitle.clone(),
            sort_key: a.modified.clone().unwrap_or_default(),
            created_ts: a.created,
            modified_ts: a.modified,
            message_count: a.count,
            branch: a.branch,
            snippet: subtitle,
        });
    }
    Some(out)
}

fn super_sanitize_dir_index_path(codex_home: &Path, cwd: &Path) -> PathBuf {
    let mut name = cwd.to_string_lossy().to_string();
    name = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if name.len() > 160 {
        name.truncate(160);
    }
    let mut p = codex_home.to_path_buf();
    p.push("sessions");
    p.push("index");
    p.push("by-dir");
    p.push(format!("{}.jsonl", name));
    p
}

// Removed fallback slow scan; the fast per-directory index is authoritative.

fn fallback_scan_sessions_for_cwd(cwd: &Path, codex_home: &Path) -> Vec<ResumeCandidate> {
    const MAX_RESULTS: usize = 200;

    let codex_home = codex_home.to_path_buf();
    let cwd = cwd.to_path_buf();

    let fetch = async move {
        let mut collected: Vec<ResumeCandidate> = Vec::new();
        let mut cursor: Option<Cursor> = None;
        while collected.len() < MAX_RESULTS {
            let page = match RolloutRecorder::list_conversations(
                codex_home.as_path(),
                256,
                cursor.as_ref(),
            )
            .await
            {
                Ok(page) => page,
                Err(err) => {
                    tracing::warn!("failed to list conversations for resume fallback: {err}");
                    break;
                }
            };

            if page.items.is_empty() {
                break;
            }

            for item in page.items {
                if let Some(candidate) = parse_rollout_candidate(&item.path, &cwd) {
                    collected.push(candidate);
                    if collected.len() >= MAX_RESULTS {
                        break;
                    }
                }
            }

            if collected.len() >= MAX_RESULTS || page.next_cursor.is_none() {
                break;
            }
            cursor = page.next_cursor;
        }

        collected
    };

    // Execute the async fetch, reusing an existing runtime when available.
    let mut sessions = match Handle::try_current() {
        Ok(handle) => {
            let handle = handle.clone();
            match thread::spawn(move || handle.block_on(fetch)).join() {
                Ok(result) => result,
                Err(_) => {
                    tracing::warn!("resume fallback thread panicked while listing conversations");
                    Vec::new()
                }
            }
        }
        Err(_) => match Builder::new_current_thread().enable_all().build() {
            Ok(rt) => rt.block_on(fetch),
            Err(err) => {
                tracing::warn!("failed to build tokio runtime for resume fallback: {err}");
                Vec::new()
            }
        },
    };

    sessions.sort_by(|a, b| b.sort_key.cmp(&a.sort_key));
    if sessions.len() > MAX_RESULTS {
        sessions.truncate(MAX_RESULTS);
    }
    sessions
}

fn parse_rollout_candidate(path: &Path, target_cwd: &Path) -> Option<ResumeCandidate> {
    let file = fs::File::open(path).ok()?;
    let reader = BufReader::new(file);

    let mut created_ts: Option<String> = None;
    let mut modified_ts: Option<String> = None;
    let mut message_count: usize = 0;
    let mut branch: Option<String> = None;
    let mut last_user_snippet: Option<String> = None;
    let mut instructions: Option<String> = None;
    let mut path_checked = false;

    for line in reader.lines() {
        let Ok(line) = line else { continue };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parsed: RolloutLine = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };
        modified_ts = Some(parsed.timestamp.clone());
        match parsed.item {
            RolloutItem::SessionMeta(meta_line) => {
                if !path_checked {
                    path_checked = true;
                    if !paths_match(&meta_line.meta.cwd, target_cwd) {
                        return None;
                    }
                }
                if created_ts.is_none() {
                    created_ts = Some(meta_line.meta.timestamp.clone());
                }
                instructions = instructions.or_else(|| meta_line.meta.instructions.clone());
                if branch.is_none() {
                    if let Some(git) = &meta_line.git {
                        if let Some(b) = &git.branch {
                            if !b.is_empty() {
                                branch = Some(b.clone());
                            }
                        }
                    }
                }
            }
            RolloutItem::ResponseItem(response_item) => {
                message_count = message_count.saturating_add(1);
                if let Some(snippet) = extract_user_snippet_from_response(&response_item) {
                    last_user_snippet = Some(snippet);
                }
            }
            RolloutItem::Event(recorded) => {
                if let Some(event) = codex_core::protocol::event_msg_from_protocol(&recorded.msg) {
                    if let Some(snippet) = extract_user_snippet_from_event(&event) {
                        last_user_snippet = Some(snippet);
                    }
                }
            }
            RolloutItem::Compacted(_) | RolloutItem::TurnContext(_) => {}
        }
    }

    if !path_checked {
        return None;
    }

    if message_count == 0 {
        return None;
    }

    let modified_ts = modified_ts.or_else(|| created_ts.clone());
    let snippet = last_user_snippet.or_else(|| instructions.clone());

    Some(ResumeCandidate {
        path: path.to_path_buf(),
        subtitle: snippet.clone(),
        sort_key: modified_ts.clone().unwrap_or_default(),
        created_ts,
        modified_ts,
        message_count,
        branch,
        snippet,
    })
}

fn extract_user_snippet_from_response(item: &ResponseItem) -> Option<String> {
    match item {
        ResponseItem::Message { role, content, .. } if role == "user" => {
            content.iter().find_map(|c| match c {
                ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                    let trimmed = text.trim();
                    if trimmed.is_empty() || trimmed.contains("== System Status ==") {
                        None
                    } else {
                        Some(trimmed.to_string())
                    }
                }
                _ => None,
            })
        }
        _ => None,
    }
}

fn extract_user_snippet_from_event(event: &EventMsg) -> Option<String> {
    match event {
        EventMsg::UserMessage(ev) => {
            let trimmed = ev.message.trim();
            if trimmed.is_empty() || trimmed.contains("== System Status ==") {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        _ => None,
    }
}

fn paths_match(meta: &Path, target: &Path) -> bool {
    if meta == target {
        return true;
    }

    let meta_norm = normalize_path(meta);
    let target_norm = normalize_path(target);
    if meta_norm == target_norm || meta_norm.eq_ignore_ascii_case(&target_norm) {
        return true;
    }

    if let (Ok(meta_real), Ok(target_real)) = (fs::canonicalize(meta), fs::canonicalize(target)) {
        let meta_real_norm = normalize_path(&meta_real);
        let target_real_norm = normalize_path(&target_real);
        if meta_real_norm == target_real_norm
            || meta_real_norm.eq_ignore_ascii_case(&target_real_norm)
        {
            return true;
        }

        // If both paths belong to the same Git project (including worktrees) consider them equivalent.
        if let (Some(meta_root), Some(target_root)) = (
            codex_core::git_info::resolve_root_git_project_for_trust(&meta_real)
                .and_then(|p| fs::canonicalize(p).ok()),
            codex_core::git_info::resolve_root_git_project_for_trust(&target_real)
                .and_then(|p| fs::canonicalize(p).ok()),
        ) {
            let meta_root_norm = normalize_path(&meta_root);
            let target_root_norm = normalize_path(&target_root);
            if meta_root_norm == target_root_norm
                || meta_root_norm.eq_ignore_ascii_case(&target_root_norm)
            {
                return true;
            }
        }
    }

    false
}

fn normalize_path(path: &Path) -> String {
    let mut s = path.to_string_lossy().replace('\\', "/");
    while s.ends_with('/') && s.len() > 1 {
        s.pop();
    }
    s
}
