use crate::protocol::AgentMessageEvent;
use crate::protocol::AgentReasoningEvent;
use crate::protocol::AgentReasoningRawContentEvent;
use crate::protocol::BrowserSnapshotEvent;
use crate::protocol::EnvironmentContextDeltaEvent;
use crate::protocol::EnvironmentContextFullEvent;
use crate::protocol::EventMsg;
use crate::protocol::WebSearchCompleteEvent;
use code_protocol::models::ContentItem;
use code_protocol::models::ReasoningItemContent;
use code_protocol::models::ReasoningItemReasoningSummary;
use code_protocol::models::ResponseItem;
use code_protocol::models::WebSearchAction;
use code_protocol::protocol::BROWSER_SNAPSHOT_CLOSE_TAG;
use code_protocol::protocol::BROWSER_SNAPSHOT_OPEN_TAG;
use code_protocol::protocol::ENVIRONMENT_CONTEXT_CLOSE_TAG;
use code_protocol::protocol::ENVIRONMENT_CONTEXT_DELTA_CLOSE_TAG;
use code_protocol::protocol::ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG;
use code_protocol::protocol::ENVIRONMENT_CONTEXT_OPEN_TAG;
use serde_json::Value as JsonValue;

/// Convert a `ResponseItem` into zero or more `EventMsg` values that the UI can render.
///
/// When `show_raw_agent_reasoning` is false, raw reasoning content events are omitted.
#[allow(dead_code)]
pub(crate) fn map_response_item_to_event_messages(
    item: &ResponseItem,
    show_raw_agent_reasoning: bool,
) -> Vec<EventMsg> {
    match item {
        ResponseItem::Message { role, content, .. } => {
            // Do not surface system messages as user events.
            if role == "system" {
                return Vec::new();
            }

            let mut events: Vec<EventMsg> = Vec::new();

            for content_item in content.iter() {
                match content_item {
                    ContentItem::InputText { text } => {
                        if let Some(snapshot) = extract_tagged_json(text, ENVIRONMENT_CONTEXT_OPEN_TAG, ENVIRONMENT_CONTEXT_CLOSE_TAG)
                            .and_then(parse_json)
                        {
                            events.push(EventMsg::EnvironmentContextFull(
                                EnvironmentContextFullEvent {
                                    snapshot,
                                    sequence: None,
                                },
                            ));
                            continue;
                        }

                        if let Some(delta) = extract_tagged_json(
                            text,
                            ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG,
                            ENVIRONMENT_CONTEXT_DELTA_CLOSE_TAG,
                        )
                        .and_then(parse_json)
                        {
                            let base_fingerprint = delta
                                .get("base_fingerprint")
                                .and_then(|value| value.as_str())
                                .map(|value| value.to_string());
                            events.push(EventMsg::EnvironmentContextDelta(
                                EnvironmentContextDeltaEvent {
                                    base_fingerprint,
                                    sequence: None,
                                    delta,
                                },
                            ));
                            continue;
                        }

                        if let Some(snapshot) = extract_tagged_json(
                            text,
                            BROWSER_SNAPSHOT_OPEN_TAG,
                            BROWSER_SNAPSHOT_CLOSE_TAG,
                        )
                        .and_then(parse_json)
                        {
                            let url = snapshot
                                .get("url")
                                .and_then(|value| value.as_str())
                                .map(|value| value.to_string());
                            let captured_at = snapshot
                                .get("captured_at")
                                .and_then(|value| value.as_str())
                                .map(|value| value.to_string());
                            events.push(EventMsg::BrowserSnapshot(BrowserSnapshotEvent {
                                snapshot,
                                url,
                                captured_at,
                            }));
                            continue;
                        }
                    }
                    ContentItem::InputImage { .. } => {}
                    ContentItem::OutputText { text } => {
                        events.push(EventMsg::AgentMessage(AgentMessageEvent {
                            message: text.clone(),
                        }));
                    }
                }
            }

            events
        }

        ResponseItem::CompactionSummary { .. } => Vec::new(),

        ResponseItem::Reasoning {
            summary, content, ..
        } => {
            let mut events = Vec::new();
            for ReasoningItemReasoningSummary::SummaryText { text } in summary {
                events.push(EventMsg::AgentReasoning(AgentReasoningEvent {
                    text: text.clone(),
                }));
            }
            if let Some(items) = content.as_ref().filter(|_| show_raw_agent_reasoning) {
                for c in items {
                    let text = match c {
                        ReasoningItemContent::ReasoningText { text }
                        | ReasoningItemContent::Text { text } => text,
                    };
                    events.push(EventMsg::AgentReasoningRawContent(
                        AgentReasoningRawContentEvent { text: text.clone() },
                    ));
                }
            }
            events
        }

        ResponseItem::WebSearchCall { id, action, .. } => match action {
            WebSearchAction::Search { query } => {
                let call_id = id.clone().unwrap_or_else(|| "".to_string());
                vec![EventMsg::WebSearchComplete(WebSearchCompleteEvent {
                    call_id,
                    query: Some(query.clone()),
                })]
            }
            WebSearchAction::Other => Vec::new(),
        },

        // Variants that require side effects are handled by higher layers and do not emit events here.
        ResponseItem::FunctionCall { .. }
        | ResponseItem::FunctionCallOutput { .. }
        | ResponseItem::LocalShellCall { .. }
        | ResponseItem::CustomToolCall { .. }
        | ResponseItem::CustomToolCallOutput { .. }
        | ResponseItem::Other => Vec::new(),
    }
}

