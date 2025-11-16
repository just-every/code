#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

use tempfile::TempDir;
use uuid::Uuid;

use code_protocol::ConversationId;
use code_protocol::protocol::{
    EventMsg as ProtoEventMsg,
    RecordedEvent,
    RolloutItem,
    RolloutLine,
    SessionMeta,
    SessionMetaLine,
    SessionSource,
    UserMessageEvent,
};

fn write_event_only_session(path: &Path, cwd: &Path) {
    let file = File::create(path).unwrap();
    let mut writer = BufWriter::new(file);

    let session_meta = SessionMeta {
        id: ConversationId::from(Uuid::from_u128(0xDEAD_BEEF_u128)),
        timestamp: "2025-10-06T12:00:00.000Z".to_string(),
        cwd: cwd.to_path_buf(),
        originator: "resume-test".to_string(),
        cli_version: "0.0.0-test".to_string(),
        instructions: None,
        source: SessionSource::Cli,
    };

    let meta_line = RolloutLine {
        timestamp: "2025-10-06T12:00:00.000Z".to_string(),
        item: RolloutItem::SessionMeta(SessionMetaLine {
            meta: session_meta,
            git: None,
        }),
    };
    serde_json::to_writer(&mut writer, &meta_line).unwrap();
    writer.write_all(b"\n").unwrap();

    let user_event = RecordedEvent {
        id: "evt-user".to_string(),
        event_seq: 1,
        order: None,
        msg: ProtoEventMsg::UserMessage(UserMessageEvent {
            message: "restore me".to_string(),
            kind: None,
            images: None,
        }),
    };
    let event_line = RolloutLine {
        timestamp: "2025-10-06T12:00:01.000Z".to_string(),
        item: RolloutItem::Event(user_event),
    };
    serde_json::to_writer(&mut writer, &event_line).unwrap();
    writer.write_all(b"\n").unwrap();
    writer.flush().unwrap();
}

#[test]
fn event_only_sessions_are_dropped_by_resume_discovery() {
    let temp = TempDir::new().unwrap();
    let code_home = temp.path();
    let project_cwd = code_home.join("project");
    fs::create_dir_all(&project_cwd).unwrap();

    let sessions_dir = code_home
        .join("sessions")
        .join("2025")
        .join("10")
        .join("06");
    fs::create_dir_all(&sessions_dir).unwrap();

    let rollout_path = sessions_dir.join("rollout-2025-10-06T12-00-00-00000000-0000-0000-0000-000000000042.jsonl");
    write_event_only_session(&rollout_path, &project_cwd);

    let results = super::discovery::list_sessions_for_cwd(&project_cwd, code_home, None);

    assert!(
        results.is_empty(),
        "event-only sessions should be excluded from the resume picker"
    );
}
