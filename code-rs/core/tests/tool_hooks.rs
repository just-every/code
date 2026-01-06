#![allow(clippy::unwrap_used)]

mod common;

use common::load_default_config_for_test;

use code_core::built_in_model_providers;
use code_core::config_types::{ProjectHookConfig, ProjectHookEvent};
use code_core::project_features::ProjectHooks;
use code_core::protocol::{AskForApproval, EventMsg, InputItem, Op, SandboxPolicy};
use code_core::{CodexAuth, ConversationManager, ModelProviderInfo};
use serde_json::json;
use std::fs::{self, File};
use tempfile::TempDir;
use tokio::time::timeout;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn sse_response(body: String) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("content-type", "text/event-stream")
        .set_body_string(body)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tool_hooks_fire_for_shell_exec() {
    let code_home = TempDir::new().unwrap();
    let project_dir = TempDir::new().unwrap();
    let log_path = project_dir.path().join("hooks.log");
    File::create(&log_path).unwrap();

    let mut config = load_default_config_for_test(&code_home);
    config.cwd = project_dir.path().to_path_buf();
    config.approval_policy = AskForApproval::Never;
    config.sandbox_policy = SandboxPolicy::DangerFullAccess;

    let hook_cmd = |label: &str| {
        vec![
            "bash".to_string(),
            "-lc".to_string(),
            format!("echo {label}:${{CODE_HOOK_EVENT}} >> {}", log_path.display()),
        ]
    };

    let hook_configs = vec![
        ProjectHookConfig {
            event: ProjectHookEvent::ToolBefore,
            name: Some("before".to_string()),
            command: hook_cmd("before"),
            cwd: None,
            env: None,
            timeout_ms: None,
            run_in_background: Some(false),
        },
        ProjectHookConfig {
            event: ProjectHookEvent::ToolAfter,
            name: Some("after".to_string()),
            command: hook_cmd("after"),
            cwd: None,
            env: None,
            timeout_ms: None,
            run_in_background: Some(false),
        },
    ];
    config.project_hooks = ProjectHooks::from_configs(&hook_configs, &config.cwd);

    let server = MockServer::start().await;

    let function_call_args = json!({
        "command": ["bash", "-lc", "echo exec-body"],
        "workdir": config.cwd,
        "timeout_ms": null,
        "sandbox_permissions": null,
        "justification": null,
    });
    let function_call_item = json!({
        "type": "response.output_item.done",
        "item": {
            "type": "function_call",
            "id": "call-1",
            "call_id": "call-1",
            "name": "shell",
            "arguments": function_call_args.to_string(),
        }
    });
    let completed_one = json!({
        "type": "response.completed",
        "response": {
            "id": "resp-1",
            "usage": {
                "input_tokens": 0,
                "input_tokens_details": null,
                "output_tokens": 0,
                "output_tokens_details": null,
                "total_tokens": 0
            }
        }
    });

    let body_one = format!(
        "event: response.output_item.done\ndata: {}\n\n\
event: response.completed\ndata: {}\n\n",
        function_call_item,
        completed_one
    );

    let message_item = json!({
        "type": "response.output_item.done",
        "item": {
            "type": "message",
            "id": "msg-1",
            "role": "assistant",
            "content": [{"type": "output_text", "text": "done"}],
        }
    });
    let completed_two = json!({
        "type": "response.completed",
        "response": {
            "id": "resp-2",
            "usage": {
                "input_tokens": 0,
                "input_tokens_details": null,
                "output_tokens": 0,
                "output_tokens_details": null,
                "total_tokens": 0
            }
        }
    });
    let body_two = format!(
        "event: response.output_item.done\ndata: {}\n\n\
event: response.completed\ndata: {}\n\n",
        message_item,
        completed_two
    );

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

    config.model_provider = ModelProviderInfo {
        base_url: Some(format!("{}/v1", server.uri())),
        ..built_in_model_providers()["openai"].clone()
    };
    config.model = "gpt-5.1-codex".to_string();

    let conversation_manager = ConversationManager::with_auth(CodexAuth::from_api_key("Test API Key"));
    let codex = conversation_manager
        .new_conversation(config)
        .await
        .expect("create conversation")
        .conversation;

    codex
        .submit(Op::UserInput {
            items: vec![InputItem::Text {
                text: "run hook".into(),
            }],
            final_output_json_schema: None,
        })
        .await
        .unwrap();

    let mut events = Vec::new();
    let mut saw_task_complete = false;
    for _ in 0..20 {
        match timeout(std::time::Duration::from_secs(5), codex.next_event()).await {
            Ok(Ok(event)) => {
                if matches!(event.msg, EventMsg::TaskComplete(_)) {
                    saw_task_complete = true;
                }
                events.push(event.msg.clone());
                if saw_task_complete {
                    break;
                }
            }
            Ok(Err(err)) => panic!("unexpected error receiving event: {err:?}"),
            Err(_) => break,
        }
    }

    assert!(saw_task_complete, "did not receive TaskComplete event");

    let hook_before_seen = events.iter().any(|msg| match msg {
        EventMsg::ExecCommandBegin(ev) => ev.call_id.contains("_hook_tool_before"),
        _ => false,
    });
    let hook_after_seen = events.iter().any(|msg| match msg {
        EventMsg::ExecCommandEnd(ev) => ev.call_id.contains("_hook_tool_after"),
        _ => false,
    });

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2, "expected two model requests (tool + follow-up)");

    assert!(hook_before_seen, "tool.before hook did not emit ExecCommandBegin");
    assert!(hook_after_seen, "tool.after hook did not emit ExecCommandEnd");

    let log_contents = fs::read_to_string(&log_path).unwrap();
    let lines: Vec<&str> = log_contents.lines().collect();
    assert!(lines.iter().any(|l| l.contains("before:tool.before")));
    assert!(lines.iter().any(|l| l.contains("after:tool.after")));
    assert!(lines.first().unwrap().contains("before"));
}
