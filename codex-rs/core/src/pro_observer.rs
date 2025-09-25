use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::StreamExt;
use tokio::time::sleep;

use codex_protocol::models::{
    ContentItem,
    ReasoningItemContent,
    ReasoningItemReasoningSummary,
    ResponseItem,
};
use codex_protocol::models::{FunctionCallOutputPayload, ResponseInputItem};

use crate::agent_tool::{create_run_agent_tool, AgentStatus, AGENT_MANAGER};
use crate::client_common::{Prompt, ResponseEvent, ResponseStream};
use crate::codex::{Session, ToolCallCtx, PRO_SUBMISSION_ID};
use crate::environment_context::EnvironmentContext;
use crate::openai_model_info::get_model_info;
use crate::openai_tools::{create_assist_core_tool, create_pro_recommend_tool, create_pro_submit_user_tool, OpenAiTool};
use crate::protocol::{ProEvent, ProPhase};

/// Launch a background observer turn that mirrors the core session state.
pub(crate) async fn observe_now(sess: Arc<Session>, sub_id: String, reason: &'static str) {
    if !sess.pro_is_enabled() {
        return;
    }

    let tools = observer_tools(&sess, reason);
    let (core_items, observer_items) = build_observer_inputs(&sess);
    let mut prompt_input = Vec::new();
    prompt_input.extend(core_items);
    prompt_input.extend(observer_items);

    let prompt = Prompt {
        input: prompt_input,
        store: false,
        user_instructions: Some(include_str!("prompt_for_pro_observer.md").to_string()),
        environment_context: Some(EnvironmentContext::new(
            Some(sess.get_cwd().to_path_buf()),
            Some(sess.get_approval_policy()),
            Some(sess.get_sandbox_policy().clone()),
            Some(sess.get_user_shell()),
        )),
        tools,
        status_items: Vec::new(),
        base_instructions_override: None,
        include_additional_instructions: true,
        text_format: None,
        model_override: None,
        model_family_override: None,
        output_schema: None,
    };

    let timestamp = chrono::Local::now().format("%H:%M:%S").to_string();
    sess.emit_pro_event(
        &sub_id,
        ProEvent::DeveloperNote {
            turn_id: "observer_run".to_string(),
            note: format!("Observer ran at {} because {}", timestamp, reason),
            artifacts: Vec::new(),
        },
    )
    .await;

    let stream = match sess.model_client().stream(&prompt).await {
        Ok(stream) => stream,
        Err(err) => {
            sess
                .notify_background_event(&sub_id, format!("observer stream error: {}", err))
                .await;
            return;
        }
    };

    run_observer_stream(sess, stream, sub_id).await;
}

fn observer_tools(sess: &Session, reason: &'static str) -> Vec<OpenAiTool> {
    match reason {
        "activity" => vec![
            create_pro_recommend_tool(),
            crate::openai_tools::create_wait_tool(),
            crate::openai_tools::create_kill_tool(),
            create_run_agent_tool(),
        ],
        _ => {
            let mut tools = vec![
                create_pro_recommend_tool(),
                create_assist_core_tool(),
                crate::openai_tools::create_wait_tool(),
                crate::openai_tools::create_kill_tool(),
                create_run_agent_tool(),
            ];
            if sess.pro_autonomous_enabled() {
                tools.push(create_pro_submit_user_tool());
            }
            tools
        }
    }
}

async fn run_observer_stream(sess: Arc<Session>, mut stream: ResponseStream, sub_id: String) {
    let started_at = Instant::now();
    let mut actions: Vec<String> = Vec::new();
    let mut pending_outputs: Vec<ResponseInputItem> = Vec::new();

    while let Some(item) = stream.next().await {
        let Ok(event) = item else { break; };
        match event {
            ResponseEvent::Created => {}
            ResponseEvent::OutputTextDelta { .. } => {}
            ResponseEvent::OutputItemDone { item, sequence_number, output_index } => {
                handle_output_item(&sess, &sub_id, &mut actions, &mut pending_outputs, item, sequence_number, output_index).await;
            }
            ResponseEvent::Completed { .. } => {
                summarize_observer_run(&sess, &sub_id, started_at.elapsed(), &actions).await;
                break;
            }
            ResponseEvent::RateLimits(_) => {}
            _ => {}
        }
    }

    if !pending_outputs.is_empty() {
        let mut history = sess
            .pro_observer_history()
            .lock()
            .expect("poisoned lock");
        for output in pending_outputs {
            history.record_items([&ResponseItem::from(output)]);
        }
    }
}

