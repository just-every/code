use std::collections::BTreeMap;
use std::time::Duration;

use bytes::Bytes;
use code_otel::otel_event_manager::OtelEventManager;
use eventsource_stream::Eventsource;
use futures::Stream;
use futures::StreamExt;
use futures::TryStreamExt;
use reqwest::StatusCode;
use reqwest::header::HeaderMap;
use serde_json::Value;
use serde_json::json;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tracing::debug;
use tracing::trace;

use crate::auth::AuthManager;
use crate::ModelProviderInfo;
use crate::client_common::Prompt;
use crate::client_common::ResponseEvent;
use crate::client_common::ResponseStream;
use crate::debug_logger::DebugLogger;
use crate::error::CodexErr;
use crate::error::Result;
use crate::error::RetryLimitReachedError;
use crate::error::UnexpectedResponseError;
use crate::model_family::ModelFamily;
use crate::openai_tools::create_tools_json_for_chat_completions_api;
use crate::util::backoff;
use std::sync::{Arc, Mutex};
use code_app_server_protocol::AuthMode;
use code_protocol::models::ContentItem;
use code_protocol::models::ReasoningItemContent;
use code_protocol::models::ResponseItem;

/// Implementation for the classic Chat Completions API.
pub(crate) async fn stream_chat_completions(
    prompt: &Prompt,
    model_family: &ModelFamily,
    model_slug: &str,
    client: &reqwest::Client,
    provider: &ModelProviderInfo,
    debug_logger: &Arc<Mutex<DebugLogger>>,
    auth_manager: Option<Arc<AuthManager>>,
    otel_event_manager: Option<OtelEventManager>,
    log_tag: Option<&str>,
) -> Result<ResponseStream> {
    if prompt.output_schema.is_some() {
        return Err(CodexErr::UnsupportedOperation(
            "output_schema is not supported for Chat Completions API".to_string(),
        ));
    }

    // Build messages array
    let mut messages = Vec::<serde_json::Value>::new();

    let full_instructions = prompt.get_full_instructions(model_family);
    messages.push(json!({"role": "system", "content": full_instructions}));

    let input = prompt.get_formatted_input();

    // Pre-scan: map Reasoning blocks to the adjacent assistant anchor after the last user.
    // - If the last emitted message is a user message, drop all reasoning.
    // - Otherwise, for each Reasoning item after the last user message, attach it
    //   to the immediate previous assistant message (stop turns) or the immediate
    //   next assistant anchor (tool-call turns: function/local shell call, or assistant message).
    let mut reasoning_by_anchor_index: std::collections::HashMap<usize, String> =
        std::collections::HashMap::new();

    // Determine the last role that would be emitted to Chat Completions.
    let mut last_emitted_role: Option<&str> = None;
    for item in &input {
        match item {
            ResponseItem::Message { role, .. } => last_emitted_role = Some(role.as_str()),
            ResponseItem::FunctionCall { .. } | ResponseItem::LocalShellCall { .. } => {
                last_emitted_role = Some("assistant")
            }
            ResponseItem::FunctionCallOutput { .. } => last_emitted_role = Some("tool"),
            ResponseItem::CompactionSummary { .. } => last_emitted_role = Some("assistant"),
            ResponseItem::Reasoning { .. } | ResponseItem::Other => {}
            ResponseItem::CustomToolCall { .. } => {}
            ResponseItem::CustomToolCallOutput { .. } => {}
            ResponseItem::WebSearchCall { .. } => {}
        }
    }

    // Find the last user message index in the input.
    let mut last_user_index: Option<usize> = None;
    for (idx, item) in input.iter().enumerate() {
        if let ResponseItem::Message { role, .. } = item {
            if role == "user" {
                last_user_index = Some(idx);
            }
        }
    }

    // Attach reasoning only if the conversation does not end with a user message.
    if !matches!(last_emitted_role, Some("user")) {
        for (idx, item) in input.iter().enumerate() {
            // Only consider reasoning that appears after the last user message.
            if let Some(u_idx) = last_user_index {
                if idx <= u_idx {
                    continue;
                }
            }

            if let ResponseItem::Reasoning {
                content: Some(items),
                ..
            } = item
            {
                let mut text = String::new();
                for c in items {
                    match c {
                        ReasoningItemContent::ReasoningText { text: t }
                        | ReasoningItemContent::Text { text: t } => text.push_str(t),
                    }
                }
                if text.trim().is_empty() {
                    continue;
                }

                // Prefer immediate previous assistant message (stop turns)
                let mut attached = false;
                if idx > 0 {
                    if let ResponseItem::Message { role, .. } = &input[idx - 1] {
                        if role == "assistant" {
                            reasoning_by_anchor_index
                                .entry(idx - 1)
                                .and_modify(|v| v.push_str(&text))
                                .or_insert(text.clone());
                            attached = true;
                        }
                    }
                }

                // Otherwise, attach to immediate next assistant anchor (tool-calls or assistant message)
                if !attached && idx + 1 < input.len() {
                    match &input[idx + 1] {
                        ResponseItem::FunctionCall { .. } | ResponseItem::LocalShellCall { .. } => {
                            reasoning_by_anchor_index
                                .entry(idx + 1)
                                .and_modify(|v| v.push_str(&text))
                                .or_insert(text.clone());
                        }
                        ResponseItem::Message { role, .. } if role == "assistant" => {
                            reasoning_by_anchor_index
                                .entry(idx + 1)
                                .and_modify(|v| v.push_str(&text))
                                .or_insert(text.clone());
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Track last assistant text we emitted to avoid duplicate assistant messages
    // in the outbound Chat Completions payload (can happen if a final
    // aggregated assistant message was recorded alongside an earlier partial).
    let _last_assistant_text: Option<String> = None;

    for (idx, item) in input.iter().enumerate() {
        match item {
            ResponseItem::Message { role, content, .. } => {
                // If the message contains any images, we must use the
                // multi-modal array form supported by Chat Completions:
                //   [{ type: "text", text: "..." }, { type: "image_url", image_url: { url: "data:..." } }]
                let contains_image = content
                    .iter()
                    .any(|c| matches!(c, ContentItem::InputImage { .. }));

                if contains_image {
                    let mut parts = Vec::<serde_json::Value>::new();
                    for c in content {
                        match c {
                            ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                                parts.push(json!({ "type": "text", "text": text }));
                            }
                            ContentItem::InputImage { image_url } => {
                                parts.push(json!({
                                    "type": "image_url",
                                    "image_url": { "url": image_url }
                                }));
                            }
                        }
                    }
                    messages.push(json!({"role": role, "content": parts}));
                } else {
                    // Text-only messages can be sent as a single string for
                    // maximal compatibility with providers that only accept
                    // plain text in Chat Completions.
                    let mut text = String::new();
                    for c in content {
                        match c {
                            ContentItem::InputText { text: t }
                            | ContentItem::OutputText { text: t } => {
                                text.push_str(t);
                            }
                            _ => {}
                        }
                    }
                    messages.push(json!({"role": role, "content": text}));
                }
            }
            ResponseItem::CompactionSummary { .. } => {
                // Compaction summaries are only meaningful to the Responses API; omit them
                // when translating to Chat Completions.
                continue;
            }
            ResponseItem::FunctionCall {
                name,
                arguments,
                call_id,
                ..
            } => {
                let reasoning = reasoning_by_anchor_index.get(&idx).map(String::as_str);
                let tool_call = json!({
                    "id": call_id,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": arguments,
                    }
                });
                push_tool_call_message(&mut messages, tool_call, reasoning);
            }
            ResponseItem::LocalShellCall {
                id,
                call_id: _,
                status,
                action,
            } => {
                // Confirm with API team.
                let reasoning = reasoning_by_anchor_index.get(&idx).map(String::as_str);
                let tool_call = json!({
                    "id": id.clone().unwrap_or_default(),
                    "type": "local_shell_call",
                    "status": status,
                    "action": action,
                });
                push_tool_call_message(&mut messages, tool_call, reasoning);
            }
            ResponseItem::FunctionCallOutput { call_id, output } => {
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": call_id,
                    "content": output.content,
                }));
            }
            ResponseItem::CustomToolCall {
                id,
                call_id: _,
                name,
                input,
                status: _,
            } => {
                let reasoning = reasoning_by_anchor_index.get(&idx).map(String::as_str);
                let tool_call = json!({
                    "id": id,
                    "type": "custom",
                    "custom": {
                        "name": name,
                        "input": input,
                    }
                });
                push_tool_call_message(&mut messages, tool_call, reasoning);
            }
            ResponseItem::CustomToolCallOutput { call_id, output } => {
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": call_id,
                    "content": output,
                }));
            }
            ResponseItem::Reasoning { .. }
            | ResponseItem::WebSearchCall { .. }
            | ResponseItem::Other => {
                // Omit these items from the conversation history.
                continue;
            }
        }
    }

    let tools_json = create_tools_json_for_chat_completions_api(&prompt.tools)?;
    let mut payload = json!({
        "model": model_slug,
        "messages": messages,
        "stream": true,
        "tools": tools_json,
    });

    if let Some(openrouter_cfg) = provider.openrouter_config() {
        if let Some(obj) = payload.as_object_mut() {
            if let Some(provider_cfg) = &openrouter_cfg.provider {
                obj.insert(
                    "provider".to_string(),
                    serde_json::to_value(provider_cfg)?
                );
            }
            if let Some(route) = &openrouter_cfg.route {
                obj.insert("route".to_string(), route.clone());
            }
            for (key, value) in &openrouter_cfg.extra {
                obj.entry(key.clone()).or_insert(value.clone());
            }
        }
    }

    // If an Ollama context override is present, propagate it. Some Ollama
    // builds honor `num_ctx` directly in OpenAI-compatible Chat Completions,
    // and others accept it under an `options` object – include both.
    if let Ok(val) = std::env::var("CODEX_OLLAMA_NUM_CTX") {
        if let Ok(n) = val.parse::<u64>() {
            if let Some(obj) = payload.as_object_mut() {
                obj.insert("num_ctx".to_string(), json!(n));
                // Also set options.num_ctx for native-style compatibility.
                let mut options = serde_json::Map::new();
                options.insert("num_ctx".to_string(), json!(n));
                obj.entry("options").or_insert(json!(options));
            }
        }
    }

    let endpoint = provider.get_full_url(&None);
    debug!(
        "POST to {}: {}",
        endpoint,
        serde_json::to_string_pretty(&payload).unwrap_or_default()
    );

    let mut attempt = 0;
    let max_retries = provider.request_max_retries();
    let mut request_id = String::new();
    loop {
        attempt += 1;

        let auth = auth_manager.as_ref().and_then(|m| m.auth());
        let mut req_builder = provider.create_request_builder(client, &auth).await?;

        if let Some(auth) = auth.as_ref() {
            if auth.mode == AuthMode::ChatGPT {
                if let Some(account_id) = auth.get_account_id() {
                    req_builder = req_builder.header("chatgpt-account-id", account_id);
                }
            }
        }

        req_builder = req_builder
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .json(&payload);

        if request_id.is_empty() {
            let endpoint_for_log = provider.get_full_url(&auth);
            let header_snapshot = req_builder
                .try_clone()
                .and_then(|builder| builder.build().ok())
                .map(|req| header_map_to_json(req.headers()));

            if let Ok(logger) = debug_logger.lock() {
                request_id = logger
                    .start_request_log(
                        &endpoint_for_log,
                        &payload,
                        header_snapshot.as_ref(),
                        log_tag,
                    )
                    .unwrap_or_default();
            }
        }

        let res = req_builder.send().await;

        match res {
            Ok(resp) if resp.status().is_success() => {
                // Log successful response initiation
                if let Ok(logger) = debug_logger.lock() {
                    let _ = logger.append_response_event(
                        &request_id,
                        "stream_initiated",
                        &serde_json::json!({
                            "status": "success",
                            "status_code": resp.status().as_u16()
                        }),
                    );
                }
                let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent>>(1600);
                let stream = resp.bytes_stream().map_err(CodexErr::Reqwest);
                let debug_logger_clone = Arc::clone(&debug_logger);
                let request_id_clone = request_id.clone();
                tokio::spawn(process_chat_sse(
                    stream,
                    tx_event,
                    provider.stream_idle_timeout(),
                    debug_logger_clone,
                    request_id_clone,
                    otel_event_manager.clone(),
                ));
                return Ok(ResponseStream { rx_event });
            }
            Ok(res) => {
                let status = res.status();
                if !(status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()) {
                    let body = (res.text().await).unwrap_or_default();
                    if let Ok(logger) = debug_logger.lock() {
                        let _ = logger.append_response_event(
                            &request_id,
                            "error",
                            &serde_json::json!({
                                "status": status.as_u16(),
                                "body": body
                            }),
                        );
                        let _ = logger.end_request_log(&request_id);
                    }
                    return Err(CodexErr::UnexpectedStatus(UnexpectedResponseError {
                        status,
                        body,
                        request_id: None,
                    }));
                }

                if attempt > max_retries {
                    return Err(CodexErr::RetryLimit(RetryLimitReachedError {
                        status,
                        request_id: None,
                        retryable: status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS,
                    }));
                }

                let retry_after_secs = res
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok());

                let delay = retry_after_secs
                    .map(|s| Duration::from_millis(s * 1_000))
                    .unwrap_or_else(|| backoff(attempt));
                tokio::time::sleep(delay).await;
            }
            Err(e) => {
                let is_connectivity = e.is_connect() || e.is_timeout() || e.is_request();
                if attempt > max_retries {
                    if let Ok(logger) = debug_logger.lock() {
                        let _ = logger.append_response_event(
                            &request_id,
                            "network_error",
                            &serde_json::json!({
                                "error": e.to_string()
                            }),
                        );
                        let _ = logger.end_request_log(&request_id);
                    }
                    if is_connectivity {
                        let req_id = (!request_id.is_empty()).then(|| request_id.clone());
                        return Err(CodexErr::Stream(
                            format!("[transport] network unavailable: {e}"),
                            None,
                            req_id,
                        ));
                    }
                    return Err(e.into());
                }
                let delay = backoff(attempt);
                tokio::time::sleep(delay).await;
            }
        }
    }
}

