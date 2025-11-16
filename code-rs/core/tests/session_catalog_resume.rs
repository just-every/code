use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use code_core::{SessionCatalog, SessionIndexEntry, SessionQuery};
use code_protocol::models::{ContentItem, ResponseItem};
use code_protocol::protocol::{
    EventMsg as ProtoEventMsg, RecordedEvent, RolloutItem, RolloutLine, SessionMeta,
    SessionMetaLine, SessionSource, UserMessageEvent,
};
use code_protocol::ConversationId;
use filetime::{set_file_mtime, FileTime};
use tempfile::TempDir;
use uuid::Uuid;

fn write_catalog(code_home: &Path, entries: &[SessionIndexEntry]) {
    let catalog_path = code_home.join("sessions/index/catalog.jsonl");
    if let Some(parent) = catalog_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let data = entries
        .iter()
        .map(|entry| serde_json::to_string(entry).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(catalog_path, format!("{}\n", data)).unwrap();
}

fn entry(id: &str, source: SessionSource, last: &str, created: &str, cwd: &Path) -> SessionIndexEntry {
    SessionIndexEntry {
        session_id: Uuid::parse_str(id).unwrap(),
        rollout_path: PathBuf::from(format!("sessions/2025/11/16/rollout-{id}.jsonl")),
        snapshot_path: None,
        created_at: created.to_string(),
        last_event_at: last.to_string(),
        cwd_real: cwd.to_path_buf(),
        cwd_display: cwd.to_string_lossy().to_string(),
        git_project_root: None,
        git_branch: None,
        model_provider: Some("test-provider".to_string()),
        session_source: source,
        message_count: 3,
        last_user_snippet: Some("last user input".to_string()),
        sync_origin_device: None,
        sync_version: 0,
        archived: false,
        deleted: false,
    }
}

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
    serde_json::to_writer(&mut writer, &response_line).unwrap();
    writer.write_all(b"\n").unwrap();
    writer.flush().unwrap();

    path
}

#[tokio::test]
async fn query_includes_exec_sessions() {
    let temp = TempDir::new().unwrap();
    let cwd = PathBuf::from("/workspace/project");
    let cli_entry = entry(
        "11111111-1111-4111-8111-111111111111",
        SessionSource::Cli,
        "2025-11-15T12:00:00Z",
        "2025-11-15T11:59:00Z",
        &cwd,
    );
    let exec_entry = entry(
        "22222222-2222-4222-8222-222222222222",
        SessionSource::Exec,
        "2025-11-15T13:00:00Z",
        "2025-11-15T12:59:00Z",
        &cwd,
    );

    write_catalog(temp.path(), &[cli_entry.clone(), exec_entry.clone()]);

    let catalog = SessionCatalog::new(temp.path().to_path_buf());
    let query = SessionQuery {
        cwd: Some(cwd.clone()),
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
    let older = entry(
        "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
        SessionSource::Cli,
        "2025-11-15T10:00:00Z",
        "2025-11-15T09:00:00Z",
        &cwd,
    );
    let newer = entry(
        "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
        SessionSource::Cli,
        "2025-11-16T10:00:00Z",
        "2025-11-16T09:00:00Z",
        &cwd,
    );

    write_catalog(temp.path(), &[older, newer.clone()]);

    let catalog = SessionCatalog::new(temp.path().to_path_buf());
    let query = SessionQuery { limit: Some(1), ..SessionQuery::default() };
    let result = catalog.get_latest(&query).await.unwrap().unwrap();
    assert_eq!(result.session_id, newer.session_id);
}

#[tokio::test]
async fn find_by_prefix_matches_entry() {
    let temp = TempDir::new().unwrap();
    let cwd = PathBuf::from("/workspace/project");
    let target = entry(
        "12345678-9abc-4def-8123-456789abcdef",
        SessionSource::Cli,
        "2025-11-16T12:34:56Z",
        "2025-11-16T12:00:00Z",
        &cwd,
    );

    write_catalog(temp.path(), &[target.clone()]);

    let catalog = SessionCatalog::new(temp.path().to_path_buf());
    let result = catalog
        .find_by_id("12345678")
        .await
        .unwrap()
        .expect("entry should exist");
    assert_eq!(result.session_id, target.session_id);
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
        "to delete",
    );

    let catalog = SessionCatalog::new(temp.path().to_path_buf());
    let query = SessionQuery {
        cwd: Some(cwd.clone()),
        ..SessionQuery::default()
    };
    let results = catalog.query(&query).await.unwrap();
    assert_eq!(results.len(), 1);

    fs::remove_file(rollout_path).unwrap();

    let results_after = catalog.query(&query).await.unwrap();
    assert!(results_after.is_empty());
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
        .get_latest(&SessionQuery::default())
        .await
        .unwrap()
        .expect("latest entry");

    assert_eq!(latest.session_id, newer_id);
}