async fn handle_output_item(
    sess: &Arc<Session>,
    sub_id: &str,
    actions: &mut Vec<String>,
    pending_outputs: &mut Vec<ResponseInputItem>,
    item: ResponseItem,
    sequence_number: Option<u64>,
    output_index: Option<u32>,
) {
    match &item {
        ResponseItem::FunctionCall { name, arguments, call_id, .. } => match name.as_str() {
            "pro_recommend" => {
                let (title, note) = parse_recommend(arguments);
                let full_note = if title.is_empty() {
                    note.clone()
                } else if note.is_empty() {
                    title.clone()
                } else {
                    format!("{}\n\n{}", title, note)
                };
                sess.emit_pro_event(
                    sub_id,
                    ProEvent::DeveloperNote {
                        turn_id: "observer".to_string(),
                        note: full_note,
                        artifacts: Vec::new(),
                    },
                )
                .await;
                actions.push(format!("recommendation: {}", title));
                record_observer_log(sess, &item);
                record_history_item(sess, &item);

                let output = ResponseInputItem::FunctionCallOutput {
                    call_id: call_id.clone(),
                    output: FunctionCallOutputPayload {
                        content: "ok".to_string(),
                        success: Some(true),
                    },
                };
                pending_outputs.push(output.clone());
                record_history_item(sess, &ResponseItem::from(output));
            }
            "assist_core" => {
                let instructions = parse_instructions(arguments);
                if !instructions.is_empty() {
                    let developer_text = format!("[Observer → Core]\n{}", instructions);
                    sess.add_pending_input(ResponseInputItem::Message {
                        role: "developer".to_string(),
                        content: vec![ContentItem::InputText {
                            text: developer_text.clone(),
                        }],
                    });
                    sess.emit_pro_event(
                        sub_id,
                        ProEvent::DeveloperNote {
                            turn_id: "observer".to_string(),
                            note: developer_text,
                            artifacts: Vec::new(),
                        },
                    )
                    .await;
                    actions.push("assist core".to_string());
                }
                record_observer_log(sess, &item);
                record_history_item(sess, &item);

                let output = ResponseInputItem::FunctionCallOutput {
                    call_id: call_id.clone(),
                    output: FunctionCallOutputPayload {
                        content: "ok".to_string(),
                        success: Some(true),
                    },
                };
                pending_outputs.push(output.clone());
                record_history_item(sess, &ResponseItem::from(output));
            }
            "wait" => {
                actions.push("wait".to_string());
                record_observer_log(sess, &item);
                record_history_item(sess, &item);

                let output = ResponseInputItem::FunctionCallOutput {
                    call_id: call_id.clone(),
                    output: FunctionCallOutputPayload {
                        content: "waiting".to_string(),
                        success: Some(true),
                    },
                };
                pending_outputs.push(output.clone());
                record_history_item(sess, &ResponseItem::from(output));
            }
            "agent_run" => {
                let ctx = ToolCallCtx::new(sub_id.to_string(), call_id.clone(), sequence_number, output_index);
                let output = crate::codex::handle_run_agent(sess, &ctx, arguments.clone()).await;
                pending_outputs.push(output.clone());
                record_observer_log(sess, &item);
                record_history_item(sess, &item);
                record_history_item(sess, &ResponseItem::from(output.clone()));
                actions.push("spawn agent".to_string());

                let sess_clone = Arc::clone(sess);
                let sub_after = sub_id.to_string();
                tokio::spawn(async move {
                    wait_for_agents().await;
                    sess_clone.observer_maybe_trigger(sub_after, true, "agents_complete");
                });
            }
            "pro_submit_user" => {
                if sess.pro_autonomous_enabled() {
                    if let Some(text) = parse_follow_up(arguments) {
                        sess.submit_follow_up_user_message(text.clone()).await;
                        actions.push(format!("follow-up: {}", text.chars().take(40).collect::<String>()));
                        let output = ResponseInputItem::FunctionCallOutput {
                            call_id: call_id.clone(),
                            output: FunctionCallOutputPayload {
                                content: "ok".to_string(),
                                success: Some(true),
                            },
                        };
                        pending_outputs.push(output.clone());
                        record_history_item(sess, &item);
                        record_history_item(sess, &ResponseItem::from(output));
                    }
                } else {
                    let output = ResponseInputItem::FunctionCallOutput {
                        call_id: call_id.clone(),
                        output: FunctionCallOutputPayload {
                            content: "autonomous disabled".to_string(),
                            success: Some(false),
                        },
                    };
                    pending_outputs.push(output.clone());
                    record_history_item(sess, &item);
                    record_history_item(sess, &ResponseItem::from(output));
                }
            }
            other => {
                let output = ResponseInputItem::FunctionCallOutput {
                    call_id: call_id.clone(),
                    output: FunctionCallOutputPayload {
                        content: format!("unsupported call: {}", other),
                        success: Some(false),
                    },
                };
                pending_outputs.push(output.clone());
                record_history_item(sess, &item);
                record_history_item(sess, &ResponseItem::from(output));
            }
        },
        ResponseItem::Message { role, .. } if role == "assistant" => {
                record_observer_log(sess, &item);
            record_history_item(sess, &item);
        }
        _ => {}
    }
}