fn extract_tagged_json<'a>(text: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = text.find(open)? + open.len();
    let end = text[start..].find(close)? + start;
    let json_slice = text.get(start..end)?;
    let trimmed = json_slice.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed)
}

fn parse_json(fragment: &str) -> Option<JsonValue> {
    serde_json::from_str(fragment).ok()
}

#[cfg(test)]
mod tests {
    use super::map_response_item_to_event_messages;
    use crate::protocol::BrowserSnapshotEvent;
    use crate::protocol::EnvironmentContextDeltaEvent;
    use crate::protocol::EnvironmentContextFullEvent;
    use crate::protocol::EventMsg;
    use code_protocol::models::ContentItem;
    use code_protocol::models::ResponseItem;
    use code_protocol::protocol::BROWSER_SNAPSHOT_CLOSE_TAG;
    use code_protocol::protocol::BROWSER_SNAPSHOT_OPEN_TAG;
    use code_protocol::protocol::ENVIRONMENT_CONTEXT_CLOSE_TAG;
    use code_protocol::protocol::ENVIRONMENT_CONTEXT_DELTA_CLOSE_TAG;
    use code_protocol::protocol::ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG;
    use code_protocol::protocol::ENVIRONMENT_CONTEXT_OPEN_TAG;
    use serde_json::json;

    #[test]
    fn maps_user_message_with_text_and_two_images() {
        let img1 = "https://example.com/one.png".to_string();
        let img2 = "https://example.com/two.jpg".to_string();

        let item = ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![
                ContentItem::InputText {
                    text: "Hello world".to_string(),
                },
                ContentItem::InputImage {
                    image_url: img1.clone(),
                },
                ContentItem::InputImage {
                    image_url: img2.clone(),
                },
            ],
        };

        let events = map_response_item_to_event_messages(&item, false);
        // No UI event is emitted for raw user input in this fork
        assert!(events.is_empty());
    }

    #[test]
    fn maps_environment_context_full_message() {
        let payload = json!({
            "version": 1,
            "cwd": "/repo",
            "git_branch": "feature/ctx",
        });
        let item = ResponseItem::Message {
            id: Some("env-1".into()),
            role: "user".into(),
            content: vec![ContentItem::InputText {
                text: format!(
                    "{}\n{}\n{}",
                    ENVIRONMENT_CONTEXT_OPEN_TAG,
                    serde_json::to_string_pretty(&payload).unwrap(),
                    ENVIRONMENT_CONTEXT_CLOSE_TAG
                ),
            }],
        };

        let events = map_response_item_to_event_messages(&item, false);
        assert_eq!(events.len(), 1);
        match &events[0] {
            EventMsg::EnvironmentContextFull(EnvironmentContextFullEvent { snapshot, sequence }) => {
                assert_eq!(snapshot, &payload);
                assert_eq!(*sequence, None);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn maps_environment_context_delta_message() {
        let payload = json!({
            "version": 1,
            "base_fingerprint": "fp-123",
            "changes": {
                "git_branch": "ctx-ui",
            }
        });

        let item = ResponseItem::Message {
            id: Some("env-2".into()),
            role: "user".into(),
            content: vec![ContentItem::InputText {
                text: format!(
                    "{}\n{}\n{}",
                    ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG,
                    serde_json::to_string_pretty(&payload).unwrap(),
                    ENVIRONMENT_CONTEXT_DELTA_CLOSE_TAG
                ),
            }],
        };

        let events = map_response_item_to_event_messages(&item, false);
        assert_eq!(events.len(), 1);
        match &events[0] {
            EventMsg::EnvironmentContextDelta(EnvironmentContextDeltaEvent {
                delta,
                base_fingerprint,
                sequence,
            }) => {
                assert_eq!(delta, &payload);
                assert_eq!(base_fingerprint.as_deref(), Some("fp-123"));
                assert!(sequence.is_none());
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn maps_browser_snapshot_message() {
        let payload = json!({
            "url": "https://example.com",
            "captured_at": "2025-11-05T09:30:00Z",
            "hash": "abc123",
        });
        let item = ResponseItem::Message {
            id: Some("env-browser".into()),
            role: "user".into(),
            content: vec![ContentItem::InputText {
                text: format!(
                    "{}\n{}\n{}",
                    BROWSER_SNAPSHOT_OPEN_TAG,
                    serde_json::to_string_pretty(&payload).unwrap(),
                    BROWSER_SNAPSHOT_CLOSE_TAG
                ),
            }],
        };

        let events = map_response_item_to_event_messages(&item, false);
        assert_eq!(events.len(), 1);
        match &events[0] {
            EventMsg::BrowserSnapshot(BrowserSnapshotEvent {
                snapshot,
                url,
                captured_at,
            }) => {
                assert_eq!(snapshot, &payload);
                assert_eq!(url.as_deref(), Some("https://example.com"));
                assert_eq!(captured_at.as_deref(), Some("2025-11-05T09:30:00Z"));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
