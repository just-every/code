use std::collections::BTreeMap;
use std::sync::LazyLock;

use crate::codex::{Session, ToolCallCtx};
use crate::openai_tools::JsonSchema;
use crate::openai_tools::OpenAiTool;
use crate::openai_tools::ResponsesApiTool;
use crate::protocol::EventMsg;
use code_protocol::models::FunctionCallOutputPayload;
use code_protocol::models::ResponseInputItem;

// Use the canonical plan tool types from the protocol crate to ensure
// type-identity matches events transported via `code_protocol`.
pub use code_protocol::plan_tool::PlanItemArg;
pub use code_protocol::plan_tool::StepStatus;
pub use code_protocol::plan_tool::UpdatePlanArgs;

// Types for the TODO tool arguments matching codex-vscode/todo-mcp/src/main.rs

pub(crate) static PLAN_TOOL: LazyLock<OpenAiTool> = LazyLock::new(|| {
    let mut plan_item_props = BTreeMap::new();
    plan_item_props.insert(
        "step".to_string(),
        JsonSchema::String {
            description: None,
            allowed_values: None,
        },
    );
    plan_item_props.insert(
        "status".to_string(),
        JsonSchema::String {
            description: Some("One of: pending, in_progress, completed".to_string()),
            allowed_values: None,
        },
    );

    let plan_items_schema = JsonSchema::Array {
        description: Some("The list of steps".to_string()),
        items: Box::new(JsonSchema::Object {
            properties: plan_item_props,
            required: Some(vec!["step".to_string(), "status".to_string()]),
            additional_properties: Some(false.into()),
        }),
    };

    let mut properties = BTreeMap::new();
    properties.insert(
        "name".to_string(),
        JsonSchema::String {
            description: Some("2-5 word title describing the plan e.g. 'Fix Box Rendering'".to_string()),
            allowed_values: None,
        },
    );
    properties.insert("plan".to_string(), plan_items_schema);

    OpenAiTool::Function(ResponsesApiTool {
        name: "update_plan".to_string(),
        description: r#"Updates the task plan.
Provide an optional name and a list of plan items, each with a step and status.
At most one step can be in_progress at a time.
"#
        .to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["plan".to_string()]),
            additional_properties: Some(false.into()),
        },
    })
});

/// This function doesn't do anything useful. However, it gives the model a structured way to record its plan that clients can read and render.
/// So it's the _inputs_ to this function that are useful to clients, not the outputs and neither are actually useful for the model other
/// than forcing it to come up and document a plan (TBD how that affects performance).
pub(crate) async fn handle_update_plan(
    session: &Session,
    ctx: &ToolCallCtx,
    arguments: String,
) -> ResponseInputItem {
    match parse_update_plan_arguments(arguments, &ctx.call_id) {
        Ok(mut args) => {
            args.name = normalize_plan_name(args.name.take());
            let output = ResponseInputItem::FunctionCallOutput {
                call_id: ctx.call_id.clone(),
                output: FunctionCallOutputPayload {
                    body: code_protocol::models::FunctionCallOutputBody::Text("Plan updated".to_string()),
                    success: Some(true)},
            };
            session
                .send_ordered_from_ctx(ctx, EventMsg::PlanUpdate(args))
                .await;
            output
        }
        Err(output) => *output,
    }
}

fn parse_update_plan_arguments(
    arguments: String,
    call_id: &str,
) -> Result<UpdatePlanArgs, Box<ResponseInputItem>> {
    match serde_json::from_str::<UpdatePlanArgs>(&arguments) {
        Ok(args) => Ok(args),
        Err(e) => {
            let output = ResponseInputItem::FunctionCallOutput {
                call_id: call_id.to_string(),
                output: FunctionCallOutputPayload {
                    body: code_protocol::models::FunctionCallOutputBody::Text(format!("failed to parse function arguments: {e}")),
                    success: None},
            };
            Err(Box::new(output))
        }
    }
}

