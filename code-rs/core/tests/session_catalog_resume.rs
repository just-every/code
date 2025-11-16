use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use code_core::session_catalog::{SessionCatalog, SessionQuery};
use code_protocol::models::{ContentItem, ResponseItem};
use code_protocol::protocol::{
    EventMsg as ProtoEventMsg, RecordedEvent, RolloutItem, RolloutLine, SessionMeta,
    SessionMetaLine, SessionSource, UserMessageEvent,
};
use code_protocol::ConversationId;
use filetime::{set_file_mtime, FileTime};
use tempfile::TempDir;
use uuid::Uuid;

fn write_rollout_transcript(
    code_home: &Path,
    session_id: Uuid,
    created_at: &str,
    last_event_at: &str,
    cwd: &Path,
    source: SessionSource,
    user_message: &str,
) -> PathBuf {
    let sessions_dir = code_home.join("sessions").join("2025").join("11").join("16");
    fs::create_dir_all(&sessions_dir).unwrap();

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

    let user_event = RolloutLine {
        timestamp: last_event_at.to_string(),
        item: RolloutItem::Event(RecordedEvent {
            id: "event-0".to_string(),
            event_seq: 0,
            order: None,
            msg: ProtoEventMsg::UserMessage(UserMessageEvent {
                message: user_message.to_string(),
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
                text: user_message.to_string(),
            }],
        }),
    };

    let response_line = RolloutLine {
        timestamp: last_event_at.to_string(),
        item: RolloutItem::ResponseItem(ResponseItem::Message {
            id: Some(format!("msg-{}", session_id)),
            role: "assistant".to_string(),
            content: vec![ContentItem::OutputText {
                text: "Ack".to_string(),
            }],
        }),
    };

    let mut writer = BufWriter::new(std::fs::File::create(&path).unwrap());
    serde_json::to_writer(&mut writer, &session_line).unwrap();
    writer.write_all(b"\n").unwrap();
    serde_json::to_writer(&mut writer, &user_event).unwrap();
    writer.write_all(b"\n").unwrap();
    serde_json::to_writer(&mut writer, &user_line).unwrap();
    writer.write_all(b"\n").unwrap();
    serde_json::to_writer(&mut writer, &response_line).unwrap();
    writer.write_all(b"\n").unwrap();
    writer.flush().unwrap();

    path
}

#[tokio::test]
async fn query_includes_exec_sessions() {
    let temp = TempDir::new().unwrap();
    let cwd = PathBuf::from("/workspace/project");
    write_rollout_transcript(
        temp.path(),
        Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
        "2025-11-15T12:00:00Z",
        "2025-11-15T12:00:10Z",
        &cwd,
        SessionSource::Cli,
        "cli",
    );
    write_rollout_transcript(
        temp.path(),
        Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap(),
        "2025-11-15T13:00:00Z",
        "2025-11-15T13:00:10Z",
        &cwd,
        SessionSource::Exec,
        "exec",
    );

    let catalog = SessionCatalog::new(temp.path().to_path_buf());
    let query = SessionQuery {
        cwd: Some(cwd.clone()),
        min_user_messages: 1,
        ..SessionQuery::default()
    };
    let results = catalog.query(&query).await.unwrap();

    assert_eq!(results.len(), 2);
    let sources: Vec<_> = results.iter().map(|e| e.session_source).collect();
    assert!(sources.contains(&SessionSource::Cli));
    assert!(sources.contains(&SessionSource::Exec));
}

#[tokio::test]
async fn latest_prefers_newer_timestamp() {
    let temp = TempDir::new().unwrap();
    let cwd = PathBuf::from("/workspace/project");
    write_rollout_transcript(
        temp.path(),
        Uuid::parse_str("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa").unwrap(),
        "2025-11-15T10:00:00Z",
        "2025-11-15T10:00:10Z",
        &cwd,
        SessionSource::Cli,
        "older",
    );
    write_rollout_transcript(
        temp.path(),
        Uuid::parse_str("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb").unwrap(),
        "2025-11-16T10:00:00Z",
        "2025-11-16T10:00:10Z",
        &cwd,
        SessionSource::Cli,
        "newer",
    );

    let catalog = SessionCatalog::new(temp.path().to_path_buf());
    let query = SessionQuery {
        limit: Some(1),
        min_user_messages: 1,
        ..SessionQuery::default()
    };
    let result = catalog.get_latest(&query).await.unwrap().unwrap();
    assert_eq!(
        result.session_id,
        Uuid::parse_str("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb").unwrap()
    );
}

