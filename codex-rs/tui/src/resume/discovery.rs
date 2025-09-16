use serde::Deserialize;
use serde_json::Value;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

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

/// Return rollout files under ~/.codex/sessions matching the provided cwd.
/// Reads only the first line of each file to avoid heavy IO.
pub fn list_sessions_for_cwd(cwd: &Path, codex_home: &Path) -> Vec<ResumeCandidate> {
    // First: try per-directory index
    if let Some(mut v) = read_dir_index(codex_home, cwd) {
        v.sort_by(|a, b| b.sort_key.cmp(&a.sort_key));
        return v.into_iter().take(200).collect();
    }

    // Fallback: scan sessions directory for matching files (first-line only)
    let mut v = fallback_scan_sessions(cwd, codex_home);
    v.sort_by(|a, b| b.sort_key.cmp(&a.sort_key));
    v.into_iter().take(200).collect()
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
        if l.trim().is_empty() { continue; }
        let Ok(v) = serde_json::from_str::<DirIndexLine>(&l) else { continue };
        if v.record_type != "dir_index" { continue; }
        if v.cwd.is_empty() { continue; }
        let e = map.entry(v.session_file.clone()).or_insert(Accum {
            created: v.created_ts.clone(),
            modified: v.modified_ts.clone(),
            count: 0,
            model: v.model.clone(),
            branch: v.branch.clone(),
            snippet: None,
        });
        if e.created.is_none() { e.created = v.created_ts.clone(); }
        e.modified = v.modified_ts.clone().or(e.modified.take());
        e.count = e.count.saturating_add(v.message_count_delta.unwrap_or(0));
        if let Some(s) = v.last_user_snippet { if !s.is_empty() { e.snippet = Some(s); } }
        if e.model.is_none() { e.model = v.model.clone(); }
        if e.branch.is_none() { e.branch = v.branch.clone(); }
    }
    let mut out = Vec::new();
    for (path, a) in map.into_iter() {
        if a.count == 0 { continue; }
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
    name = name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    if name.len() > 160 { name.truncate(160); }
    let mut p = codex_home.to_path_buf();
    p.push("sessions");
    p.push("index");
    p.push("by-dir");
    p.push(format!("{}.jsonl", name));
    p
}

/// Fallback: recursively scan ~/.codex/sessions for rollout files.
/// Only reads the first line of each .jsonl to match on cwd.
fn fallback_scan_sessions(cwd: &Path, codex_home: &Path) -> Vec<ResumeCandidate> {
    let mut out: Vec<ResumeCandidate> = Vec::new();
    let sessions_root = {
        let mut p = codex_home.to_path_buf();
        p.push("sessions");
        p
    };
    if !sessions_root.exists() {
        return out;
    }

    let cwd_str = cwd.to_string_lossy().to_string();

    // Simple DFS without extra deps
    let mut stack: Vec<PathBuf> = vec![sessions_root];
    // Bound the total files inspected to avoid heavy IO on huge trees
    let mut files_seen: usize = 0;
    const FILE_SCAN_LIMIT: usize = 5000;

    while let Some(dir) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for ent in rd.flatten() {
            let path = ent.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if files_seen >= FILE_SCAN_LIMIT {
                break;
            }
            files_seen += 1;
            if !matches!(path.extension().and_then(|s| s.to_str()), Some("jsonl")) {
                continue;
            }
            // Open and read only the first line
            let Ok(f) = fs::File::open(&path) else { continue };
            let mut reader = BufReader::new(f);
            let mut first = String::new();
            if reader.read_line(&mut first).is_err() { continue; }
            if first.trim().is_empty() { continue; }
            let Ok(v): Result<Value, _> = serde_json::from_str(&first) else { continue };
            // Expect a rollout line with { timestamp, type, payload }
            let t = v.get("type").and_then(|s| s.as_str()).unwrap_or("");
            if t != "session_meta" { continue; }
            let Some(payload) = v.get("payload") else { continue };
            let Some(line_cwd) = payload.get("cwd").and_then(|c| c.as_str()) else { continue };
            if line_cwd != cwd_str { continue; }

            // Extract created timestamp, prefer payload.timestamp from SessionMeta
            let created_ts = payload
                .get("timestamp")
                .and_then(|s| s.as_str())
                .map(|s| s.to_string());

            // Modified timestamp from file metadata
            let modified_ts = fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|st| {
                    // Format as RFC3339 string
                    let dt: chrono::DateTime<chrono::Utc> = st.into();
                    Some(dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
                });

            // Count messages: number of non-empty lines minus the first meta line
            let message_count = match fs::read_to_string(&path) {
                Ok(text) => {
                    let mut n = 0usize;
                    for (i, l) in text.lines().enumerate() {
                        if i == 0 { continue; }
                        if l.trim().is_empty() { continue; }
                        n = n.saturating_add(1);
                    }
                    n
                }
                Err(_) => 0,
            };

            // Optional branch from payload.git.branch
            let branch = payload
                .get("git")
                .and_then(|g| g.get("branch"))
                .and_then(|b| b.as_str())
                .map(|s| s.to_string());

            let sort_key = modified_ts.clone().unwrap_or_default();
            out.push(ResumeCandidate {
                path: path.clone(),
                subtitle: None,
                sort_key,
                created_ts,
                modified_ts,
                message_count,
                branch,
                snippet: None,
            });
        }
    }

    out
}
