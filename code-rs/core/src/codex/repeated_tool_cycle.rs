use crate::error::CodexErr;
use crate::error::Result as CodexResult;
use code_protocol::models::ResponseInputItem;
use code_protocol::models::ResponseItem;

#[derive(Clone, Debug, Eq, PartialEq)]
struct ToolCycleFingerprint {
    tool_calls: Vec<String>,
    tool_outputs: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct RepeatedToolCycleGuard {
    previous: Option<ToolCycleFingerprint>,
}

impl RepeatedToolCycleGuard {
    pub(super) fn check(
        &mut self,
        items: &[ResponseItem],
        responses: &[ResponseInputItem],
    ) -> CodexResult<()> {
        let Some(current) = ToolCycleFingerprint::from_turn(items, responses) else {
            self.previous = None;
            return Ok(());
        };

        if self.previous.as_ref() == Some(&current) {
            return Err(CodexErr::UnsupportedOperation(format!(
                "repeated identical tool-use cycle detected; refusing to continue after duplicate tool calls: {}",
                current.tool_calls.join(", ")
            )));
        }

        self.previous = Some(current);
        Ok(())
    }
}

impl ToolCycleFingerprint {
    fn from_turn(
        items: &[ResponseItem],
        responses: &[ResponseInputItem],
    ) -> Option<Self> {
        let tool_calls = items
            .iter()
            .filter_map(tool_call_fingerprint)
            .collect::<Vec<_>>();

        if tool_calls.is_empty() || responses.is_empty() {
            return None;
        }

        let tool_outputs = responses
            .iter()
            .filter_map(tool_output_fingerprint)
            .collect::<Vec<_>>();

        if tool_outputs.is_empty() {
            return None;
        }

        Some(Self {
            tool_calls,
            tool_outputs,
        })
    }
}

fn tool_call_fingerprint(item: &ResponseItem) -> Option<String> {
    match item {
        ResponseItem::FunctionCall {
            name,
            namespace,
            arguments,
            ..
        } => Some(
            serde_json::json!({
                "type": "function_call",
                "name": name,
                "namespace": namespace,
                "arguments": arguments,
            })
            .to_string(),
        ),
        ResponseItem::LocalShellCall { action, .. } => Some(
            serde_json::json!({
                "type": "local_shell_call",
                "action": action,
            })
            .to_string(),
        ),
        ResponseItem::CustomToolCall {
            name,
            namespace,
            input,
            ..
        } => Some(
            serde_json::json!({
                "type": "custom_tool_call",
                "name": name,
                "namespace": namespace,
                "input": input,
            })
            .to_string(),
        ),
        ResponseItem::ToolSearchCall {
            execution,
            arguments,
            ..
        } => Some(
            serde_json::json!({
                "type": "tool_search_call",
                "execution": execution,
                "arguments": arguments,
            })
            .to_string(),
        ),
        _ => None,
    }
}

fn tool_output_fingerprint(item: &ResponseInputItem) -> Option<String> {
    match item {
        ResponseInputItem::FunctionCallOutput { output, .. } => Some(
            serde_json::json!({
                "type": "function_call_output",
                "output": output,
            })
            .to_string(),
        ),
        ResponseInputItem::CustomToolCallOutput { name, output, .. } => Some(
            serde_json::json!({
                "type": "custom_tool_call_output",
                "name": name,
                "output": output,
            })
            .to_string(),
        ),
        ResponseInputItem::McpToolCallOutput { result, .. } => Some(
            serde_json::json!({
                "type": "mcp_tool_call_output",
                "result": result,
            })
            .to_string(),
        ),
        ResponseInputItem::ToolSearchOutput {
            status,
            execution,
            tools,
            ..
        } => Some(
            serde_json::json!({
                "type": "tool_search_output",
                "status": status,
                "execution": execution,
                "tools": tools,
            })
            .to_string(),
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_protocol::models::FunctionCallOutputPayload;

    fn shell_call(call_id: &str) -> ResponseItem {
        ResponseItem::FunctionCall {
            id: None,
            name: "shell".to_string(),
            namespace: None,
            arguments: r#"{"command":["bash","-lc","true"]}"#.to_string(),
            call_id: call_id.to_string(),
        }
    }

    fn shell_output(call_id: &str) -> ResponseInputItem {
        ResponseInputItem::FunctionCallOutput {
            call_id: call_id.to_string(),
            output: FunctionCallOutputPayload::from_text("ok".to_string()),
        }
    }

    #[test]
    fn detects_repeated_tool_cycle_even_when_call_id_changes() {
        let mut guard = RepeatedToolCycleGuard::default();

        guard
            .check(&[shell_call("call-1")], &[shell_output("call-1")])
            .unwrap();
        let err = guard
            .check(&[shell_call("call-2")], &[shell_output("call-2")])
            .unwrap_err();

        assert!(err.to_string().contains("repeated identical tool-use cycle"));
    }

    #[test]
    fn clears_after_non_tool_turn() {
        let mut guard = RepeatedToolCycleGuard::default();

        guard
            .check(&[shell_call("call-1")], &[shell_output("call-1")])
            .unwrap();
        guard.check(&[], &[]).unwrap();
        guard
            .check(&[shell_call("call-2")], &[shell_output("call-2")])
            .unwrap();
    }
}