#[tokio::test]
async fn find_by_prefix_matches_entry() {
    let temp = TempDir::new().unwrap();
    let cwd = PathBuf::from("/workspace/project");
    let target_id = Uuid::parse_str("12345678-9abc-4def-8123-456789abcdef").unwrap();
    write_rollout_transcript(
        temp.path(),
        target_id,
        "2025-11-16T12:00:00Z",
        "2025-11-16T12:34:56Z",
        &cwd,
        SessionSource::Cli,
        "prefix target",
    );

    let catalog = SessionCatalog::new(temp.path().to_path_buf());
    let result = catalog
        .find_by_id("12345678")
        .await
        .unwrap()
        .expect("entry should exist");
    assert_eq!(result.session_id, target_id);
}

#[tokio::test]
async fn bootstrap_catalog_from_rollouts() {
    let temp = TempDir::new().unwrap();
    let cwd = PathBuf::from("/workspace/project");
    let cli_id = Uuid::parse_str("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa").unwrap();
    let exec_id = Uuid::parse_str("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb").unwrap();

    write_rollout_transcript(
        temp.path(),
        cli_id,
        "2025-11-15T10:00:00Z",
        "2025-11-15T10:00:10Z",
        &cwd,
        SessionSource::Cli,
        "cli",
    );

    write_rollout_transcript(
        temp.path(),
        exec_id,
        "2025-11-16T09:00:00Z",
        "2025-11-16T09:10:00Z",
        &cwd,
        SessionSource::Exec,
        "exec",
    );

    let catalog = SessionCatalog::new(temp.path().to_path_buf());
    let query = SessionQuery {
        cwd: Some(cwd.clone()),
        min_user_messages: 1,
        ..SessionQuery::default()
    };
    let results = catalog.query(&query).await.unwrap();

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].session_id, exec_id);
    assert_eq!(results[1].session_id, cli_id);
}

#[tokio::test]
async fn reconcile_removes_deleted_sessions() {
    let temp = TempDir::new().unwrap();
    let cwd = PathBuf::from("/workspace/project");
    let session_id = Uuid::parse_str("cccccccc-cccc-4ccc-8ccc-cccccccccccc").unwrap();
    let rollout_path = write_rollout_transcript(
        temp.path(),
        session_id,
        "2025-11-15T12:00:00Z",
        "2025-11-15T12:05:00Z",
        &cwd,
        SessionSource::Cli,
        "delete",
    );

    let catalog = SessionCatalog::new(temp.path().to_path_buf());
    let query = SessionQuery {
        cwd: Some(cwd.clone()),
        min_user_messages: 1,
        ..SessionQuery::default()
    };
    assert_eq!(catalog.query(&query).await.unwrap().len(), 1);

    fs::remove_file(rollout_path).unwrap();
    assert_eq!(catalog.query(&query).await.unwrap().len(), 0);
}

#[tokio::test]
async fn reconcile_prefers_last_event_over_mtime() {
    let temp = TempDir::new().unwrap();
    let cwd = PathBuf::from("/workspace/project");
    let older_id = Uuid::parse_str("dddddddd-dddd-4ddd-8ddd-dddddddddddd").unwrap();
    let newer_id = Uuid::parse_str("eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee").unwrap();

    let older_path = write_rollout_transcript(
        temp.path(),
        older_id,
        "2025-11-10T09:00:00Z",
        "2025-11-10T09:05:00Z",
        &cwd,
        SessionSource::Cli,
        "old",
    );

    let newer_path = write_rollout_transcript(
        temp.path(),
        newer_id,
        "2025-11-16T09:00:00Z",
        "2025-11-16T09:15:00Z",
        &cwd,
        SessionSource::Exec,
        "new",
    );

    let base = SystemTime::now();
    set_file_mtime(&older_path, FileTime::from_system_time(base + Duration::from_secs(300))).unwrap();
    set_file_mtime(&newer_path, FileTime::from_system_time(base + Duration::from_secs(60))).unwrap();

    let catalog = SessionCatalog::new(temp.path().to_path_buf());
    let latest = catalog
        .get_latest(&SessionQuery {
            min_user_messages: 1,
            ..SessionQuery::default()
        })
        .await
        .unwrap()
        .expect("latest entry");

    assert_eq!(latest.session_id, newer_id);
}
