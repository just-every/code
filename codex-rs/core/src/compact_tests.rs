use super::*;
use codex_history::CodexHarnessMetadata;
use codex_history::ResponseItemEnvelope;
use codex_protocol::ResponseItemId;
use codex_protocol::models::ContentItemKind;
use codex_protocol::models::DEFAULT_IMAGE_DETAIL;
use codex_protocol::models::InternalChatMessageMetadataPassthrough;
use pretty_assertions::assert_eq;

fn annotated(items: Vec<ResponseItem>) -> Vec<ResponseItemEnvelope> {
    items.into_iter().map(ResponseItemEnvelope::new).collect()
}

fn raw(items: Vec<ResponseItemEnvelope>) -> Vec<ResponseItem> {
    items
        .into_iter()
        .map(ResponseItemEnvelope::into_item)
        .collect()
}

fn user_message(text: &str) -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: text.to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    }
}

fn compacted_user_message(text: &str) -> CompactedUserMessage {
    CompactedUserMessage {
        id: None,
        message: text.to_string(),
        internal_chat_message_metadata_passthrough: None,
        harness_metadata: None,
    }
}

#[test]
fn content_items_to_text_joins_non_empty_segments() {
    let items = vec![
        ContentItem::InputText {
            text: "hello".to_string(),
        },
        ContentItem::OutputText {
            text: String::new(),
        },
        ContentItem::OutputText {
            text: "world".to_string(),
        },
    ];

    let joined = content_items_to_text(&items);

    assert_eq!(Some("hello\nworld".to_string()), joined);
}

#[test]
fn content_items_to_text_ignores_image_only_content() {
    let items = vec![ContentItem::InputImage {
        image_url: "file://image.png".to_string(),
        detail: Some(DEFAULT_IMAGE_DETAIL),
    }];

    let joined = content_items_to_text(&items);

    assert_eq!(None, joined);
}

#[test]
fn collect_user_messages_extracts_user_text_only() {
    let items = vec![
        ResponseItem::Message {
            id: Some(ResponseItemId::with_suffix("msg", "assistant")),
            role: "assistant".to_string(),
            content: vec![ContentItem::OutputText {
                text: "ignored".to_string(),
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        },
        ResponseItem::Message {
            id: Some(ResponseItemId::with_suffix("msg", "user")),
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "first".to_string(),
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        },
        ResponseItem::Other,
    ];

    let collected = collect_user_messages(&items);

    assert_eq!(
        vec![CompactedUserMessage {
            id: Some(ResponseItemId::with_suffix("msg", "user")),
            ..compacted_user_message("first")
        }],
        collected,
    );
}

#[test]
fn collect_annotated_user_messages_extracts_user_text_only() {
    let items = vec![
        ResponseItemEnvelope {
            item: user_message("first"),
            metadata: Some(CodexHarnessMetadata::default()),
        },
        ResponseItemEnvelope::new(ResponseItem::Other),
    ];

    let collected = collect_annotated_user_messages(&items, CompactedMessageIdentity::Preserve);

    assert_eq!(
        vec![CompactedUserMessage {
            id: None,
            message: "first".to_string(),
            internal_chat_message_metadata_passthrough: None,
            harness_metadata: Some(CodexHarnessMetadata::default()),
        }],
        collected
    );
}

#[test]
fn collect_user_messages_filters_session_prefix_entries() {
    let items = vec![
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: r#"# AGENTS.md instructions for project

<INSTRUCTIONS>
do things
</INSTRUCTIONS>"#
                    .to_string(),
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        },
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "<ENVIRONMENT_CONTEXT>cwd=/tmp</ENVIRONMENT_CONTEXT>".to_string(),
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        },
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "real user message".to_string(),
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        },
    ];

    let collected = collect_user_messages(&items);

    assert_eq!(vec![compacted_user_message("real user message")], collected);
}

