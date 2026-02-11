use std::time::Instant;

use tracing::error;

use crate::codex::{Session, ToolCallCtx};
use crate::protocol::EventMsg;
use crate::protocol::McpInvocation;
use crate::protocol::McpToolCallBeginEvent;
use crate::protocol::McpToolCallEndEvent;
use code_protocol::models::FunctionCallOutputPayload;
use code_protocol::models::ResponseInputItem;

/// Handles the specified tool call dispatches the appropriate
/// `McpToolCallBegin` and `McpToolCallEnd` events to the `Session`.
pub(crate) async fn handle_mcp_tool_call(
    sess: &Session,
    ctx: &ToolCallCtx,
    server: String,
    tool_name: String,
    arguments: String,
) -> ResponseInputItem {
    // Parse the `arguments` as JSON. An empty string is OK, but invalid JSON
    // is not.
    let arguments_value = if arguments.trim().is_empty() {
        None
    } else {
        match serde_json::from_str::<serde_json::Value>(&arguments) {
            Ok(value) => Some(value),
            Err(e) => {
                error!("failed to parse tool call arguments: {e}");
                return ResponseInputItem::FunctionCallOutput {
                    call_id: ctx.call_id.clone(),
                    output: FunctionCallOutputPayload {
                        body: code_protocol::models::FunctionCallOutputBody::Text(format!("err: {e}")),
                        success: Some(false)},
                };
            }
        }
    };

    let invocation = McpInvocation {
        server: server.clone(),
        tool: tool_name.clone(),
        arguments: arguments_value.clone(),
    };

    let tool_call_begin_event = EventMsg::McpToolCallBegin(McpToolCallBeginEvent { call_id: ctx.call_id.clone(), invocation: invocation.clone() });
    notify_mcp_tool_call_event(sess, ctx, tool_call_begin_event).await;

    let start = Instant::now();
    // Perform the tool call.
    let result = sess
        .call_tool(&server, &tool_name, arguments_value.clone(), None)
        .await
        .map_err(|e| format!("tool call error: {e}"));
    let protocol_result = result.clone().and_then(|value| {
        serde_json::to_value(value)
            .map_err(|e| format!("failed to encode MCP tool result: {e}"))
            .and_then(|json| {
                serde_json::from_value::<code_protocol::mcp::CallToolResult>(json)
                    .map_err(|e| format!("failed to decode MCP tool result: {e}"))
            })
    });
    let tool_call_end_event = EventMsg::McpToolCallEnd(McpToolCallEndEvent { call_id: ctx.call_id.clone(), invocation, duration: start.elapsed(), result: result.clone() });

    notify_mcp_tool_call_event(sess, ctx, tool_call_end_event.clone()).await;

    ResponseInputItem::McpToolCallOutput {
        call_id: ctx.call_id.clone(),
        result: protocol_result,
    }
}

async fn notify_mcp_tool_call_event(sess: &Session, ctx: &ToolCallCtx, event: EventMsg) {
    sess.send_ordered_from_ctx(ctx, event).await;
}
