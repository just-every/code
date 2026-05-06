#![allow(clippy::unwrap_used)]

mod common;

use common::load_default_config_for_test;

use code_core::built_in_model_providers;
use code_core::config_types::{ProjectHookConfig, ProjectHookEvent};
use code_core::project_features::ProjectHooks;
use code_core::protocol::{AskForApproval, EventMsg, InputItem, Op, SandboxPolicy};
use code_core::{CodexAuth, CodexConversation, ConversationManager, ModelProviderInfo};
use serde_json::json;
use std::sync::Arc;
use std::fs::{self, File};
use tempfile::TempDir;
use tokio::time::{timeout, Duration};
use serial_test::serial;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn sse_response(body: String) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("content-type", "text/event-stream")
        .set_body_string(body)
}

fn assistant_message_event(text: &str) -> serde_json::Value {
    json!({
        "type": "response.output_item.done",
        "item": {
            "type": "message",
            "id": format!("msg-{text}"),
            "role": "assistant",
            "content": [{"type": "output_text", "text": text}],
        }
    })
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

fn hook_cmd(script: &str) -> Vec<String> {
    vec!["bash".to_string(), "-lc".to_string(), script.to_string()]
}

fn configure_test(
    code_home: &TempDir,
    project_dir: &TempDir,
    hook_configs: Vec<ProjectHookConfig>,
    server: &MockServer,
) -> code_core::config::Config {
    let mut config = load_default_config_for_test(code_home);
    config.cwd = project_dir.path().to_path_buf();
    config.approval_policy = AskForApproval::Never;
    config.sandbox_policy = SandboxPolicy::DangerFullAccess;
    config.project_hooks = ProjectHooks::from_configs(&hook_configs, &config.cwd);
    config.model_provider = ModelProviderInfo {
        base_url: Some(format!("{}/v1", server.uri())),
        ..built_in_model_providers(None)["openai"].clone()
    };
    config.model = "gpt-5.1-codex".to_string();
    config
}

async fn new_conversation(config: code_core::config::Config) -> Arc<CodexConversation> {
    ConversationManager::with_auth(CodexAuth::from_api_key("Test API Key"))
        .new_conversation(config)
        .await
        .expect("create conversation")
        .conversation
}

async fn submit_prompt(conversation: &Arc<CodexConversation>, text: &str) {
    conversation
        .submit(Op::UserInput {
            items: vec![InputItem::Text { text: text.into() }],
            final_output_json_schema: None,
        })
        .await
        .unwrap();
}

async fn collect_events_until_idle(conversation: &Arc<CodexConversation>) -> Vec<EventMsg> {
    let mut events = Vec::new();
    loop {
        match timeout(Duration::from_millis(500), conversation.next_event()).await {
            Ok(Ok(event)) => events.push(event.msg),
            Ok(Err(err)) => panic!("unexpected error receiving event: {err:?}"),
            Err(_) => break,
        }
    }
    events
}

async fn collect_events_until_task_complete(conversation: &Arc<CodexConversation>) -> Vec<EventMsg> {
    let mut events = Vec::new();
    let mut saw_task_complete = false;
    for _ in 0..40 {
        match timeout(Duration::from_secs(5), conversation.next_event()).await {
            Ok(Ok(event)) => {
                if matches!(event.msg, EventMsg::TaskComplete(_)) {
                    saw_task_complete = true;
                }
                events.push(event.msg);
                if saw_task_complete {
                    break;
                }
            }
            Ok(Err(err)) => panic!("unexpected error receiving event: {err:?}"),
            Err(_) => break,
        }
    }
    assert!(saw_task_complete, "did not receive TaskComplete event");
    events
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn tool_hooks_fire_for_shell_exec() {
    let code_home = TempDir::new().unwrap();
    let project_dir = TempDir::new().unwrap();
    let log_path = project_dir.path().join("hooks.log");
    File::create(&log_path).unwrap();

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

    let server = MockServer::start().await;

    let function_call_args = json!({
        "command": ["bash", "-lc", "echo exec-body"],
        "workdir": project_dir.path(),
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
    let body_one = sse_body(&[
        function_call_item,
        response_completed_event("resp-1"),
    ]);
    let body_two = sse_body(&[
        assistant_message_event("done"),
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

    let config = configure_test(&code_home, &project_dir, hook_configs, &server);
    let conversation = new_conversation(config).await;

    submit_prompt(&conversation, "run hook").await;
    let events = collect_events_until_task_complete(&conversation).await;

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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn user_prompt_submit_hook_fires_and_injects_context() {
    let code_home = TempDir::new().unwrap();
    let project_dir = TempDir::new().unwrap();
    let log_path = project_dir.path().join("prompt_hook.log");
    File::create(&log_path).unwrap();

    let hook_configs = vec![ProjectHookConfig {
        event: ProjectHookEvent::UserPromptSubmit,
        name: Some("prompt".to_string()),
        command: hook_cmd(&format!(
            "echo 'Injected hook context'; echo prompt:${{CODE_HOOK_EVENT}} >> {}",
            log_path.display()
        )),
        cwd: None,
        env: None,
        timeout_ms: None,
        run_in_background: Some(false),
    }];

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(sse_response(sse_body(&[
            assistant_message_event("done"),
            response_completed_event("resp-1"),
        ])))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let config = configure_test(&code_home, &project_dir, hook_configs, &server);
    let conversation = new_conversation(config).await;

    submit_prompt(&conversation, "hello world").await;
    let events = collect_events_until_task_complete(&conversation).await;

    assert!(events.iter().any(|msg| match msg {
        EventMsg::ExecCommandBegin(ev) => ev.call_id.contains("_hook_user_prompt_submit_1"),
        _ => false,
    }));

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1, "expected a single model request");
    let body = String::from_utf8_lossy(&requests[0].body);
    assert!(body.contains("Injected hook context"));
    assert!(body.contains("hello world"));

    let log_contents = fs::read_to_string(&log_path).unwrap();
    assert!(log_contents.contains("prompt:user.prompt_submit"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn blocked_user_prompt_submit_surfaces_and_skips_model_request() {
    let code_home = TempDir::new().unwrap();
    let project_dir = TempDir::new().unwrap();
    let log_path = project_dir.path().join("blocked_prompt_hook.log");
    File::create(&log_path).unwrap();

    let hook_configs = vec![ProjectHookConfig {
        event: ProjectHookEvent::UserPromptSubmit,
        name: Some("prompt-block".to_string()),
        command: hook_cmd(&format!(
            "echo 'blocked by policy' >&2; echo prompt:${{CODE_HOOK_EVENT}} >> {}; exit 2",
            log_path.display()
        )),
        cwd: None,
        env: None,
        timeout_ms: None,
        run_in_background: Some(false),
    }];

    let server = MockServer::start().await;
    let config = configure_test(&code_home, &project_dir, hook_configs, &server);
    let conversation = new_conversation(config).await;

    submit_prompt(&conversation, "hello world").await;
    let events = collect_events_until_idle(&conversation).await;

    assert!(events.iter().any(|msg| match msg {
        EventMsg::ExecCommandEnd(ev) => {
            ev.call_id.contains("_hook_user_prompt_submit_1")
                && ev.stderr.contains("blocked by policy")
                && ev.exit_code == 2
        }
        EventMsg::BackgroundEvent(ev) => ev.message.contains("User prompt blocked by hook"),
        _ => false,
    }));
    assert!(
        !events.iter().any(|msg| matches!(msg, EventMsg::TaskStarted | EventMsg::TaskComplete(_))),
        "blocked prompt should not start a task"
    );

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 0, "blocked prompt should not reach the model");

    let log_contents = fs::read_to_string(&log_path).unwrap();
    assert!(log_contents.contains("prompt:user.prompt_submit"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn user_prompt_submit_payload_omits_multimodal_items() {
    let code_home = TempDir::new().unwrap();
    let project_dir = TempDir::new().unwrap();
    let payload_path = project_dir.path().join("prompt_payload.json");
    File::create(&payload_path).unwrap();
    let huge_data_url = format!("data:image/png;base64,{}", "A".repeat(20_000));

    let hook_configs = vec![ProjectHookConfig {
        event: ProjectHookEvent::UserPromptSubmit,
        name: Some("prompt-payload".to_string()),
        command: hook_cmd(&format!(
            "printf %s \"$CODE_HOOK_PAYLOAD\" > {}",
            payload_path.display()
        )),
        cwd: None,
        env: None,
        timeout_ms: None,
        run_in_background: Some(false),
    }];

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(sse_response(sse_body(&[
            assistant_message_event("done"),
            response_completed_event("resp-1"),
        ])))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let config = configure_test(&code_home, &project_dir, hook_configs, &server);
    let conversation = new_conversation(config).await;

    conversation
        .submit(Op::UserInput {
            items: vec![
                InputItem::Text {
                    text: "describe image".into(),
                },
                InputItem::Image {
                    image_url: huge_data_url.clone(),
                },
            ],
            final_output_json_schema: None,
        })
        .await
        .unwrap();
    collect_events_until_task_complete(&conversation).await;

    let payload = fs::read_to_string(&payload_path).unwrap();
    assert!(payload.contains("describe image"));
    assert!(!payload.contains("\"items\""));
    assert!(!payload.contains(&huge_data_url));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn stop_hook_fires_and_joins_continuation_prompts() {
    let code_home = TempDir::new().unwrap();
    let project_dir = TempDir::new().unwrap();
    let log_path = project_dir.path().join("stop_hook.log");
    File::create(&log_path).unwrap();

    let hook_configs = vec![
        ProjectHookConfig {
            event: ProjectHookEvent::Stop,
            name: Some("stop-a".to_string()),
            command: hook_cmd(&format!(
                "echo 'retry with tests' >&2; echo stop-a:${{CODE_HOOK_EVENT}} >> {}; exit 2",
                log_path.display()
            )),
            cwd: None,
            env: None,
            timeout_ms: None,
            run_in_background: Some(false),
        },
        ProjectHookConfig {
            event: ProjectHookEvent::Stop,
            name: Some("stop-b".to_string()),
            command: hook_cmd(&format!(
                "echo 'also mention lint' >&2; echo stop-b:${{CODE_HOOK_EVENT}} >> {}; exit 2",
                log_path.display()
            )),
            cwd: None,
            env: None,
            timeout_ms: None,
            run_in_background: Some(false),
        },
    ];

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(sse_response(sse_body(&[
            assistant_message_event("first pass complete"),
            response_completed_event("resp-1"),
        ])))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(sse_response(sse_body(&[
            assistant_message_event("second pass complete"),
            response_completed_event("resp-2"),
        ])))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let config = configure_test(&code_home, &project_dir, hook_configs, &server);
    let conversation = new_conversation(config).await;

    submit_prompt(&conversation, "finish this task").await;
    collect_events_until_task_complete(&conversation).await;

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2, "stop hook should trigger a continuation turn");
    let second_body = String::from_utf8_lossy(&requests[1].body);
    assert!(second_body.contains("retry with tests"));
    assert!(second_body.contains("also mention lint"));
    assert_eq!(
        second_body.matches("retry with tests").count(),
        1,
        "stop continuation prompt should only appear once in the follow-up turn"
    );
    assert_eq!(
        second_body.matches("also mention lint").count(),
        1,
        "joined stop continuation prompt should only appear once in the follow-up turn"
    );

    let log_contents = fs::read_to_string(&log_path).unwrap();
    assert!(log_contents.contains("stop-a:stop"));
    assert!(log_contents.contains("stop-b:stop"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn stop_hook_loop_guard_ignores_second_continuation_request() {
    let code_home = TempDir::new().unwrap();
    let project_dir = TempDir::new().unwrap();
    let log_path = project_dir.path().join("stop_guard_hook.log");
    File::create(&log_path).unwrap();

    let hook_configs = vec![ProjectHookConfig {
        event: ProjectHookEvent::Stop,
        name: Some("stop-loop".to_string()),
        command: hook_cmd(&format!(
            "echo 'retry forever' >&2; echo stop:${{CODE_HOOK_EVENT}} >> {}; exit 2",
            log_path.display()
        )),
        cwd: None,
        env: None,
        timeout_ms: None,
        run_in_background: Some(false),
    }];

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(sse_response(sse_body(&[
            assistant_message_event("first pass complete"),
            response_completed_event("resp-1"),
        ])))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(sse_response(sse_body(&[
            assistant_message_event("second pass complete"),
            response_completed_event("resp-2"),
        ])))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let config = configure_test(&code_home, &project_dir, hook_configs, &server);
    let conversation = new_conversation(config).await;

    submit_prompt(&conversation, "finish this task").await;
    let events = collect_events_until_task_complete(&conversation).await;

    assert!(events.iter().any(|msg| match msg {
        EventMsg::BackgroundEvent(ev) => ev.message.contains("Stop hook requested another continuation"),
        _ => false,
    }));

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2, "loop guard should cap stop continuations at one extra turn");
    let second_body = String::from_utf8_lossy(&requests[1].body);
    assert_eq!(
        second_body.matches("retry forever").count(),
        1,
        "loop-guarded continuation prompt should only appear once in the follow-up turn"
    );

    let log_contents = fs::read_to_string(&log_path).unwrap();
    assert_eq!(log_contents.lines().count(), 2, "stop hook should fire for both completions");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn session_start_and_stop_hooks_include_session_metadata() {
    let code_home = TempDir::new().unwrap();
    let project_dir = TempDir::new().unwrap();
    let log_path = project_dir.path().join("session_stop_payloads.log");
    File::create(&log_path).unwrap();

    let session_cmd = format!(
        "python3 - <<'PY'\nimport json, os\np=json.loads(os.environ['CODE_HOOK_PAYLOAD'])\nwith open({:?}, 'a', encoding='utf-8') as f:\n    f.write('session_start|{{}}|{{}}|{{}}\\n'.format(bool(p.get('session_id')), bool(p.get('transcript_path')), bool(p.get('model'))))\nPY",
        log_path
    );
    let stop_cmd = format!(
        "python3 - <<'PY'\nimport json, os\np=json.loads(os.environ['CODE_HOOK_PAYLOAD'])\nwith open({:?}, 'a', encoding='utf-8') as f:\n    f.write('stop|{{}}|{{}}|{{}}|{{}}\\n'.format(bool(p.get('session_id')), bool(p.get('transcript_path')), bool(p.get('model')), bool(p.get('turn_id'))))\nPY",
        log_path
    );

    let hook_configs = vec![
        ProjectHookConfig {
            event: ProjectHookEvent::SessionStart,
            name: Some("session-start".to_string()),
            command: hook_cmd(&session_cmd),
            cwd: None,
            env: None,
            timeout_ms: None,
            run_in_background: Some(false),
        },
        ProjectHookConfig {
            event: ProjectHookEvent::Stop,
            name: Some("stop-meta".to_string()),
            command: hook_cmd(&stop_cmd),
            cwd: None,
            env: None,
            timeout_ms: None,
            run_in_background: Some(false),
        },
    ];

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(sse_response(sse_body(&[
            assistant_message_event("done"),
            response_completed_event("resp-1"),
        ])))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let config = configure_test(&code_home, &project_dir, hook_configs, &server);
    let conversation = new_conversation(config).await;

    submit_prompt(&conversation, "hello world").await;
    collect_events_until_task_complete(&conversation).await;

    let log_contents = fs::read_to_string(&log_path).unwrap();
    assert!(log_contents.contains("session_start|True|True|True"));
    assert!(log_contents.contains("stop|True|True|True|True"));
}
