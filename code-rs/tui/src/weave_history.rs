use serde::Deserialize;
use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;

pub(crate) const ROOM_LOG_FILENAME: &str = "room.jsonl";

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct WeaveDmThread {
    pub session_id: String,
    pub a: String,
    pub b: String,
}

impl WeaveDmThread {
    pub(crate) fn new(session_id: impl Into<String>, id1: impl Into<String>, id2: impl Into<String>) -> Self {
        let session_id = session_id.into();
        let id1 = id1.into();
        let id2 = id2.into();
        if id1 <= id2 {
            Self {
                session_id,
                a: id1,
                b: id2,
            }
        } else {
            Self {
                session_id,
                a: id2,
                b: id1,
            }
        }
    }

    pub(crate) fn key(&self) -> String {
        format!("dm:{}:{}:{}", self.session_id, self.a, self.b)
    }

    pub(crate) fn log_path(&self, code_home: &Path) -> PathBuf {
        let filename = format!("dm_{}_{}.jsonl", self.a, self.b);
        code_home
            .join("weave")
            .join("logs")
            .join(&self.session_id)
            .join(filename)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WeaveThreadKey {
    Room { session_id: String },
    Dm(WeaveDmThread),
}

impl WeaveThreadKey {
    pub(crate) fn session_id(&self) -> &str {
        match self {
            Self::Room { session_id } => session_id,
            Self::Dm(thread) => &thread.session_id,
        }
    }

    pub(crate) fn key(&self) -> String {
        match self {
            Self::Room { session_id } => room_thread_key(session_id),
            Self::Dm(thread) => thread.key(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct WeaveLogRecipient {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct WeaveLogEntry {
    pub v: u8,
    pub ts_ms: u64,
    pub session_id: String,
    pub message_id: String,
    pub sender_id: String,
    pub sender_label: String,
    pub recipients: Vec<WeaveLogRecipient>,
    pub text: String,
}

pub(crate) fn parse_dm_log_filename(session_id: &str, filename: &str) -> Option<WeaveDmThread> {
    let rest = filename.strip_prefix("dm_")?;
    let rest = rest.strip_suffix(".jsonl")?;
    let (a, b) = rest.rsplit_once('_')?;
    Some(WeaveDmThread::new(session_id.to_string(), a.to_string(), b.to_string()))
}

pub(crate) fn room_thread_key(session_id: &str) -> String {
    format!("room:{}", session_id.trim())
}

pub(crate) fn room_log_path(code_home: &Path, session_id: &str) -> PathBuf {
    code_home
        .join("weave")
        .join("logs")
        .join(session_id.trim())
        .join(ROOM_LOG_FILENAME)
}

pub(crate) fn parse_thread_key(key: &str) -> Option<WeaveThreadKey> {
    let key = key.trim();
    if key.is_empty() {
        return None;
    }

    let mut parts = key.split(':');
    let kind = parts.next()?;
    match kind {
        "room" => {
            let session_id = parts.next()?.trim();
            if session_id.is_empty() {
                return None;
            }
            Some(WeaveThreadKey::Room {
                session_id: session_id.to_string(),
            })
        }
        "dm" => {
            let session_id = parts.next()?.trim();
            let a = parts.next()?.trim();
            let b = parts.next()?.trim();
            if session_id.is_empty() || a.is_empty() || b.is_empty() {
                return None;
            }
            if parts.next().is_some() {
                return None;
            }
            Some(WeaveThreadKey::Dm(WeaveDmThread::new(
                session_id.to_string(),
                a.to_string(),
                b.to_string(),
            )))
        }
        _ => None,
    }
}

pub(crate) fn unread_count(entries: &[WeaveLogEntry], self_id: &str, last_read_ts_ms: u64) -> usize {
    entries
        .iter()
        .filter(|entry| entry.ts_ms > last_read_ts_ms && entry.sender_id != self_id)
        .count()
}

pub(crate) async fn read_log_tail(path: &Path, max_entries: usize) -> Vec<WeaveLogEntry> {
    let Ok(contents) = tokio::fs::read_to_string(path).await else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<WeaveLogEntry>(line) else {
            continue;
        };
        if !seen.insert(entry.message_id.clone()) {
            continue;
        }
        out.push(entry);
        if max_entries > 0 && out.len() > max_entries {
            let overflow = out.len().saturating_sub(max_entries);
            out.drain(0..overflow);
        }
    }
    out
}

pub(crate) async fn append_log_line(path: &Path, entry: &WeaveLogEntry) -> std::io::Result<()> {
    let line = serde_json::to_string(entry).unwrap_or_else(|_| "{}".to_string());
    let mut line = line;
    line.push('\n');

    if let Some(parent) = path.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }

    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;
    file.write_all(line.as_bytes()).await?;
    file.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dm_thread_is_symmetric() {
        let thread = WeaveDmThread::new("sess", "b", "a");

        assert_eq!(thread.session_id, "sess");
        assert_eq!(thread.a, "a");
        assert_eq!(thread.b, "b");
        assert_eq!(thread.key(), "dm:sess:a:b");
    }

    #[test]
    fn dm_log_filename_roundtrips() {
        let thread = WeaveDmThread::new("sess", "alice", "bob");
        let path = thread.log_path(Path::new("/tmp/code-home"));
        let filename = path.file_name().unwrap().to_string_lossy().to_string();

        let parsed = parse_dm_log_filename("sess", &filename).expect("parse");

        assert_eq!(parsed, thread);
    }

    #[test]
    fn parse_thread_key_understands_room_and_dm() {
        let room = parse_thread_key("room:sess").expect("room");
        assert_eq!(
            room,
            WeaveThreadKey::Room {
                session_id: "sess".to_string()
            }
        );

        let dm = parse_thread_key("dm:sess:a:b").expect("dm");
        assert_eq!(dm.session_id(), "sess");
        assert_eq!(dm.key(), "dm:sess:a:b");
    }

    #[test]
    fn room_log_path_is_stable() {
        let path = room_log_path(Path::new("/tmp/code-home"), "sess");
        assert!(path.to_string_lossy().ends_with("/weave/logs/sess/room.jsonl"));
    }

    #[test]
    fn unread_count_counts_inbound_since_last_read() {
        let entries = vec![
            WeaveLogEntry {
                v: 1,
                ts_ms: 90,
                session_id: "sess".to_string(),
                message_id: "m1".to_string(),
                sender_id: "other".to_string(),
                sender_label: "Other".to_string(),
                recipients: vec![WeaveLogRecipient {
                    id: "me".to_string(),
                    label: "Me".to_string(),
                }],
                text: "old".to_string(),
            },
            WeaveLogEntry {
                v: 1,
                ts_ms: 110,
                session_id: "sess".to_string(),
                message_id: "m2".to_string(),
                sender_id: "other".to_string(),
                sender_label: "Other".to_string(),
                recipients: vec![WeaveLogRecipient {
                    id: "me".to_string(),
                    label: "Me".to_string(),
                }],
                text: "new".to_string(),
            },
            WeaveLogEntry {
                v: 1,
                ts_ms: 120,
                session_id: "sess".to_string(),
                message_id: "m3".to_string(),
                sender_id: "me".to_string(),
                sender_label: "Me".to_string(),
                recipients: vec![WeaveLogRecipient {
                    id: "other".to_string(),
                    label: "Other".to_string(),
                }],
                text: "outbound".to_string(),
            },
        ];

        assert_eq!(unread_count(&entries, "me", 100), 1);
    }

    #[tokio::test]
    async fn read_log_tail_dedupes_by_message_id() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("dm.jsonl");
        tokio::fs::write(
            &path,
            r#"{"v":1,"ts_ms":1,"session_id":"s","message_id":"m1","sender_id":"a","sender_label":"A","recipients":[],"text":"hi"}
{"v":1,"ts_ms":2,"session_id":"s","message_id":"m1","sender_id":"a","sender_label":"A2","recipients":[],"text":"hi"}
{"v":1,"ts_ms":3,"session_id":"s","message_id":"m2","sender_id":"b","sender_label":"B","recipients":[],"text":"yo"}
"#,
        )
        .await
        .expect("write");

        let entries = read_log_tail(&path, 10).await;

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].message_id, "m1");
        assert_eq!(entries[1].message_id, "m2");
    }
}
