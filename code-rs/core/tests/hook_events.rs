#![allow(clippy::unwrap_used)]

mod common;

use common::{load_default_config_for_test, wait_for_event};

use code_core::built_in_model_providers;
use code_core::config_types::{ProjectHookConfig, ProjectHookEvent};
use code_core::project_features::ProjectHooks;
use code_core::protocol::{AgentInfo, AgentSourceKind, AskForApproval, EventMsg, InputItem, Op, SandboxPolicy};
use code_core::{AgentStatusUpdatePayload, CodexAuth, ConversationManager, ModelProviderInfo, AGENT_MANAGER};
use serde_json::json;
use std::fs::{self, File};
use std::path::Path;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

static HOOK_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn hook_test_guard() -> std::sync::MutexGuard<'static, ()> {
    HOOK_TEST_LOCK
        .lock()
        .unwrap_or_else(|err| err.into_inner())
}

fn hook_cmd(log_path: &Path, label: &str) -> Vec<String> {
    vec![
        "bash".to_string(),
        "-lc".to_string(),
        format!(
            "printf '%s\\n' \"{}:$CODE_HOOK_EVENT\" >> {}",
            label,
            log_path.display()
        ),
    ]
}

fn hook_config_with_background(
    event: ProjectHookEvent,
    label: &str,
    log_path: &Path,
    run_in_background: bool,
) -> ProjectHookConfig {
    ProjectHookConfig {
        event,
        name: Some(label.to_string()),
        command: hook_cmd(log_path, label),
        cwd: None,
        env: None,
        timeout_ms: Some(1500),
        run_in_background: Some(run_in_background),
    }
}

fn hook_config(event: ProjectHookEvent, label: &str, log_path: &Path) -> ProjectHookConfig {
    hook_config_with_background(event, label, log_path, false)
}
fn sse_response(body: String) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("content-type", "text/event-stream")
        .set_body_string(body)
}

fn sse_message_body(message: &str, msg_id: &str, resp_id: &str) -> String {
    let message_item = json!({
        "type": "response.output_item.done",
        "item": {
            "type": "message",
            "id": msg_id,
            "role": "assistant",
            "content": [{"type": "output_text", "text": message}],
        }
    });
    let completed = json!({
        "type": "response.completed",
        "response": {
            "id": resp_id,
            "usage": {
                "input_tokens": 0,
                "input_tokens_details": null,
                "output_tokens": 0,
                "output_tokens_details": null,
                "total_tokens": 0
            }
        }
    });

    format!(
        "event: response.output_item.done\ndata: {}\n\n\
event: response.completed\ndata: {}\n\n",
        message_item, completed
    )
}

fn sse_function_call_body(call_id: &str, name: &str, args: serde_json::Value, resp_id: &str) -> String {
    let function_call_item = json!({
        "type": "response.output_item.done",
        "item": {
            "type": "function_call",
            "id": call_id,
            "call_id": call_id,
            "name": name,
            "arguments": args.to_string(),
        }
    });
    let completed = json!({
        "type": "response.completed",
        "response": {
            "id": resp_id,
            "usage": {
                "input_tokens": 0,
                "input_tokens_details": null,
                "output_tokens": 0,
                "output_tokens_details": null,
                "total_tokens": 0
            }
        }
    });
    format!(
        "event: response.output_item.done\ndata: {}\n\n\
event: response.completed\ndata: {}\n\n",
        function_call_item, completed
    )
}

