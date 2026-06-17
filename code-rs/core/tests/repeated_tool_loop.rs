#![allow(clippy::unwrap_used)]

mod common;

use code_core::built_in_model_providers;
use code_core::protocol::{AskForApproval, EventMsg, InputItem, Op, SandboxPolicy};
use code_core::{CodexAuth, ConversationManager, ModelProviderInfo};
use common::load_default_config_for_test;
use serde_json::json;
use serial_test::serial;
use tempfile::TempDir;
use tokio::time::{timeout, Duration};
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn sse_response(body: String) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("content-type", "text/event-stream")
        .set_body_string(body)
}

fn response_completed_event(id: &str) -> serde_json::Value {
    json!({
        "type": "response.completed",
        "response": {
            "id": id,
            "usage": {
                "input_tokens": 0,
                "input_tokens_details": null,
                "output_tokens": 0,
                "output_tokens_details": null,
                "total_tokens": 0
            }
        }
    })
}

fn sse_body(events: &[serde_json::Value]) -> String {
    events
        .iter()
        .map(|event| {
            let event_type = event["type"].as_str().expect("event type");
            format!("event: {event_type}\ndata: {event}\n\n")
        })
        .collect()
}

fn repeated_shell_call(project_dir: &TempDir) -> serde_json::Value {
    let function_call_args = json!({
        "command": ["bash", "-lc", "printf repeat-loop"],
        "workdir": project_dir.path(),
        "timeout_ms": null,
        "sandbox_permissions": null,
        "justification": null,
    });

    json!({
        "type": "response.output_item.done",
        "item": {
            "type": "function_call",
            "id": "call-repeat",
            "call_id": "call-repeat",
            "name": "shell",
            "arguments": function_call_args.to_string(),
        }
    })
}

async fn collect_until_task_complete(
    conversation: &code_core::CodexConversation,
) -> Vec<EventMsg> {
    let mut events = Vec::new();

    for _ in 0..80 {
        let event = timeout(Duration::from_secs(5), conversation.next_event())
            .await
            .expect("timeout waiting for event")
            .expect("event stream ended unexpectedly");
        let is_complete = matches!(event.msg, EventMsg::TaskComplete(_));
        events.push(event.msg);
        if is_complete {
            return events;
        }
    }

    panic!("did not receive TaskComplete event");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn repeated_identical_tool_cycle_stops_with_visible_error() {
    let code_home = TempDir::new().unwrap();
    let project_dir = TempDir::new().unwrap();
    let server = MockServer::start().await;

    let body_one = sse_body(&[
        repeated_shell_call(&project_dir),
        response_completed_event("resp-1"),
    ]);
    let body_two = sse_body(&[
        repeated_shell_call(&project_dir),
        response_completed_event("resp-2"),
    ]);

    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(sse_response(body_one))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(sse_response(body_two))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let mut config = load_default_config_for_test(&code_home);
    config.cwd = project_dir.path().to_path_buf();
    config.approval_policy = AskForApproval::Never;
    config.sandbox_policy = SandboxPolicy::DangerFullAccess;
    config.model_provider = ModelProviderInfo {
        base_url: Some(format!("{}/v1", server.uri())),
        ..built_in_model_providers(None)["openai"].clone()
    };
    config.model = "gpt-5.1-codex".to_string();

    let conversation = ConversationManager::with_auth(CodexAuth::from_api_key("Test API Key"))
        .new_conversation(config)
        .await
        .expect("create conversation")
        .conversation;

    conversation
        .submit(Op::UserInput {
            items: vec![InputItem::Text {
                text: "run once".to_string(),
            }],
            final_output_json_schema: None,
        })
        .await
        .unwrap();

    let events = collect_until_task_complete(&conversation).await;

    let error_message = events.iter().find_map(|event| match event {
        EventMsg::Error(error) => Some(error.message.as_str()),
        _ => None,
    });
    assert!(
        error_message.is_some_and(|message| {
            message.contains("repeated identical tool-use cycle detected")
                && message.contains("refusing to continue")
        }),
        "expected visible repeated-tool-cycle error, got events: {events:?}"
    );

    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests.len(),
        2,
        "duplicate cycle should stop before a third provider request"
    );
}
