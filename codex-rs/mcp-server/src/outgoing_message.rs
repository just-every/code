use std::future::Future;
use std::pin::Pin;

use codex_core::protocol::Event;
use mcp_types::RequestId;
use serde::Serialize;
use tracing::warn;

pub use codex_app_server::outgoing_message::{
    OutgoingMessage, OutgoingMessageSender, OutgoingNotification,
};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OutgoingNotificationParams {
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<OutgoingNotificationMeta>,

    #[serde(flatten)]
    pub event: serde_json::Value,
}

// Additional MCP-specific data to be added to a [`codex_core::protocol::Event`] as notification.params._meta
// MCP Spec: https://modelcontextprotocol.io/specification/2025-06-18/basic#meta
// Typescript Schema: https://github.com/modelcontextprotocol/modelcontextprotocol/blob/0695a497eb50a804fc0e88c18a93a21a675d6b3e/schema/2025-06-18/schema.ts
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutgoingNotificationMeta {
    pub request_id: Option<RequestId>,
}

impl OutgoingNotificationMeta {
    pub fn new(request_id: Option<RequestId>) -> Self {
        Self { request_id }
    }
}

pub trait OutgoingMessageSenderExt {
    fn send_event_as_notification<'a>(
        &'a self,
        event: &'a Event,
        meta: Option<OutgoingNotificationMeta>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
}

impl OutgoingMessageSenderExt for OutgoingMessageSender {
    fn send_event_as_notification<'a>(
        &'a self,
        event: &'a Event,
        meta: Option<OutgoingNotificationMeta>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            #[expect(clippy::expect_used)]
            let event_json = serde_json::to_value(event).expect("Event must serialize");

            let params = if let Ok(params) = serde_json::to_value(OutgoingNotificationParams {
                meta,
                event: event_json.clone(),
            }) {
                params
            } else {
                warn!("Failed to serialize event as OutgoingNotificationParams");
                event_json
            };

            self.send_notification(OutgoingNotification {
                method: "codex/event".to_string(),
                params: Some(params),
            })
            .await;
        })
    }
}

#[cfg(test)]
mod tests {
    use codex_core::protocol::EventMsg;
    use codex_core::protocol::SessionConfiguredEvent;
    use codex_protocol::ConversationId;
    use pretty_assertions::assert_eq;
    use serde_json::json;
    use tokio::sync::mpsc;
    use uuid::Uuid;

    use super::*;

    #[tokio::test]
    async fn test_send_event_as_notification() {
        let (outgoing_tx, mut outgoing_rx) = mpsc::unbounded_channel::<OutgoingMessage>();
        let outgoing_message_sender = OutgoingMessageSender::new(outgoing_tx);

        let conversation_id = ConversationId::new();
        let event = Event {
            id: "1".to_string(),
            event_seq: 0,
            order: None,
            msg: EventMsg::SessionConfigured(SessionConfiguredEvent {
                session_id: conversation_id,
                model: "gpt-4o".to_string(),
                reasoning_effort: None,
                history_log_id: 1,
                history_entry_count: 1000,
                initial_messages: None,
                rollout_path: std::path::PathBuf::new(),
            }),
        };

        outgoing_message_sender
            .send_event_as_notification(&event, None)
            .await;

        let result = outgoing_rx.recv().await.unwrap();
        let OutgoingMessage::Notification(OutgoingNotification { method, params }) = result else {
            panic!("expected Notification for first message");
        };
        assert_eq!(method, "codex/event");

        let Ok(expected_params) = serde_json::to_value(&event) else {
            panic!("Event must serialize");
        };
        assert_eq!(params, Some(expected_params));
    }

    #[tokio::test]
    async fn test_send_event_as_notification_with_meta() {
        let (outgoing_tx, mut outgoing_rx) = mpsc::unbounded_channel::<OutgoingMessage>();
        let outgoing_message_sender = OutgoingMessageSender::new(outgoing_tx);

        let conversation_id = ConversationId::new();
        let session_configured_event = SessionConfiguredEvent {
            session_id: conversation_id,
            model: "gpt-4o".to_string(),
            reasoning_effort: None,
            history_log_id: 1,
            history_entry_count: 1000,
            initial_messages: None,
            rollout_path: std::path::PathBuf::new(),
        };
        let event = Event {
            id: "1".to_string(),
            event_seq: 0,
            order: None,
            msg: EventMsg::SessionConfigured(session_configured_event.clone()),
        };
        let meta = OutgoingNotificationMeta {
            request_id: Some(RequestId::String("123".to_string())),
        };

        outgoing_message_sender
            .send_event_as_notification(&event, Some(meta))
            .await;

        let result = outgoing_rx.recv().await.unwrap();
        let OutgoingMessage::Notification(OutgoingNotification { method, params }) = result else {
            panic!("expected Notification for first message");
        };
        assert_eq!(method, "codex/event");
        let expected_params = json!({
            "_meta": {
                "requestId": "123",
            },
            "id": "1",
            "event_seq": 0,
            "msg": {
                "session_id": session_configured_event.session_id,
                "model": session_configured_event.model,
                "history_log_id": session_configured_event.history_log_id,
                "history_entry_count": session_configured_event.history_entry_count,
                "type": "session_configured"
            }
        });
        assert_eq!(params.unwrap(), expected_params);
    }
}