#[test]
fn collect_user_messages_filters_legacy_warnings() {
    let items = vec![
        user_message(
            "Warning: The maximum number of unified exec processes you can keep open is 60 and you currently have 61 processes open. Reuse older processes or close them to prevent automatic pruning of old processes",
        ),
        user_message(
            "Warning: apply_patch was requested via exec_command. Use the apply_patch tool instead of exec_command.",
        ),
        user_message(
            "Warning: Your account was flagged for potentially high-risk cyber activity and this request was routed to gpt-5.2 as a fallback. To regain access to gpt-5.3-codex, apply for trusted access: https://chatgpt.com/cyber or learn more: https://developers.openai.com/codex/concepts/cyber-safety",
        ),
        user_message("real user message"),
    ];

    let collected = collect_user_messages(&items);

    assert_eq!(vec![compacted_user_message("real user message")], collected);
}

#[test]
fn build_token_limited_compacted_history_truncates_overlong_user_messages() {
    // Use a small truncation limit so the test remains fast while still validating
    // that oversized user content is truncated.
    let max_tokens = 16;
    let big = "word ".repeat(200);
    let user_message = CompactedUserMessage {
        id: Some(ResponseItemId::with_suffix("msg", "long-user")),
        message: big.clone(),
        internal_chat_message_metadata_passthrough: None,
        harness_metadata: Some(CodexHarnessMetadata::default()),
    };
    let history = super::build_compacted_history_with_limit(
        Vec::new(),
        std::slice::from_ref(&user_message),
        "SUMMARY",
        max_tokens,
    );
    assert_eq!(history.len(), 2);

    let truncated_message = &history[0].item;
    let summary_message = &history[1].item;

    let truncated_text = match truncated_message {
        ResponseItem::Message { role, content, .. } if role == "user" => {
            content_items_to_text(content).unwrap_or_default()
        }
        other => panic!("unexpected item in history: {other:?}"),
    };

    assert!(
        truncated_text.contains("tokens truncated"),
        "expected truncation marker in truncated user message"
    );
    assert!(
        !truncated_text.contains(&big),
        "truncated user message should not include the full oversized user text"
    );

    let summary_text = match summary_message {
        ResponseItem::Message { role, content, .. } if role == "user" => {
            content_items_to_text(content).unwrap_or_default()
        }
        other => panic!("unexpected item in history: {other:?}"),
    };
    assert_eq!(summary_text, "SUMMARY");
    assert_eq!(history[0].id(), user_message.id.as_ref());
    assert_eq!(history[0].metadata, Some(CodexHarnessMetadata::default()));
    assert_eq!(history[1].metadata, None);
}

#[test]
fn build_token_limited_compacted_history_appends_summary_message() {
    let initial_context: Vec<ResponseItemEnvelope> = Vec::new();
    let user_messages = vec![compacted_user_message("first user message")];
    let summary_text = "summary text";

    let history = build_compacted_history(initial_context, &user_messages, summary_text);
    assert!(
        !history.is_empty(),
        "expected compacted history to include summary"
    );

    let last = history.last().expect("history should have a summary entry");
    let summary = match &last.item {
        ResponseItem::Message { role, content, .. } if role == "user" => {
            content_items_to_text(content).unwrap_or_default()
        }
        other => panic!("expected summary message, found {other:?}"),
    };
    assert_eq!(summary, summary_text);
}

