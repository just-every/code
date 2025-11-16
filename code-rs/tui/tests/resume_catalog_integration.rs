use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use code_protocol::models::{ContentItem, ResponseItem};
use code_protocol::protocol::{
    EventMsg as ProtoEventMsg, RecordedEvent, RolloutItem, RolloutLine, SessionMeta,
    SessionMetaLine, SessionSource, UserMessageEvent,
};
use code_protocol::ConversationId;
use filetime::{set_file_mtime, FileTime};
use tempfile::TempDir;
use uuid::Uuid;

use code_tui::resume::discovery::list_sessions_for_cwd;

fn write_rollout(
    code_home: &std::path::Path,
    session_id: Uuid,
    created_at: &str,
    last_event_at: &str,
    cwd: &std::path::Path,
    source: SessionSource,
    user_text: &str,
) -> PathBuf {
    let sessions_dir = code_home.join("sessions").join("2025").join("11").join("16");
    std::fs::create_dir_all(&sessions_dir).unwrap();

    let filename = format!(
        "rollout-{}-{}.jsonl",
        created_at.replace(':', "-"),
        session_id
    );
    let path = sessions_dir.join(filename);

    let session_meta = SessionMeta {
        id: ConversationId::from(session_id),
        timestamp: created_at.to_string(),
        cwd: cwd.to_path_buf(),
        originator: "test".to_string(),
        cli_version: "0.0.0-test".to_string(),
        instructions: None,
        source,
    };

    let session_line = RolloutLine {
        timestamp: created_at.to_string(),
        item: RolloutItem::SessionMeta(SessionMetaLine {
            meta: session_meta,
            git: None,
        }),
    };

    let event_line = RolloutLine {
        timestamp: last_event_at.to_string(),
        item: RolloutItem::Event(RecordedEvent {
            id: "event-0".to_string(),
            event_seq: 0,
            order: None,
            msg: ProtoEventMsg::UserMessage(UserMessageEvent {
                message: user_text.to_string(),
                kind: None,
                images: None,
            }),
        }),
    };

    let user_line = RolloutLine {
        timestamp: last_event_at.to_string(),
        item: RolloutItem::ResponseItem(ResponseItem::Message {
            id: Some(format!("user-{}", session_id)),
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: user_text.to_string(),
            }],
        }),
    };

    let assistant_line = RolloutLine {
        timestamp: last_event_at.to_string(),
        item: RolloutItem::ResponseItem(ResponseItem::Message {
            id: Some(format!("msg-{}", session_id)),
            role: "assistant".to_string(),
            content: vec![ContentItem::OutputText {
                text: format!("Ack: {}", user_text),
            }],
        }),
    };

    let mut writer = BufWriter::new(std::fs::File::create(&path).unwrap());
    serde_json::to_writer(&mut writer, &session_line).unwrap();
    writer.write_all(b"\n").unwrap();
    serde_json::to_writer(&mut writer, &event_line).unwrap();
    writer.write_all(b"\n").unwrap();
    serde_json::to_writer(&mut writer, &user_line).unwrap();
    writer.write_all(b"\n").unwrap();
    serde_json::to_writer(&mut writer, &assistant_line).unwrap();
    writer.write_all(b"\n").unwrap();
    writer.flush().unwrap();

    path
}

#[test]
fn resume_picker_lists_exec_sessions() {
    let temp = TempDir::new().unwrap();
    let cwd = std::path::PathBuf::from("/project");

    write_rollout(
        temp.path(),
        Uuid::parse_str("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa").unwrap(),
        "2025-11-15T10:00:00Z",
        "2025-11-15T10:00:10Z",
        &cwd,
        SessionSource::Cli,
        "cli",
    );

    write_rollout(
        temp.path(),
        Uuid::parse_str("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb").unwrap(),
        "2025-11-15T11:00:00Z",
        "2025-11-15T11:00:10Z",
        &cwd,
        SessionSource::Exec,
        "exec",
    );

    let results = list_sessions_for_cwd(&cwd, temp.path(), None);
    assert_eq!(results.len(), 2);
    let sources: Vec<_> = results.iter().map(|cand| cand.subtitle.clone()).collect();
    assert!(sources.iter().any(|s| s.as_deref() == Some("cli")));
    assert!(sources.iter().any(|s| s.as_deref() == Some("exec")));
}

#[test]
fn resume_picker_orders_by_last_event_even_with_mtime_drift() {
    let temp = TempDir::new().unwrap();
    let cwd = std::path::PathBuf::from("/project");

    let old_path = write_rollout(
        temp.path(),
        Uuid::parse_str("cccccccc-cccc-4ccc-8ccc-cccccccccccc").unwrap(),
        "2025-11-01T10:00:00Z",
        "2025-11-01T10:00:05Z",
        &cwd,
        SessionSource::Cli,
        "old",
    );

    let new_path = write_rollout(
        temp.path(),
        Uuid::parse_str("dddddddd-dddd-4ddd-8ddd-dddddddddddd").unwrap(),
        "2025-11-16T10:00:00Z",
        "2025-11-16T10:15:00Z",
        &cwd,
        SessionSource::Exec,
        "new",
    );

    // Simulate sync where the older file has the newest mtime.
    let base = SystemTime::now();
    set_file_mtime(&old_path, FileTime::from_system_time(base + Duration::from_secs(300))).unwrap();
    set_file_mtime(&new_path, FileTime::from_system_time(base + Duration::from_secs(60))).unwrap();

    let results = list_sessions_for_cwd(&cwd, temp.path(), None);
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].path, new_path);
}

#[test]
fn resume_picker_excludes_current_path_and_empty_sessions() {
    let temp = TempDir::new().unwrap();
    let cwd = std::path::PathBuf::from("/project");

    let exclude = write_rollout(
        temp.path(),
        Uuid::parse_str("eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee").unwrap(),
        "2025-11-17T10:00:00Z",
        "2025-11-17T10:00:05Z",
        &cwd,
        SessionSource::Cli,
        "current",
    );

    // Empty session
    let sessions_dir = temp.path().join("sessions").join("2025").join("11").join("18");
    std::fs::create_dir_all(&sessions_dir).unwrap();
    let empty_path = sessions_dir.join("rollout-empty.jsonl");
    let session_meta = SessionMeta {
        id: ConversationId::new(),
        timestamp: "2025-11-18T09:00:00Z".to_string(),
        cwd: cwd.clone(),
        originator: "test".to_string(),
        cli_version: "0.0.0-test".to_string(),
        instructions: None,
        source: SessionSource::Cli,
    };
    let session_line = RolloutLine {
        timestamp: session_meta.timestamp.clone(),
        item: RolloutItem::SessionMeta(SessionMetaLine { meta: session_meta, git: None }),
    };
    let mut writer = BufWriter::new(std::fs::File::create(&empty_path).unwrap());
    serde_json::to_writer(&mut writer, &session_line).unwrap();
    writer.write_all(b"\n").unwrap();
    writer.flush().unwrap();

    let visible = write_rollout(
        temp.path(),
        Uuid::parse_str("ffffffff-ffff-4fff-8fff-ffffffffffff").unwrap(),
        "2025-11-18T08:00:00Z",
        "2025-11-18T08:05:00Z",
        &cwd,
        SessionSource::Exec,
        "visible",
    );

    let results = list_sessions_for_cwd(&cwd, temp.path(), Some(exclude.as_path()));
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].path, visible);
}
