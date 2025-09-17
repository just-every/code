use serde::Deserialize;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use uuid::Uuid;

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

    // Fallback: scan ~/.codex/sessions when index is missing
    let mut v = fallback_scan_sessions(codex_home, cwd);
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
        // Ensure absolute rollout path for reliability
        let p = make_absolute_rollout_path(&path, codex_home);
        out.push(ResumeCandidate {
            path: p,
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

// Helper: make rollout path absolute if it looks relative by joining with codex_home/sessions.
fn make_absolute_rollout_path(path: &str, codex_home: &Path) -> PathBuf {
    let p = PathBuf::from(path);
    if p.is_absolute() {
        return p;
    }
    let mut root = codex_home.to_path_buf();
    root.push("sessions");
    root.join(p)
}

// Fallback scan implemented conservatively: walk sessions/YYYY/MM/DD ordering newest first,
// read only the first non-empty JSONL line, and match cwd exactly.
fn fallback_scan_sessions(codex_home: &Path, cwd: &Path) -> Vec<ResumeCandidate> {
    let mut root = codex_home.to_path_buf();
    root.push("sessions");
    if !root.exists() {
        return Vec::new();
    }

    let mut out: Vec<ResumeCandidate> = Vec::new();
    let mut scanned_files: usize = 0;
    const MAX_SCAN_FILES: usize = 100;

    // Collect dirs sorted desc by name interpreted as year/month/day
    fn collect_dirs_desc<T: Ord + Copy, F: Fn(&str) -> Option<T>>(parent: &Path, parse: F) -> Vec<(T, PathBuf)> {
        let mut v: Vec<(T, PathBuf)> = Vec::new();
        if let Ok(mut rd) = fs::read_dir(parent) {
            while let Some(Ok(e)) = rd.next() {
                if let Ok(ft) = e.file_type() {
                    if ft.is_dir() {
                        if let Some(name) = e.file_name().to_str() {
                            if let Some(val) = parse(name) {
                                v.push((val, e.path()));
                            }
                        }
                    }
                }
            }
        }
        v.sort_by(|a, b| b.0.cmp(&a.0));
        v
    }

    // Collect files in a dir; filter to rollout-*.jsonl and parse ts/uuid for stable sort.
    fn collect_day_files(parent: &Path) -> Vec<(String, Uuid, String, PathBuf)> {
        let mut v: Vec<(String, Uuid, String, PathBuf)> = Vec::new();
        if let Ok(mut rd) = fs::read_dir(parent) {
            while let Some(Ok(e)) = rd.next() {
                if let Ok(ft) = e.file_type() {
                    if ft.is_file() {
                        if let Some(name_str) = e.file_name().to_str() {
                            if name_str.starts_with("rollout-") && name_str.ends_with(".jsonl") {
                                if let Some((ts, id)) = parse_timestamp_uuid_from_filename(name_str) {
                                    v.push((ts, id, name_str.to_string(), e.path()));
                                }
                            }
                        }
                    }
                }
            }
        }
        // Sort (ts desc, uuid desc)
        v.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
        v
    }

    let years = collect_dirs_desc(&root, |s| s.parse::<u16>().ok());
    'outer: for (_y, yp) in years {
        let months = collect_dirs_desc(&yp, |s| s.parse::<u8>().ok());
        for (_m, mp) in months {
            let days = collect_dirs_desc(&mp, |s| s.parse::<u8>().ok());
            for (_d, dp) in days {
                let files = collect_day_files(&dp);
                for (_ts, _id, _name, p) in files {
                    scanned_files += 1;
                    if scanned_files > MAX_SCAN_FILES {
                        break 'outer;
                    }
                    if let Some(candidate) = try_make_candidate(&p, cwd) {
                        out.push(candidate);
                    }
                }
            }
        }
    }

    out
}

fn try_make_candidate(path: &Path, cwd: &Path) -> Option<ResumeCandidate> {
    // Open and read first non-empty line
    let f = fs::File::open(path).ok()?;
    let reader = BufReader::new(f);
    let mut first: Option<String> = None;
    for line in reader.lines() {
        match line {
            Ok(l) if !l.trim().is_empty() => {
                first = Some(l);
                break;
            }
            _ => continue,
        }
    }
    let Some(line) = first else { return None };
    let v: serde_json::Value = serde_json::from_str(&line).ok()?;
    let rl: codex_protocol::protocol::RolloutLine = serde_json::from_value(v).ok()?;
    let (created_ts, branch, meta_cwd) = match rl.item {
        codex_protocol::protocol::RolloutItem::SessionMeta(m) => {
            let created = m.meta.timestamp;
            let br = m.git.and_then(|g| g.branch);
            (created, br, m.meta.cwd)
        }
        _ => return None,
    };
    if meta_cwd != cwd { return None; }

    let modified_ts = file_modified_rfc3339(path).or_else(|| Some(created_ts.clone()));
    Some(ResumeCandidate {
        path: path.to_path_buf(),
        subtitle: None,
        sort_key: modified_ts.clone().unwrap_or_default(),
        created_ts: Some(created_ts),
        modified_ts,
        message_count: 0,
        branch,
        snippet: None,
    })
}

fn file_modified_rfc3339(path: &Path) -> Option<String> {
    use chrono::{DateTime, SecondsFormat, Utc};
    let meta = fs::metadata(path).ok()?;
    let mtime = meta.modified().ok()?;
    let dt: DateTime<Utc> = DateTime::<Utc>::from(mtime);
    Some(dt.to_rfc3339_opts(SecondsFormat::Secs, true))
}

fn parse_timestamp_uuid_from_filename(name: &str) -> Option<(String, Uuid)> {
    // Expected: rollout-YYYY-MM-DDThh-mm-ss-<uuid>.jsonl
    let core = name.strip_prefix("rollout-")?.strip_suffix(".jsonl")?;
    // scan from right for '-' where suffix is UUID
    let (sep_idx, uuid) = core
        .match_indices('-')
        .rev()
        .find_map(|(i, _)| Uuid::parse_str(&core[i + 1..]).ok().map(|u| (i, u)))?;
    let ts_str = &core[..sep_idx];
    Some((ts_str.to_string(), uuid))
}