async fn summarize_observer_run(sess: &Session, sub_id: &str, elapsed: Duration, actions: &[String]) {
    let mut lines = Vec::new();
    lines.push(format!("Observer completed in {}s", elapsed.as_secs()));
    if !actions.is_empty() {
        for action in actions {
            lines.push(format!("- {}", action));
        }
    }

    sess
        .emit_pro_event(
            sub_id,
            ProEvent::DeveloperNote {
                turn_id: "observer_summary".to_string(),
                note: lines.join("\n"),
                artifacts: Vec::new(),
            },
        )
        .await;

    sess
        .emit_pro_event(
            PRO_SUBMISSION_ID,
            ProEvent::Status {
                phase: if actions.is_empty() { ProPhase::Idle } else { ProPhase::Background },
                stats: crate::protocol::ProStats::default(),
            },
        )
        .await;
}

fn record_observer_log(sess: &Session, item: &ResponseItem) {
    let mut log = sess
        .pro_observer_log()
        .lock()
        .expect("poisoned lock");
    log.push(item.clone());
    if log.len() > 80 {
        let drain = log.len() - 80;
        log.drain(0..drain);
    }
}

fn record_history_item(sess: &Session, item: &ResponseItem) {
    let mut history = sess
        .pro_observer_history()
        .lock()
        .expect("poisoned lock");
    history.record_items([item]);
}

fn build_observer_inputs(sess: &Session) -> (Vec<ResponseItem>, Vec<ResponseItem>) {
    let model_family = sess.model_client().get_model_family();
    let info = get_model_info(&model_family);
    let tokens = info.map(|m| m.context_window).unwrap_or(200_000) as f64;
    let core_budget = (tokens * 0.40 * 4.0) as usize;
    let observer_budget = (tokens * 0.10 * 4.0) as usize;

    let history = sess.get_history_contents();
    let mut core = Vec::new();
    let mut used = 0usize;
    for item in history.iter().rev() {
        if let Some((len, converted)) = clone_for_prompt(item) {
            used += len;
            core.push(converted);
            if used >= core_budget {
                break;
            }
        }
    }
    core.reverse();

    let mut observer = Vec::new();
    let mut used_obs = 0usize;
    let log = sess
        .pro_observer_log()
        .lock()
        .expect("poisoned lock")
        .clone();
    for item in log.iter().rev() {
        if let Some((len, converted)) = clone_for_prompt(item) {
            used_obs += len;
            observer.push(converted);
            if used_obs >= observer_budget {
                break;
            }
        }
    }
    observer.reverse();

    (core, observer)
}

fn clone_for_prompt(item: &ResponseItem) -> Option<(usize, ResponseItem)> {
    summarize_item(item).map(|text| {
        let len = text.len().max(1);
        (
            len,
            ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText { text }],
            },
        )
    })
}

fn summarize_item(item: &ResponseItem) -> Option<String> {
    match item {
        ResponseItem::Message { role, content, .. } => summarize_message(role, content),
        ResponseItem::Reasoning {
            summary,
            content,
            encrypted_content,
            ..
        } => summarize_reasoning(summary, content, encrypted_content.as_deref()),
        ResponseItem::FunctionCall { name, arguments, .. } => {
            let snippet = truncate_text(arguments, 300);
            let body = format!("{name}({snippet})");
            Some(format_prefixed("assistant tool_call", &body))
        }
        ResponseItem::FunctionCallOutput { call_id, output } => {
            let snippet = truncate_text(&output.content, 300);
            let status = output.success.map(|s| if s { "ok" } else { "error" });
            let body = match status {
                Some(label) => format!("{call_id} [{label}] {snippet}"),
                None => format!("{call_id} {snippet}"),
            };
            Some(format_prefixed("tool result", &body))
        }
        ResponseItem::LocalShellCall { status, action, .. } => summarize_shell_call(status, action),
        ResponseItem::CustomToolCall {
            name,
            input,
            status,
            call_id,
            ..
        } => {
            let mut body = format!("{name}#{call_id}");
            if let Some(status) = status {
                body.push_str(&format!(" [{status}]"));
            }
            let snippet = truncate_text(input, 200);
            if !snippet.is_empty() {
                body.push(' ');
                body.push_str(&snippet);
            }
            Some(format_prefixed("assistant custom_tool", &body))
        }
        ResponseItem::CustomToolCallOutput { call_id, output } => {
            let snippet = truncate_text(output, 300);
            let body = format!("{call_id} {snippet}");
            Some(format_prefixed("tool custom_output", &body))
        }
        ResponseItem::WebSearchCall { action, .. } => match action {
            codex_protocol::models::WebSearchAction::Search { query } => {
                Some(format_prefixed("assistant web_search", query))
            }
            codex_protocol::models::WebSearchAction::Other => None,
        },
        ResponseItem::Other => None,
    }
}