async fn wait_for_log_contains(path: &Path, needle: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(contents) = fs::read_to_string(path) {
            if contents.contains(needle) {
                return;
            }
        }
        if Instant::now() > deadline {
            let contents = fs::read_to_string(path).unwrap_or_default();
            panic!(
                "timed out waiting for log entry: {} (contents: {})",
                needle, contents
            );
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn wait_for_event_with_timeout<F>(
    codex: &code_core::CodexConversation,
    timeout: Duration,
    mut predicate: F,
) -> Option<EventMsg>
where
    F: FnMut(&EventMsg) -> bool,
{
    let deadline = Instant::now() + timeout;
    loop {
        let now = Instant::now();
        if now >= deadline {
            return None;
        }
        let remaining = deadline.saturating_duration_since(now);
        let event = match tokio::time::timeout(remaining, codex.next_event()).await {
            Ok(Ok(event)) => event,
            Ok(Err(_)) => return None,
            Err(_) => return None,
        };
        if predicate(&event.msg) {
            return Some(event.msg);
        }
    }
}

fn base_config(code_home: &TempDir, project_dir: &TempDir) -> code_core::config::Config {
    let mut config = load_default_config_for_test(code_home);
    config.cwd = project_dir.path().to_path_buf();
    config.approval_policy = AskForApproval::Never;
    config.sandbox_policy = SandboxPolicy::DangerFullAccess;
    config
}

fn attach_mock_provider(config: &mut code_core::config::Config, server: &MockServer) {
    config.model_provider = ModelProviderInfo {
        base_url: Some(format!("{}/v1", server.uri())),
        ..built_in_model_providers()["openai"].clone()
    };
    config.model = "gpt-5.1-codex".to_string();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn session_start_end_hooks_fire() {
    let _guard = hook_test_guard();
    let code_home = TempDir::new().unwrap();
    let project_dir = TempDir::new().unwrap();
    let log_path = project_dir.path().join("hooks.log");
    File::create(&log_path).unwrap();

    let mut config = base_config(&code_home, &project_dir);
    let hook_configs = vec![
        hook_config(ProjectHookEvent::SessionStart, "start", &log_path),
        hook_config(ProjectHookEvent::SessionEnd, "end", &log_path),
    ];
    config.project_hooks = ProjectHooks::from_configs(&hook_configs, &config.cwd);

    let server = MockServer::start().await;
    attach_mock_provider(&mut config, &server);

    let conversation_manager = ConversationManager::with_auth(CodexAuth::from_api_key("Test API Key"));
    let codex = conversation_manager
        .new_conversation(config)
        .await
        .expect("create conversation")
        .conversation;

    wait_for_log_contains(&log_path, "start:session.start").await;

    codex.submit(Op::Shutdown).await.unwrap();
    let _ = wait_for_event(&codex, |msg| matches!(msg, EventMsg::ShutdownComplete)).await;
    wait_for_log_contains(&log_path, "end:session.end").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn user_prompt_hook_fires() {
    let _guard = hook_test_guard();
    let code_home = TempDir::new().unwrap();
    let project_dir = TempDir::new().unwrap();
    let log_path = project_dir.path().join("hooks.log");
    File::create(&log_path).unwrap();

    let mut config = base_config(&code_home, &project_dir);
    let hook_configs = vec![
        hook_config(ProjectHookEvent::UserPromptSubmit, "prompt", &log_path),
    ];
    config.project_hooks = ProjectHooks::from_configs(&hook_configs, &config.cwd);

    let server = MockServer::start().await;
    attach_mock_provider(&mut config, &server);

    let body = sse_message_body("ok", "msg-1", "resp-1");
    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(sse_response(body))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let conversation_manager = ConversationManager::with_auth(CodexAuth::from_api_key("Test API Key"));
    let codex = conversation_manager
        .new_conversation(config)
        .await
        .expect("create conversation")
        .conversation;

    codex
        .submit(Op::UserInput {
            items: vec![InputItem::Text { text: "hello".into() }],
        })
        .await
        .unwrap();

    let _ = wait_for_event(&codex, |msg| matches!(msg, EventMsg::TaskComplete(_))).await;
    wait_for_log_contains(&log_path, "prompt:user.prompt_submit").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stop_hook_fires() {
    let _guard = hook_test_guard();
    let code_home = TempDir::new().unwrap();
    let project_dir = TempDir::new().unwrap();
    let log_path = project_dir.path().join("hooks.log");
    File::create(&log_path).unwrap();

    let mut config = base_config(&code_home, &project_dir);
    let hook_configs = vec![hook_config(ProjectHookEvent::Stop, "stop", &log_path)];
    config.project_hooks = ProjectHooks::from_configs(&hook_configs, &config.cwd);

    let server = MockServer::start().await;
    attach_mock_provider(&mut config, &server);

    let body = sse_message_body("ok", "msg-1", "resp-1");
    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(sse_response(body))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let conversation_manager = ConversationManager::with_auth(CodexAuth::from_api_key("Test API Key"));
    let codex = conversation_manager
        .new_conversation(config)
        .await
        .expect("create conversation")
        .conversation;

    codex
        .submit(Op::UserInput {
            items: vec![InputItem::Text { text: "hello".into() }],
        })
        .await
        .unwrap();

    let _ = wait_for_event(&codex, |msg| matches!(msg, EventMsg::TaskComplete(_))).await;
    wait_for_log_contains(&log_path, "stop:stop").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn notification_hook_fires() {
    let _guard = hook_test_guard();
    let code_home = TempDir::new().unwrap();
    let project_dir = TempDir::new().unwrap();
    let log_path = project_dir.path().join("hooks.log");
    File::create(&log_path).unwrap();

    let mut config = base_config(&code_home, &project_dir);
    let hook_configs = vec![
        hook_config(ProjectHookEvent::SessionStart, "start", &log_path),
        hook_config_with_background(ProjectHookEvent::Notification, "notify", &log_path, true),
    ];
    config.project_hooks = ProjectHooks::from_configs(&hook_configs, &config.cwd);

    let server = MockServer::start().await;
    attach_mock_provider(&mut config, &server);

    let body = sse_message_body("ok", "msg-1", "resp-1");
    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(sse_response(body))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let conversation_manager = ConversationManager::with_auth(CodexAuth::from_api_key("Test API Key"));
    let codex = conversation_manager
        .new_conversation(config)
        .await
        .expect("create conversation")
        .conversation;

    wait_for_log_contains(&log_path, "start:session.start").await;

    codex
        .submit(Op::UserInput {
            items: vec![InputItem::Text { text: "hello".into() }],
        })
        .await
        .unwrap();

    let _ = wait_for_event(&codex, |msg| matches!(msg, EventMsg::TaskComplete(_))).await;
    wait_for_log_contains(&log_path, "notify:notification").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn file_before_after_write_hooks_fire() {
    let _guard = hook_test_guard();
    let code_home = TempDir::new().unwrap();
    let project_dir = TempDir::new().unwrap();
    let log_path = project_dir.path().join("hooks.log");
    File::create(&log_path).unwrap();

    let mut config = base_config(&code_home, &project_dir);
    let hook_configs = vec![
        hook_config(ProjectHookEvent::FileBeforeWrite, "before", &log_path),
        hook_config(ProjectHookEvent::FileAfterWrite, "after", &log_path),
    ];
    config.project_hooks = ProjectHooks::from_configs(&hook_configs, &config.cwd);

    let server = MockServer::start().await;
    attach_mock_provider(&mut config, &server);

    let patch = "*** Begin Patch\n*** Add File: hello.txt\n+hello\n*** End Patch";
    let script = format!("apply_patch <<'EOF'\n{patch}\nEOF");
    let call_args = json!({
        "command": ["bash", "-lc", script],
        "workdir": config.cwd,
        "timeout_ms": null,
        "sandbox_permissions": null,
        "justification": null,
    });

    let first_body = sse_function_call_body("call-1", "shell", call_args, "resp-1");
    let second_body = sse_message_body("done", "msg-1", "resp-2");

    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(sse_response(first_body))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(sse_response(second_body))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let conversation_manager = ConversationManager::with_auth(CodexAuth::from_api_key("Test API Key"));
    let codex = conversation_manager
        .new_conversation(config)
        .await
        .expect("create conversation")
        .conversation;

    codex
        .submit(Op::UserInput {
            items: vec![InputItem::Text {
                text: "apply patch".into(),
            }],
        })
        .await
        .unwrap();

    let _ = wait_for_event(&codex, |msg| matches!(msg, EventMsg::TaskComplete(_))).await;
    wait_for_log_contains(&log_path, "before:file.before_write").await;
    wait_for_log_contains(&log_path, "after:file.after_write").await;

    let created = project_dir.path().join("hello.txt");
    assert!(created.exists(), "apply_patch did not create file");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn precompact_hook_fires() {
    let _guard = hook_test_guard();
    let code_home = TempDir::new().unwrap();
    let project_dir = TempDir::new().unwrap();
    let log_path = project_dir.path().join("hooks.log");
    File::create(&log_path).unwrap();

    let mut config = base_config(&code_home, &project_dir);
    let hook_configs = vec![hook_config(ProjectHookEvent::PreCompact, "precompact", &log_path)];
    config.project_hooks = ProjectHooks::from_configs(&hook_configs, &config.cwd);

    let server = MockServer::start().await;
    attach_mock_provider(&mut config, &server);

    let body = sse_message_body("summary", "msg-1", "resp-1");
    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(sse_response(body))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let conversation_manager = ConversationManager::with_auth(CodexAuth::from_api_key("Test API Key"));
    let codex = conversation_manager
        .new_conversation(config)
        .await
        .expect("create conversation")
        .conversation;

    codex.submit(Op::Compact).await.unwrap();
    let _ = wait_for_event(&codex, |msg| matches!(msg, EventMsg::TaskComplete(_))).await;
    wait_for_log_contains(&log_path, "precompact:pre.compact").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn subagent_stop_hook_fires() {
    let _guard = hook_test_guard();
    let code_home = TempDir::new().unwrap();
    let project_dir = TempDir::new().unwrap();
    let log_path = project_dir.path().join("hooks.log");
    File::create(&log_path).unwrap();

    let mut config = base_config(&code_home, &project_dir);
    let hook_configs = vec![hook_config_with_background(
        ProjectHookEvent::SubagentStop,
        "subagent",
        &log_path,
        true,
    )];
    config.project_hooks = ProjectHooks::from_configs(&hook_configs, &config.cwd);

    let server = MockServer::start().await;
    attach_mock_provider(&mut config, &server);

    let conversation_manager = ConversationManager::with_auth(CodexAuth::from_api_key("Test API Key"));
    let codex = conversation_manager
        .new_conversation(config)
        .await
        .expect("create conversation")
        .conversation;

    tokio::time::sleep(Duration::from_millis(50)).await;

    let payload = AgentStatusUpdatePayload {
        agents: vec![AgentInfo {
            id: "agent-1".to_string(),
            name: "agent-1".to_string(),
            status: "completed".to_string(),
            batch_id: Some("batch-1".to_string()),
            model: Some("gpt-5.1-codex".to_string()),
            last_progress: None,
            result: Some("ok".to_string()),
            error: None,
            elapsed_ms: Some(1),
            token_count: None,
            last_activity_at: None,
            seconds_since_last_activity: None,
            source_kind: Some(AgentSourceKind::Default),
        }],
        context: None,
        task: None,
    };

    let manager = AGENT_MANAGER.read().await;
    let mut received = false;
    for _ in 0..5 {
        manager.emit_status_update(payload.clone());
        if wait_for_event_with_timeout(
            &codex,
            Duration::from_millis(400),
            |msg| matches!(msg, EventMsg::AgentStatusUpdate(_)),
        )
        .await
        .is_some()
        {
            received = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(received, "timed out waiting for AgentStatusUpdate event");
    wait_for_log_contains(&log_path, "subagent:subagent.stop").await;
}
