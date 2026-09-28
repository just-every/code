//! Exercises confirmed Code Mode sends while root or worker Guardian reviews are pending.

use super::*;
use anyhow::Context;
use codex_history::RetainedContextEntry;
use codex_protocol::protocol::GuardianAssessmentStatus;
use core_test_support::streaming_sse::StreamingSseChunk;
use core_test_support::streaming_sse::start_streaming_sse_server;
use pretty_assertions::assert_eq;
use tokio::sync::oneshot;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn confirmed_root_delivery_invalidates_pending_worker_allow() -> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_wine_exec!(
        Ok(()),
        "Guardian approval actions require host-native paths"
    );

    let server = start_mock_server().await;
    let pending_delivery = Arc::new(Notify::new());
    let (release_delivery, receive_release) = std::sync::mpsc::channel();
    let pending_delivery_for_server = Arc::clone(&pending_delivery);
    let receive_release = std::sync::Mutex::new(receive_release);
    let test = code_mode_messaging_fixture(&server, &server, move || {
        pending_delivery_for_server.notify_one();
        receive_release
            .lock()
            .expect("delivery gate")
            .recv_timeout(Duration::from_secs(/*secs*/ 20))
            .expect("release confirmed delivery");
    })
    .await?;
    let root = test.session_configured.thread_id;
    let mut created_threads = test.thread_manager.subscribe_thread_created();

    mount_sse_once_match(
        &server,
        move |request: &wiremock::Request| {
            is_root_request(request, root) && contains_text(request, INITIAL_PROMPT)
        },
        sse(vec![
            ev_function_call_with_namespace(
                SPAWN_CALL_ID,
                "collaboration",
                "spawn_agent",
                &json!({"message": INITIAL_TASK, "task_name": "worker"}).to_string(),
            ),
            ev_completed("root-spawn"),
        ]),
    )
    .await;
    mount_sse_once_match(
        &server,
        move |request: &wiremock::Request| {
            is_root_request(request, root)
                && has_call_output(request, SPAWN_CALL_ID)
                && !has_call_output(request, MESSAGE_CALL_ID)
        },
        sse(vec![ev_completed("root-spawned")]),
    )
    .await;
    mount_sse_once_match(
        &server,
        move |request: &wiremock::Request| {
            is_root_request(request, root) && contains_text(request, "Ask me before deployment.")
        },
        sse(vec![
            code_mode_message(ROOT_ASSISTANT_REPLY),
            ev_completed("root-send"),
        ]),
    )
    .await;
    mount_completion(&server, root, MESSAGE_CALL_ID).await;

    let (release_worker, worker_gate) = oneshot::channel();
    let (worker_stream, _) = start_streaming_sse_server(vec![vec![StreamingSseChunk {
        gate: Some(worker_gate),
        body: sse(vec![
            ev_function_call(
                WORKER_CALL_ID,
                "exec_command",
                &json!({
                    "cmd": "true", "sandbox_permissions": "require_escalated",
                    "justification": "Review the production deployment."
                })
                .to_string(),
            ),
            ev_completed("worker-review"),
        ]),
    }]])
    .await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/v1/responses"))
        .and(move |request: &wiremock::Request| is_worker_request(request, root))
        .respond_with(
            wiremock::ResponseTemplate::new(/*s*/ 307)
                .insert_header("location", format!("{}/v1/responses", worker_stream.uri())),
        )
        .with_priority(/*priority*/ 1)
        .up_to_n_times(/*n*/ 1)
        .mount(&server)
        .await;
    let (release_review, review_gate) = oneshot::channel();
    let (review_stream, _) = start_streaming_sse_server(vec![vec![StreamingSseChunk {
        gate: Some(review_gate),
        body: sse(vec![
            ev_assistant_message(
                "guardian-assessment",
                &json!({
                    "risk_level": "low", "user_authorization": "high", "outcome": "allow",
                    "rationale": "The original root context authorized the command."
                })
                .to_string(),
            ),
            ev_completed("guardian-response"),
        ]),
    }]])
    .await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/v1/responses"))
        .and(|request: &wiremock::Request| {
            request_body(request)
                .is_some_and(|body| body["client_metadata"]["x-openai-subagent"] == "guardian")
        })
        .respond_with(
            wiremock::ResponseTemplate::new(/*s*/ 307)
                .insert_header("location", format!("{}/v1/responses", review_stream.uri())),
        )
        .with_priority(/*priority*/ 2)
        .up_to_n_times(/*n*/ 1)
        .mount(&server)
        .await;

    test.submit_text_turn(INITIAL_PROMPT).await?;
    let worker = test
        .thread_manager
        .get_thread(created_threads.recv().await?)
        .await?;
    release_worker.send(()).expect("release worker review");
    tokio::time::timeout(
        Duration::from_secs(/*secs*/ 10),
        review_stream.wait_for_request_count(/*count*/ 1),
    )
    .await
    .context("worker Guardian review did not start")?;
    let before = worker
        .guardian_root_snapshot()
        .await
        .expect("root review context")
        .review_context_revision;
    test.codex
        .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
            text: "Ask me before deployment.".to_owned(),
            text_elements: Vec::new(),
        }]))
        .await?;
    tokio::time::timeout(
        Duration::from_secs(/*secs*/ 10),
        pending_delivery.notified(),
    )
    .await
    .context("root messaging delivery did not start")?;
    release_delivery.send(()).expect("release confirmed send");
    tokio::time::timeout(Duration::from_secs(/*secs*/ 10), async {
        loop {
            let current = worker
                .guardian_root_snapshot()
                .await
                .expect("root review context")
                .review_context_revision;
            if current != before {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .context("confirmed root delivery did not revise worker review context")?;
    release_review.send(()).expect("release Guardian allow");
    let status = wait_for_event_match(&worker, |event| match event {
        EventMsg::GuardianAssessment(assessment)
            if assessment.status != GuardianAssessmentStatus::InProgress =>
        {
            Some(assessment.status)
        }
        _ => None,
    })
    .await;
    assert_eq!(status, GuardianAssessmentStatus::Aborted);
    let shutdown = test
        .thread_manager
        .shutdown_all_threads_bounded(Duration::from_secs(/*secs*/ 10))
        .await;
    assert!(shutdown.timed_out.is_empty());
    worker_stream.shutdown().await;
    review_stream.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn confirmed_delivery_invalidates_pending_local_allow() -> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_wine_exec!(
        Ok(()),
        "Guardian approval actions require host-native paths"
    );

    let server = start_mock_server().await;
    let messaging_server = start_mock_server().await;
    let pending_delivery = Arc::new(Notify::new());
    let (release_delivery, receive_release) = std::sync::mpsc::channel();
    let pending_delivery_for_server = Arc::clone(&pending_delivery);
    let receive_release = std::sync::Mutex::new(receive_release);
    let test = code_mode_messaging_fixture(&server, &messaging_server, move || {
        pending_delivery_for_server.notify_one();
        receive_release
            .lock()
            .expect("delivery gate")
            .recv_timeout(Duration::from_secs(/*secs*/ 20))
            .expect("release confirmed delivery");
    })
    .await?;
    let root = test.session_configured.thread_id;
    let output_file = test.cwd.path().join("stale-local-allow.txt");
    let output_path = shlex::try_join([output_file.to_string_lossy().as_ref()])?;
    let messaging_tool = code_mode_name_for_tool_name(&ToolName::namespaced(
        "mcp__codex_apps__user_message",
        "_send_message",
    ));
    let command = json!({
        "cmd": format!("printf should-not-run > {output_path}"),
        "sandbox_permissions": "require_escalated",
        "justification": "Review the production deployment."
    });
    let source = format!(
        "const message = tools.{messaging_tool}({}); \
         const command = tools.exec_command({command}); \
         text(await Promise.all([command, message]));",
        json!({"text": ROOT_ASSISTANT_REPLY}),
    );
    mount_sse_once_match(
        &server,
        move |request: &wiremock::Request| {
            is_root_request(request, root) && contains_text(request, "Ask me before deployment.")
        },
        sse(vec![
            ev_custom_tool_call(MESSAGE_CALL_ID, "exec", &source),
            ev_completed("root-parallel-calls"),
        ]),
    )
    .await;
    mount_completion(&server, root, MESSAGE_CALL_ID).await;

    let (release_review, review_gate) = oneshot::channel();
    let (review_stream, _) = start_streaming_sse_server(vec![vec![StreamingSseChunk {
        gate: Some(review_gate),
        body: sse(vec![
            ev_assistant_message(
                "local-guardian-allow",
                &json!({
                    "risk_level": "low", "user_authorization": "high", "outcome": "allow",
                    "rationale": "The original context authorized the command."
                })
                .to_string(),
            ),
            ev_completed("local-guardian-allow"),
        ]),
    }]])
    .await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/v1/responses"))
        .and(|request: &wiremock::Request| {
            request_body(request).is_some_and(|body| {
                body["client_metadata"]["x-openai-subagent"] == "guardian"
                    && !is_hosted_messaging_review_body(&body)
            })
        })
        .respond_with(
            wiremock::ResponseTemplate::new(/*s*/ 307)
                .insert_header("location", format!("{}/v1/responses", review_stream.uri())),
        )
        .with_priority(/*priority*/ 2)
        .up_to_n_times(/*n*/ 1)
        .mount(&server)
        .await;

    test.codex
        .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
            text: "Ask me before deployment.".to_owned(),
            text_elements: Vec::new(),
        }]))
        .await?;
    tokio::time::timeout(
        Duration::from_secs(/*secs*/ 10),
        pending_delivery.notified(),
    )
    .await
    .context("parallel messaging delivery did not start")?;
    tokio::time::timeout(
        Duration::from_secs(/*secs*/ 10),
        review_stream.wait_for_request_count(/*count*/ 1),
    )
    .await
    .context("local Guardian review did not start")?;
    release_delivery.send(()).expect("release confirmed send");
    tokio::time::timeout(Duration::from_secs(/*secs*/ 10), async {
        loop {
            let history = test.codex.conversation_history_snapshot().await;
            if history
                .retained_context()
                .expect("retained root context")
                .ordered_entries()
                .any(|(_, entry)| {
                    matches!(entry,
                    RetainedContextEntry::AssistantMessage(message)
                        if message.text == ROOT_ASSISTANT_REPLY)
                })
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .context("confirmed delivery was not retained during local review")?;
    release_review.send(()).expect("release stale local allow");
    let status = wait_for_event_match(&test.codex, |event| match event {
        EventMsg::GuardianAssessment(assessment)
            if assessment.status == GuardianAssessmentStatus::Aborted =>
        {
            Some(assessment.status)
        }
        _ => None,
    })
    .await;
    assert_eq!(status, GuardianAssessmentStatus::Aborted);
    wait_for_event(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;
    assert!(!output_file.exists(), "stale allow executed the command");
    test.codex.shutdown_and_wait().await?;
    review_stream.shutdown().await;
    Ok(())
}