fn push_tool_call_message(messages: &mut Vec<Value>, tool_call: Value, reasoning: Option<&str>) {
    // Chat Completions requires that tool calls are grouped into a single assistant message
    // (with `tool_calls: [...]`) followed by tool role responses.
    if let Some(Value::Object(obj)) = messages.last_mut()
        && obj.get("role").and_then(Value::as_str) == Some("assistant")
        && obj.get("content").is_some_and(Value::is_null)
        && let Some(tool_calls) = obj.get_mut("tool_calls").and_then(Value::as_array_mut)
    {
        tool_calls.push(tool_call);
        if let Some(reasoning) = reasoning {
            if let Some(Value::String(existing)) = obj.get_mut("reasoning") {
                if !existing.is_empty() {
                    existing.push('\n');
                }
                existing.push_str(reasoning);
            } else {
                obj.insert("reasoning".to_string(), Value::String(reasoning.to_string()));
            }
        }
        return;
    }

    let mut msg = json!({
        "role": "assistant",
        "content": null,
        "tool_calls": [tool_call],
    });
    if let Some(reasoning) = reasoning
        && let Some(obj) = msg.as_object_mut()
    {
        obj.insert("reasoning".to_string(), json!(reasoning));
    }
    messages.push(msg);
}

/// Lightweight SSE processor for the Chat Completions streaming format. The
/// output is mapped onto Codex's internal [`ResponseEvent`] so that the rest
/// of the pipeline can stay agnostic of the underlying wire format.
async fn process_chat_sse<S>(
    stream: S,
    tx_event: mpsc::Sender<Result<ResponseEvent>>,
    idle_timeout: Duration,
    debug_logger: Arc<Mutex<DebugLogger>>,
    request_id: String,
    otel_event_manager: Option<OtelEventManager>,
) where
    S: Stream<Item = Result<Bytes>> + Unpin,
{
    let mut stream = stream.eventsource();

    // State to accumulate a function call across streaming chunks.
    // OpenAI may split the `arguments` string over multiple `delta` events
    // until the chunk whose `finish_reason` is `tool_calls` is emitted. We
    // keep collecting the pieces here and forward a single
    // `ResponseItem::FunctionCall` once the call is complete.
    #[derive(Default)]
    struct FunctionCallState {
        name: Option<String>,
        arguments: String,
        call_id: Option<String>,
        active: bool,
    }

    let mut fn_call_state = FunctionCallState::default();
    let mut assistant_text = String::new();
    let mut reasoning_text = String::new();
    let mut current_item_id: Option<String> = None;

    async fn flush_and_complete(
        tx_event: &mpsc::Sender<Result<ResponseEvent>>,
        assistant_text: &mut String,
        reasoning_text: &mut String,
        current_item_id: &Option<String>,
        debug_logger: &Arc<Mutex<DebugLogger>>,
        request_id: &str,
    ) {
        // Emit any finalized items before closing so downstream consumers receive
        // terminal events for both assistant content and raw reasoning.
        if !assistant_text.is_empty() {
            let item = ResponseItem::Message {
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText {
                    text: std::mem::take(assistant_text),
                }],
                id: current_item_id.clone(),
            };
            let _ = tx_event
                .send(Ok(ResponseEvent::OutputItemDone {
                    item,
                    sequence_number: None,
                    output_index: None,
                }))
                .await;
        }

        if !reasoning_text.is_empty() {
            let item = ResponseItem::Reasoning {
                id: current_item_id.clone().unwrap_or_else(String::new),
                summary: Vec::new(),
                content: Some(vec![ReasoningItemContent::ReasoningText {
                    text: std::mem::take(reasoning_text),
                }]),
                encrypted_content: None,
            };
            let _ = tx_event
                .send(Ok(ResponseEvent::OutputItemDone {
                    item,
                    sequence_number: None,
                    output_index: None,
                }))
                .await;
        }

        let _ = tx_event
            .send(Ok(ResponseEvent::Completed {
                response_id: String::new(),
                token_usage: None,
            }))
            .await;
        if let Ok(logger) = debug_logger.lock() {
            let _ = logger.end_request_log(request_id);
        }
    }

    loop {
        let next_event = if let Some(manager) = otel_event_manager.as_ref() {
            manager
                .log_sse_event(|| timeout(idle_timeout, stream.next()))
                .await
        } else {
            timeout(idle_timeout, stream.next()).await
        };

        let sse = match next_event {
            Ok(Some(Ok(ev))) => ev,
            Ok(Some(Err(e))) => {
                let _ = tx_event
                    .send(Err(CodexErr::Stream(
                        format!("[transport] {e}"),
                        None,
                        Some(request_id.clone()),
                    )))
                    .await;
                return;
            }
            Ok(None) => {
                // Stream closed by server without an explicit end marker – log for diagnostics
                tracing::debug!("chat SSE stream closed without [DONE] marker");
                if let Ok(logger) = debug_logger.lock() {
                    let _ = logger.append_response_event(
                        &request_id,
                        "stream_closed_without_done",
                        &serde_json::json!({
                            "assistant_len": assistant_text.len(),
                            "reasoning_len": reasoning_text.len(),
                        }),
                    );
                }
                flush_and_complete(
                    &tx_event,
                    &mut assistant_text,
                    &mut reasoning_text,
                    &current_item_id,
                    &debug_logger,
                    &request_id,
                )
                .await;
                return;
            }
            Err(_) => {
                let _ = tx_event
                    .send(Err(CodexErr::Stream(
                        "[idle] timeout waiting for SSE".into(),
                        None,
                        Some(request_id.clone()),
                    )))
                    .await;
                return;
            }
        };

        let data = sse.data.trim();

        if data.is_empty() {
            continue;
        }

        // OpenAI Chat streaming sends a literal string "[DONE]" when finished.
        if data == "[DONE]" || data == "DONE" {
            flush_and_complete(
                &tx_event,
                &mut assistant_text,
                &mut reasoning_text,
                &current_item_id,
                &debug_logger,
                &request_id,
            )
            .await;
            return;
        }

        // Parse JSON chunk
        let chunk: serde_json::Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(e) => {
                // Surface parse errors to logs and debug logger for diagnostics, then skip
                let mut excerpt = sse.data.clone();
                const MAX: usize = 600;
                if excerpt.len() > MAX { excerpt.truncate(MAX); }
                tracing::debug!("chat SSE parse error: {} | data: {}", e, excerpt);
                if let Ok(logger) = debug_logger.lock() {
                    let _ = logger.append_response_event(
                        &request_id,
                        "sse_parse_error",
                        &serde_json::json!({
                            "error": e.to_string(),
                            "data_excerpt": excerpt,
                        }),
                    );
                }
                continue;
            }
        };
        trace!("chat_completions received SSE chunk: {chunk:?}");

        // Log the SSE chunk to debug log
        if let Ok(logger) = debug_logger.lock() {
            let _ = logger.append_response_event(&request_id, "sse_event", &chunk);
        }

        // Extract item_id if present at the top level or in choice
        if let Some(item_id) = chunk.get("item_id").and_then(|id| id.as_str()) {
            current_item_id = Some(item_id.to_string());
        }

        let choice_opt = chunk.get("choices").and_then(|c| c.get(0));

        if let Some(choice) = choice_opt {
            // Check for item_id in the choice as well
            if let Some(item_id) = choice.get("item_id").and_then(|id| id.as_str()) {
                current_item_id = Some(item_id.to_string());
            }

            // Handle assistant content tokens as streaming deltas.
            if let Some(content) = choice
                .get("delta")
                .and_then(|d| d.get("content"))
                .and_then(|c| c.as_str())
            {
                if !content.is_empty() {
                    assistant_text.push_str(content);
                    let _ = tx_event
                        .send(Ok(ResponseEvent::OutputTextDelta {
                            delta: content.to_string(),
                            item_id: current_item_id.clone(),
                            sequence_number: None,
                            output_index: None,
                        }))
                        .await;
                }
            }

            // Forward any reasoning/thinking deltas if present.
            // Some providers stream `reasoning` as a plain string while others
            // nest the text under an object (e.g. `{ "reasoning": { "text": "…" } }`).
            if let Some(reasoning_val) = choice.get("delta").and_then(|d| d.get("reasoning")) {
                let mut maybe_text = reasoning_val
                    .as_str()
                    .map(str::to_string)
                    .filter(|s| !s.is_empty());

                if maybe_text.is_none() && reasoning_val.is_object() {
                    if let Some(s) = reasoning_val
                        .get("text")
                        .and_then(|t| t.as_str())
                        .filter(|s| !s.is_empty())
                    {
                        maybe_text = Some(s.to_string());
                    } else if let Some(s) = reasoning_val
                        .get("content")
                        .and_then(|t| t.as_str())
                        .filter(|s| !s.is_empty())
                    {
                        maybe_text = Some(s.to_string());
                    }
                }

                if let Some(reasoning) = maybe_text {
                    // Accumulate so we can emit a terminal Reasoning item at the end.
                    reasoning_text.push_str(&reasoning);
                    let _ = tx_event
                        .send(Ok(ResponseEvent::ReasoningContentDelta {
                            delta: reasoning,
                            item_id: current_item_id.clone(),
                            sequence_number: None,
                            output_index: None,
                            content_index: None,
                        }))
                        .await;
                }
            }

            // Some providers only include reasoning on the final message object.
            if let Some(message_reasoning) = choice.get("message").and_then(|m| m.get("reasoning"))
            {
                // Accept either a plain string or an object with { text | content }
                if let Some(s) = message_reasoning.as_str() {
                    if !s.is_empty() {
                        reasoning_text.push_str(s);
                        let _ = tx_event
                            .send(Ok(ResponseEvent::ReasoningContentDelta {
                                delta: s.to_string(),
                                item_id: current_item_id.clone(),
                                sequence_number: None,
                                output_index: None,
                                content_index: None,
                            }))
                            .await;
                    }
                } else if let Some(obj) = message_reasoning.as_object() {
                    if let Some(s) = obj
                        .get("text")
                        .and_then(|v| v.as_str())
                        .or_else(|| obj.get("content").and_then(|v| v.as_str()))
                    {
                        if !s.is_empty() {
                            reasoning_text.push_str(s);
                            let _ = tx_event
                                .send(Ok(ResponseEvent::ReasoningContentDelta {
                                    delta: s.to_string(),
                                    item_id: current_item_id.clone(),
                                    sequence_number: None,
                                    output_index: None,
                                    content_index: None,
                                }))
                                .await;
                        }
                    }
                }
            }

            // Handle streaming function / tool calls.
            if let Some(tool_calls) = choice
                .get("delta")
                .and_then(|d| d.get("tool_calls"))
                .and_then(|tc| tc.as_array())
            {
                if let Some(tool_call) = tool_calls.first() {
                    // Mark that we have an active function call in progress.
                    fn_call_state.active = true;

                    // Extract call_id if present.
                    if let Some(id) = tool_call.get("id").and_then(|v| v.as_str()) {
                        fn_call_state.call_id.get_or_insert_with(|| id.to_string());
                    }

                    // Extract function details if present.
                    if let Some(function) = tool_call.get("function") {
                        if let Some(name) = function.get("name").and_then(|n| n.as_str()) {
                            fn_call_state.name.get_or_insert_with(|| name.to_string());
                        }

                        if let Some(args_fragment) =
                            function.get("arguments").and_then(|a| a.as_str())
                        {
                            fn_call_state.arguments.push_str(args_fragment);
                        }
                    }
                }
            }

            // Emit end-of-turn when finish_reason signals completion.
            if let Some(finish_reason) = choice.get("finish_reason").and_then(|v| v.as_str()) {
                match finish_reason {
                    "tool_calls" if fn_call_state.active => {
                        // First, flush the terminal raw reasoning so UIs can finalize
                        // the reasoning stream before any exec/tool events begin.
                        if !reasoning_text.is_empty() {
                            let item = ResponseItem::Reasoning {
                                id: current_item_id.clone().unwrap_or_else(String::new),
                                summary: Vec::new(),
                                content: Some(vec![ReasoningItemContent::ReasoningText {
                                    text: std::mem::take(&mut reasoning_text),
                                }]),
                                encrypted_content: None,
                            };
                            let _ = tx_event.send(Ok(ResponseEvent::OutputItemDone { item, sequence_number: None, output_index: None })).await;
                        }

                        // Then emit the FunctionCall response item.
                        let item = ResponseItem::FunctionCall {
                            id: current_item_id.clone(),
                            name: fn_call_state.name.clone().unwrap_or_else(|| "".to_string()),
                            arguments: fn_call_state.arguments.clone(),
                            call_id: fn_call_state.call_id.clone().unwrap_or_else(String::new),
                        };

                        let _ = tx_event.send(Ok(ResponseEvent::OutputItemDone { item, sequence_number: None, output_index: None })).await;
                    }
                    "stop" => {
                        // Regular turn without tool-call. Emit the final assistant message
                        // as a single OutputItemDone so non-delta consumers see the result.
                        if !assistant_text.is_empty() {
                            let item = ResponseItem::Message {
                                role: "assistant".to_string(),
                                content: vec![ContentItem::OutputText {
                                    text: std::mem::take(&mut assistant_text),
                                }],
                                id: current_item_id.clone(),
                            };
                            let _ = tx_event.send(Ok(ResponseEvent::OutputItemDone { item, sequence_number: None, output_index: None })).await;
                        }
                        // Also emit a terminal Reasoning item so UIs can finalize raw reasoning.
                        if !reasoning_text.is_empty() {
                            let item = ResponseItem::Reasoning {
                                id: current_item_id.clone().unwrap_or_else(String::new),
                                summary: Vec::new(),
                                content: Some(vec![ReasoningItemContent::ReasoningText {
                                    text: std::mem::take(&mut reasoning_text),
                                }]),
                                encrypted_content: None,
                            };
                            let _ = tx_event.send(Ok(ResponseEvent::OutputItemDone { item, sequence_number: None, output_index: None })).await;
                        }
                    }
                    _ => {}
                }

                // Emit Completed regardless of reason so the agent can advance.
                let _ = tx_event
                    .send(Ok(ResponseEvent::Completed {
                        response_id: String::new(),
                        token_usage: None,
                    }))
                    .await;

                // Prepare for potential next turn (should not happen in same stream).
                // fn_call_state = FunctionCallState::default();

                // Mark the request log as complete
                if let Ok(logger) = debug_logger.lock() {
                    let _ = logger.end_request_log(&request_id);
                }

                return; // End processing for this SSE stream.
            }
        }
    }
}

