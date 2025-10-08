use codex_core::protocol::{AgentMessageEvent, EventMsg, RecordedEvent, event_msg_to_protocol};
use codex_core::rollout::RolloutRecorder;
use codex_core::rollout::recorder::RolloutRecorderParams;
use codex_protocol::mcp_protocol::ConversationId;
use codex_protocol::models::{ContentItem, ResponseItem};
use tempfile::TempDir;
use uuid::Uuid;

use crate::common::load_default_config_for_test;

#[tokio::test]
async fn resume_restores_recorded_events() {
    let codex_home = TempDir::new().expect("tempdir");
    let mut config = load_default_config_for_test(&codex_home);
    config.cwd = codex_home.path().to_path_buf();

    let conversation_id = ConversationId(Uuid::new_v4());
    let recorder = RolloutRecorder::new(&config, RolloutRecorderParams::new(conversation_id, None))
        .await
        .expect("create recorder");

    let response_item = ResponseItem::Message {
        id: None,
        role: "assistant".to_string(),
        content: vec![ContentItem::OutputText {
            text: "hello from resume".to_string(),
        }],
    };
    recorder
        .record_response_items(std::slice::from_ref(&response_item))
        .await
        .expect("record response");

    let recorded_event = RecordedEvent {
        id: "turn-1".to_string(),
        event_seq: 1,
        order: None,
        msg: EventMsg::AgentMessage(AgentMessageEvent {
            message: "assistant says hi".to_string(),
        }),
    };
    let protocol_event = codex_protocol::protocol::RecordedEvent {
        id: recorded_event.id.clone(),
        event_seq: recorded_event.event_seq,
        order: None,
        msg: event_msg_to_protocol(&recorded_event.msg).expect("convert event"),
    };
    recorder
        .record_events(std::slice::from_ref(&protocol_event))
        .await
        .expect("record event");

    let rollout_path = recorder.rollout_path.clone();
    recorder.shutdown().await.expect("shutdown recorder");

    let (_rec, saved) = RolloutRecorder::resume(&config, &rollout_path)
        .await
        .expect("resume recorder");

    assert_eq!(saved.items, vec![response_item]);
    assert_eq!(saved.events, vec![recorded_event]);
}