fn summarize_message(role: &str, content: &[ContentItem]) -> Option<String> {
    let (mut segments, mut has_image) = (Vec::new(), false);
    for item in content {
        match item {
            ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                if !text.is_empty() {
                    segments.push(text.clone());
                }
            }
            ContentItem::InputImage { image_url } => {
                has_image = true;
                segments.push(format!("[image] {image_url}"));
            }
        }
    }

    if segments.is_empty() && !has_image {
        return None;
    }

    let body = segments.join("\n");
    Some(format_prefixed(role, &body))
}

fn summarize_reasoning(
    summary: &[ReasoningItemReasoningSummary],
    content: &Option<Vec<ReasoningItemContent>>,
    encrypted: Option<&str>,
) -> Option<String> {
    let mut segments = Vec::new();
    for ReasoningItemReasoningSummary::SummaryText { text } in summary {
        if !text.is_empty() {
            segments.push(truncate_text(text, 400));
        }
    }

    if let Some(items) = content {
        for entry in items {
            let text = match entry {
                ReasoningItemContent::ReasoningText { text }
                | ReasoningItemContent::Text { text } => text,
            };
            if !text.is_empty() {
                segments.push(truncate_text(text, 200));
            }
        }
    }

    if encrypted.is_some() {
        segments.push("<encrypted reasoning omitted>".to_string());
    }

    if segments.is_empty() {
        return None;
    }

    let body = segments.join("\n");
    Some(format_prefixed("assistant reasoning", &body))
}

fn summarize_shell_call(
    status: &codex_protocol::models::LocalShellStatus,
    action: &codex_protocol::models::LocalShellAction,
) -> Option<String> {
    match action {
        codex_protocol::models::LocalShellAction::Exec(exec) => {
            let joined = exec.command.join(" ");
            let mut body = format!("{joined}");
            if let Some(timeout) = exec.timeout_ms {
                body.push_str(&format!(" (timeout={}ms)", timeout));
            }
            if let Some(dir) = &exec.working_directory {
                body.push_str(&format!(" [cwd={dir}]"));
            }
            let status_label = format!("{status:?}").to_lowercase();
            let prefix = format!("assistant shell_{status_label}");
            let truncated = truncate_text(&body, 300);
            Some(format_prefixed(&prefix, &truncated))
        }
    }
}

fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        let truncated: String = text.chars().take(max_len).collect();
        format!("{truncated}...")
    }
}

fn format_prefixed(prefix: &str, body: &str) -> String {
    if body.is_empty() {
        format!("[{prefix}]")
    } else if body.contains('\n') {
        format!("[{prefix}]\n{body}")
    } else {
        format!("[{prefix}] {body}")
    }
}

fn parse_recommend(arguments: &str) -> (String, String) {
    serde_json::from_str::<serde_json::Value>(arguments)
        .ok()
        .map(|value| {
            let title = value
                .get("title")
                .and_then(|s| s.as_str())
                .unwrap_or("Recommendation")
                .to_string();
            let note = value
                .get("note")
                .and_then(|s| s.as_str())
                .unwrap_or_default()
                .to_string();
            (title, note)
        })
        .unwrap_or_else(|| ("Recommendation".to_string(), String::new()))
}

fn parse_instructions(arguments: &str) -> String {
    serde_json::from_str::<serde_json::Value>(arguments)
        .ok()
        .and_then(|value| value.get("instructions").and_then(|s| s.as_str()).map(|s| s.to_string()))
        .unwrap_or_default()
}

fn parse_follow_up(arguments: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(arguments)
        .ok()
        .and_then(|value| value.get("text").and_then(|s| s.as_str()).map(|s| s.to_string()))
}

async fn wait_for_agents() {
    loop {
        let manager = AGENT_MANAGER.read().await;
        let running = manager
            .get_all_agents()
            .any(|agent| matches!(agent.status, AgentStatus::Pending | AgentStatus::Running));
        drop(manager);
        if !running {
            break;
        }
        sleep(Duration::from_secs(2)).await;
    }
}