#[test]
fn build_compacted_history_preserves_user_message_passthrough_metadata() {
    let history = build_compacted_history(
        Vec::new(),
        &[CompactedUserMessage {
            id: Some(ResponseItemId::with_suffix("msg", "user")),
            message: "first user message".to_string(),
            internal_chat_message_metadata_passthrough: Some(
                InternalChatMessageMetadataPassthrough {
                    turn_id: Some("turn-1".to_string()),
                    content_item_kinds: Some(vec![
                        ContentItemKind("user.image".to_string()),
                        ContentItemKind("user.text".to_string()),
                        ContentItemKind("user.audio".to_string()),
                    ]),
                    ..Default::default()
                },
            ),
            harness_metadata: Some(CodexHarnessMetadata::default()),
        }],
        "summary text",
    );

    assert_eq!(
        history,
        vec![
            ResponseItemEnvelope {
                item: ResponseItem::Message {
                    id: Some(ResponseItemId::with_suffix("msg", "user")),
                    role: "user".to_string(),
                    content: vec![ContentItem::InputText {
                        text: "first user message".to_string(),
                    }],
                    phase: None,
                    internal_chat_message_metadata_passthrough: Some(
                        InternalChatMessageMetadataPassthrough {
                            turn_id: Some("turn-1".to_string()),
                            content_item_kinds: Some(vec![ContentItemKind(
                                "user.text".to_string()
                            )]),
                            ..Default::default()
                        },
                    ),
                },
                metadata: Some(CodexHarnessMetadata::default()),
            },
            ResponseItemEnvelope::new(ContextualUserFragment::into(CompactionSummary::new(
                "summary text",
            ))),
        ]
    );
}

#[test]
fn insert_initial_context_before_last_real_user_or_summary_keeps_summary_last() {
    let agent_completion = ResponseItem::AgentMessage {
        id: None,
        author: "child".to_string(),
        recipient: "parent".to_string(),
        content: vec![AgentMessageInputContent::InputText {
            text: "Message Type: FINAL_ANSWER\nPayload:\nchild completion".to_string(),
        }],
        internal_chat_message_metadata_passthrough: None,
    };
    let compacted_history = vec![
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "older user".to_string(),
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        },
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "latest user".to_string(),
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        },
        agent_completion.clone(),
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: format!("{SUMMARY_PREFIX}\nsummary text"),
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        },
    ];
    let initial_context = vec![ResponseItem::Message {
        id: None,
        role: "developer".to_string(),
        content: vec![ContentItem::InputText {
            text: "fresh permissions".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    }];

    let refreshed = raw(insert_initial_context_before_last_real_user_or_summary(
        annotated(compacted_history),
        annotated(initial_context),
    ));
    let expected = vec![
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "older user".to_string(),
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        },
        ResponseItem::Message {
            id: None,
            role: "developer".to_string(),
            content: vec![ContentItem::InputText {
                text: "fresh permissions".to_string(),
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        },
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "latest user".to_string(),
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        },
        agent_completion,
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: format!("{SUMMARY_PREFIX}\nsummary text"),
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        },
    ];
    assert_eq!(refreshed, expected);
}

#[test]
fn insert_initial_context_before_last_real_user_or_summary_keeps_compaction_last() {
    let agent_task = ResponseItem::AgentMessage {
        id: None,
        author: "parent".to_string(),
        recipient: "child".to_string(),
        content: Vec::new(),
        internal_chat_message_metadata_passthrough: None,
    };
    let compacted_history = vec![
        agent_task.clone(),
        ResponseItem::Compaction {
            id: None,
            encrypted_content: "encrypted".to_string(),
            internal_chat_message_metadata_passthrough: None,
        },
    ];
    let initial_context = vec![ResponseItem::Message {
        id: None,
        role: "developer".to_string(),
        content: vec![ContentItem::InputText {
            text: "fresh permissions".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    }];

    let refreshed = raw(insert_initial_context_before_last_real_user_or_summary(
        annotated(compacted_history),
        annotated(initial_context),
    ));
    let expected = vec![
        ResponseItem::Message {
            id: None,
            role: "developer".to_string(),
            content: vec![ContentItem::InputText {
                text: "fresh permissions".to_string(),
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        },
        agent_task,
        ResponseItem::Compaction {
            id: None,
            encrypted_content: "encrypted".to_string(),
            internal_chat_message_metadata_passthrough: None,
        },
    ];
    assert_eq!(refreshed, expected);
}