fn normalize_plan_name(name: Option<String>) -> Option<String> {
    let Some(name) = name.map(|value| value.trim().to_string()) else {
        return None;
    };

    if name.is_empty() {
        return None;
    }

    let canonicalized = canonicalize_word_boundaries(&name);
    let words: Vec<&str> = canonicalized.split_whitespace().collect();
    if words.is_empty() {
        return None;
    }

    Some(
        words
            .into_iter()
            .map(format_plan_word)
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn canonicalize_word_boundaries(input: &str) -> String {
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    let mut prev_char: Option<char> = None;
    let mut uppercase_run: usize = 0;

    while let Some(ch) = chars.next() {
        if ch.is_whitespace() || matches!(ch, '_' | '-' | '/' | ':' | '.') {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            prev_char = None;
            uppercase_run = 0;
            continue;
        }

        let next_char = chars.peek().copied();
        let mut split = false;

        if !current.is_empty() {
            if let Some(prev) = prev_char {
                if prev.is_ascii_lowercase() && ch.is_ascii_uppercase() {
                    split = true;
                } else if prev.is_ascii_uppercase()
                    && ch.is_ascii_uppercase()
                    && uppercase_run > 0
                    && next_char.map_or(false, |c| c.is_ascii_lowercase())
                {
                    split = true;
                }
            }
        }

        if split {
            tokens.push(std::mem::take(&mut current));
            uppercase_run = 0;
        }

        current.push(ch);

        if ch.is_ascii_uppercase() {
            uppercase_run += 1;
        } else {
            uppercase_run = 0;
        }

        prev_char = Some(ch);
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens.join(" ")
}

const KNOWN_ACRONYMS: &[&str] = &[
    "AI", "API", "CLI", "CPU", "DB", "GPU", "HTTP", "HTTPS", "ID", "LLM", "SDK", "SQL", "TUI", "UI", "UX",
];

fn format_plan_word(word: &str) -> String {
    if word.is_empty() {
        return String::new();
    }

    let uppercase = word.to_ascii_uppercase();
    if KNOWN_ACRONYMS.contains(&uppercase.as_str()) {
        return uppercase;
    }

    let mut chars = word.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };

    let mut formatted = String::new();
    formatted.extend(first.to_uppercase());
    formatted.push_str(&chars.flat_map(char::to_lowercase).collect::<String>());
    formatted
}

#[cfg(test)]
mod tests {
    use super::normalize_plan_name;

    #[test]
    fn drops_empty_names() {
        assert_eq!(normalize_plan_name(None), None);
        assert_eq!(normalize_plan_name(Some("   ".into())), None);
    }

    #[test]
    fn title_cases_snake_and_kebab_cases() {
        assert_eq!(
            normalize_plan_name(Some("add_cat_command_guard".into())),
            Some("Add Cat Command Guard".into())
        );
        assert_eq!(
            normalize_plan_name(Some("update-core-tui".into())),
            Some("Update Core TUI".into())
        );
    }

    #[test]
    fn handles_camel_case_and_acronyms() {
        assert_eq!(
            normalize_plan_name(Some("updateCoreAPIIntegration".into())),
            Some("Update Core API Integration".into())
        );
    }

    #[test]
    fn prioritizes_snake_and_kebab_cases() {
        assert_eq!(
            normalize_plan_name(Some("improve_tui_scroll_perf".into())),
            Some("Improve TUI Scroll Perf".into())
        );
        assert_eq!(
            normalize_plan_name(Some("fix-ui-drag".into())),
            Some("Fix UI Drag".into())
        );
    }

    #[test]
    fn keeps_acronyms_intact_when_present() {
        assert_eq!(
            normalize_plan_name(Some("ImproveTUIScrollPerf".into())),
            Some("Improve TUI Scroll Perf".into())
        );
        assert_eq!(
            normalize_plan_name(Some("http_server_cleanup".into())),
            Some("HTTP Server Cleanup".into())
        );
    }
}