/// Optional client-side aggregation helper
///
/// Stream adapter that merges the incremental `OutputItemDone` chunks coming from
/// [`process_chat_sse`] into a *running* assistant message, **suppressing the
/// per-token deltas**.  The stream stays silent while the model is thinking
/// and only emits two events per turn:
///
///   1. `ResponseEvent::OutputItemDone` with the *complete* assistant message
///      (fully concatenated).
///   2. The original `ResponseEvent::Completed` right after it.
///
/// This mirrors the behaviour the TypeScript CLI exposes to its higher layers.
///
/// The adapter is intentionally *lossless*: callers who do **not** opt in via
/// [`AggregateStreamExt::aggregate()`] keep receiving the original unmodified
/// events.
#[derive(Copy, Clone, Eq, PartialEq)]
enum AggregateMode {
    AggregatedOnly,
    Streaming,
}
pub(crate) struct AggregatedChatStream<S> {
    inner: S,
    cumulative: String,
    cumulative_reasoning: String,
    cumulative_item_id: Option<String>,
    pending: std::collections::VecDeque<ResponseEvent>,
    mode: AggregateMode,
}

impl<S> Stream for AggregatedChatStream<S>
where
    S: Stream<Item = Result<ResponseEvent>> + Unpin,
{
    type Item = Result<ResponseEvent>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        // First, flush any buffered events from the previous call.
        if let Some(ev) = this.pending.pop_front() {
            return Poll::Ready(Some(Ok(ev)));
        }

        loop {
            match Pin::new(&mut this.inner).poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Ready(Some(Err(e))) => return Poll::Ready(Some(Err(e))),
                Poll::Ready(Some(Ok(ResponseEvent::OutputItemDone { item, sequence_number: _, .. }))) => {
                    // If this is an incremental assistant message chunk, accumulate but
                    // do NOT emit yet. Forward any other item (e.g. FunctionCall) right
                    // away so downstream consumers see it.

                    let is_assistant_delta = matches!(&item, code_protocol::models::ResponseItem::Message { role, .. } if role == "assistant");

                    if is_assistant_delta {
                        // Only use the final assistant message if we have not
                        // seen any deltas; otherwise, deltas already built the
                        // cumulative text and this would duplicate it.
                        if this.cumulative.is_empty() {
                            if let ResponseItem::Message { content, id, .. } = &item
                            {
                                // Capture the item_id if present
                                if let Some(item_id) = id {
                                    this.cumulative_item_id = Some(item_id.clone());
                                }
                                if let Some(text) = content.iter().find_map(|c| match c {
                                    ContentItem::OutputText { text } => Some(text),
                                    _ => None,
                                }) {
                                    this.cumulative.push_str(text);
                                }
                            }
                        }
                    }

                    // Also capture item_id from Reasoning items
                    if let ResponseItem::Reasoning { id, .. } = &item {
                        if !id.is_empty() {
                            this.cumulative_item_id = Some(id.clone());
                        }
                    }

                    // Not an assistant message – forward immediately.
                    return Poll::Ready(Some(Ok(ResponseEvent::OutputItemDone { item, sequence_number: None, output_index: None })));
                }
                Poll::Ready(Some(Ok(ResponseEvent::RateLimits(snapshot)))) => {
                    return Poll::Ready(Some(Ok(ResponseEvent::RateLimits(snapshot))));
                }
                Poll::Ready(Some(Ok(ResponseEvent::ServerReasoningIncluded(included)))) => {
                    return Poll::Ready(Some(Ok(ResponseEvent::ServerReasoningIncluded(included))));
                }
                Poll::Ready(Some(Ok(ResponseEvent::ModelsEtag(etag)))) => {
                    return Poll::Ready(Some(Ok(ResponseEvent::ModelsEtag(etag))));
                }
                Poll::Ready(Some(Ok(ResponseEvent::Completed {
                    response_id,
                    token_usage,
                }))) => {
                    // Build any aggregated items in the correct order: Reasoning first, then Message.
                    let mut emitted_any = false;

                    if !this.cumulative_reasoning.is_empty()
                        && matches!(this.mode, AggregateMode::AggregatedOnly)
                    {
                        let aggregated_reasoning = ResponseItem::Reasoning {
                            id: this.cumulative_item_id.clone().unwrap_or_else(String::new),
                            summary: Vec::new(),
                            content: Some(vec![
                                ReasoningItemContent::ReasoningText {
                                    text: std::mem::take(&mut this.cumulative_reasoning),
                                },
                            ]),
                            encrypted_content: None,
                        };
                        this.pending
                            .push_back(ResponseEvent::OutputItemDone { item: aggregated_reasoning, sequence_number: None, output_index: None });
                        emitted_any = true;
                    }

                    // Always emit the final aggregated assistant message when any
                    // content deltas have been observed. In AggregatedOnly mode this
                    // is the sole assistant output; in Streaming mode this finalizes
                    // the streamed deltas into a terminal OutputItemDone so callers
                    // can persist/render the message once per turn.
                    if !this.cumulative.is_empty() {
                        let aggregated_message = ResponseItem::Message {
                            id: this.cumulative_item_id.clone(),
                            role: "assistant".to_string(),
                            content: vec![code_protocol::models::ContentItem::OutputText {
                                text: std::mem::take(&mut this.cumulative),
                            }],
                        };
                        this.pending
                            .push_back(ResponseEvent::OutputItemDone { item: aggregated_message, sequence_number: None, output_index: None });
                        emitted_any = true;
                    }

                    // Always emit Completed last when anything was aggregated.
                    if emitted_any {
                        this.pending.push_back(ResponseEvent::Completed {
                            response_id: response_id.clone(),
                            token_usage: token_usage.clone(),
                        });
                        // Return the first pending event now.
                        if let Some(ev) = this.pending.pop_front() {
                            return Poll::Ready(Some(Ok(ev)));
                        }
                    }

                    // Nothing aggregated – forward Completed directly.
                    return Poll::Ready(Some(Ok(ResponseEvent::Completed {
                        response_id,
                        token_usage,
                    })));
                }
                Poll::Ready(Some(Ok(ResponseEvent::Created))) => {
                    // These events are exclusive to the Responses API and
                    // will never appear in a Chat Completions stream.
                    continue;
                }
                Poll::Ready(Some(Ok(ResponseEvent::OutputTextDelta { delta, item_id, sequence_number, .. }))) => {
                    // Always accumulate deltas so we can emit a final OutputItemDone at Completed.
                    this.cumulative.push_str(&delta);
                    // Capture the item_id if we haven't already
                    if item_id.is_some() && this.cumulative_item_id.is_none() {
                        this.cumulative_item_id = item_id.clone();
                    }
                    if matches!(this.mode, AggregateMode::Streaming) {
                        // In streaming mode, also forward the delta immediately.
                        return Poll::Ready(Some(Ok(ResponseEvent::OutputTextDelta {
                            delta,
                            item_id,
                            sequence_number,
                            output_index: None,
                        })));
                    } else {
                        continue;
                    }
                }
                Poll::Ready(Some(Ok(ResponseEvent::ReasoningContentDelta { delta, item_id, sequence_number, .. }))) => {
                    // Always accumulate reasoning deltas so we can emit a final Reasoning item at Completed.
                    this.cumulative_reasoning.push_str(&delta);
                    // Capture the item_id if we haven't already
                    if item_id.is_some() && this.cumulative_item_id.is_none() {
                        this.cumulative_item_id = item_id.clone();
                    }
                    if matches!(this.mode, AggregateMode::Streaming) {
                        // In streaming mode, also forward the delta immediately.
                        return Poll::Ready(Some(Ok(ResponseEvent::ReasoningContentDelta {
                            delta,
                            item_id,
                            sequence_number,
                            output_index: None,
                            content_index: None,
                        })));
                    } else {
                        continue;
                    }
                }
                Poll::Ready(Some(Ok(ResponseEvent::ReasoningSummaryDelta { .. }))) => {
                    continue;
                }
                Poll::Ready(Some(Ok(ResponseEvent::ReasoningSummaryPartAdded))) => {
                    continue;
                }
                Poll::Ready(Some(Ok(ResponseEvent::WebSearchCallBegin { call_id }))) => {
                    return Poll::Ready(Some(Ok(ResponseEvent::WebSearchCallBegin { call_id })));
                }
                Poll::Ready(Some(Ok(ResponseEvent::WebSearchCallCompleted { call_id, query }))) => {
                    return Poll::Ready(Some(Ok(ResponseEvent::WebSearchCallCompleted {
                        call_id,
                        query,
                    })));
                }
            }
        }
    }
}

