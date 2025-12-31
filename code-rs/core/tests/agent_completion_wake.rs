#![allow(clippy::unwrap_used)]

mod common;

use common::load_default_config_for_test;

use code_core::config_types::AgentConfig;
use code_core::protocol::{AskForApproval, EventMsg, Op, SandboxPolicy};
use code_core::{built_in_model_providers, CodexAuth, ConversationManager};
use code_core::AGENT_MANAGER;
use code_protocol::config_types::ReasoningEffort;
use serde_json::json;
use serial_test::serial;
use tempfile::TempDir;
use tokio::time::{timeout, Duration, Instant};
use uuid::Uuid;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn sse_response(body: String) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("content-type", "text/event-stream")
        .set_body_string(body)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn wake_on_agent_batch_completion_starts_new_turn() {
    let code_home = TempDir::new().unwrap();
    let project_dir = TempDir::new().unwrap();

    let server = MockServer::start().await;

    let message_item = json!({
        "type": "response.output_item.done",
        "item": {
            "type": "message",
            "id": "msg-1",
            "role": "assistant",
            "content": [{"type": "output_text", "text": "ok"}],
        }
    });
    let completed = json!({
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
    let body = format!(
        "event: response.output_item.done\ndata: {message_item}\n\n\
event: response.completed\ndata: {completed}\n\n",
    );
    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(sse_response(body))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let mut config = load_default_config_for_test(&code_home);
    config.cwd = project_dir.path().to_path_buf();
    config.approval_policy = AskForApproval::Never;
    config.sandbox_policy = SandboxPolicy::DangerFullAccess;
    config.model = "gpt-5.1-codex".to_string();

    let mut provider = built_in_model_providers()["openai"].clone();
    provider.base_url = Some(format!("{}/v1", server.uri()));
    config.model_provider = provider;

    let conversation_manager = ConversationManager::with_auth(CodexAuth::from_api_key("Test API Key"));
    let codex = conversation_manager
        .new_conversation(config)
        .await
        .expect("create conversation")
        .conversation;

    codex
        .submit(Op::CancelAgents {
            batch_ids: Vec::new(),
            agent_ids: Vec::new(),
        })
        .await
        .unwrap();

    let ready_deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let remaining = ready_deadline.saturating_duration_since(Instant::now());
        let event = timeout(remaining, codex.next_event())
            .await
            .expect("timeout waiting for session readiness")
            .expect("event stream ended unexpectedly");

        if matches!(event.msg, EventMsg::AgentMessage(_)) {
            break;
        }

        if Instant::now() >= ready_deadline {
            panic!("did not observe readiness event before deadline");
        }
    }

    let batch_id = format!("batch-{}", Uuid::new_v4());
    let agent_config = AgentConfig {
        name: "echo-agent".to_string(),
        command: "/bin/echo".to_string(),
        args: Vec::new(),
        read_only: true,
        enabled: true,
        description: None,
        env: None,
        args_read_only: None,
        args_write: None,
        instructions: None,
    };

    let agent_id = {
        let mut manager = AGENT_MANAGER.write().await;
        manager
            .create_agent_with_config(
                "echo".to_string(),
                Some("echo-agent".to_string()),
                "wake test".to_string(),
                None,
                None,
                Vec::new(),
                true,
                Some(batch_id.clone()),
                agent_config,
                ReasoningEffort::Low,
            )
            .await
    };

    let deadline = Instant::now() + Duration::from_secs(6);
    let mut saw_completed = false;
    let mut saw_task_started = false;

    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let event = timeout(remaining, codex.next_event())
            .await
            .expect("timeout waiting for event")
            .expect("event stream ended unexpectedly");

        match event.msg {
            EventMsg::AgentStatusUpdate(status) => {
                if status.agents.iter().any(|agent| {
                    agent.id == agent_id && agent.status.eq_ignore_ascii_case("completed")
                }) {
                    saw_completed = true;
                }
            }
            EventMsg::TaskStarted => {
                saw_task_started = true;
            }
            _ => {}
        }

        if saw_completed && saw_task_started {
            break;
        }
    }

    codex.submit(Op::Shutdown).await.unwrap();

    assert!(saw_completed, "agent did not reach completed status");
    assert!(
        saw_task_started,
        "expected a new TaskStarted after agent completion"
    );
}