/// Extension trait that activates aggregation on any stream of [`ResponseEvent`].
pub(crate) trait AggregateStreamExt: Stream<Item = Result<ResponseEvent>> + Sized {
    /// Returns a new stream that emits **only** the final assistant message
    /// per turn instead of every incremental delta.  The produced
    /// `ResponseEvent` sequence for a typical text turn looks like:
    ///
    /// ```ignore
    ///     OutputItemDone { item: <full message>, .. }
    ///     Completed
    /// ```
    ///
    /// No other `OutputItemDone` events will be seen by the caller.
    ///
    /// Usage:
    ///
    /// ```ignore
    /// let agg_stream = client.stream(&prompt).await?.aggregate();
    /// while let Some(event) = agg_stream.next().await {
    ///     // event now contains cumulative text
    /// }
    /// ```
    fn aggregate(self) -> AggregatedChatStream<Self> {
        AggregatedChatStream::new(self, AggregateMode::AggregatedOnly)
    }
}

impl<T> AggregateStreamExt for T where T: Stream<Item = Result<ResponseEvent>> + Sized {}

impl<S> AggregatedChatStream<S> {
    fn new(inner: S, mode: AggregateMode) -> Self {
        AggregatedChatStream {
            inner,
            cumulative: String::new(),
            cumulative_reasoning: String::new(),
            cumulative_item_id: None,
            pending: std::collections::VecDeque::new(),
            mode,
        }
    }

    pub(crate) fn streaming_mode(inner: S) -> Self {
        Self::new(inner, AggregateMode::Streaming)
    }
}

fn header_map_to_json(headers: &HeaderMap) -> serde_json::Value {
    let mut ordered: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (name, value) in headers.iter() {
        let entry = ordered.entry(name.as_str().to_string()).or_default();
        entry.push(value.to_str().unwrap_or_default().to_string());
    }

    serde_json::to_value(ordered).unwrap_or(serde_json::Value::Null)
}
