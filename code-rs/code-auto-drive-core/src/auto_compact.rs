use anyhow::{anyhow, Result};
use futures::StreamExt;
use std::time::Duration;
use tokio::time::timeout;

use code_core::codex::compact::{
    collect_compaction_snippets,
    make_compaction_summary_message,
    sanitize_items_for_compact,
};
use code_core::model_family::{derive_default_model_family, find_family_for_model};
use code_core::{content_items_to_text, ModelClient, Prompt, ResponseEvent, TextFormat};
use code_protocol::models::{ContentItem, ResponseItem};


const BYTES_PER_TOKEN: usize = 4;
const MAX_TRANSCRIPT_BYTES: usize = 32_000;
const MAX_COMMANDS_IN_SUMMARY: usize = 5;
const MAX_ACTION_LINES: usize = 5;
const SUMMARY_TIMEOUT_SECONDS: u64 = 45;

pub(crate) struct CheckpointSummary {
    pub message: ResponseItem,
    pub text: String,
}

pub(crate) fn compact_with_endpoint(
    runtime: &tokio::runtime::Runtime,
    client: &ModelClient,
    conversation: &[ResponseItem],
    model_slug: &str,
    compact_prompt: &str,
) -> Result<Vec<ResponseItem>> {
    let goal_marker = conversation
        .iter()
        .position(|item| matches!(item, ResponseItem::Message { role, .. } if role == "user"))
        .and_then(|idx| conversation.get(idx).cloned().map(|msg| (idx, msg)));

    let sanitized_input = sanitize_items_for_compact(conversation.to_vec());

    let prompt_instructions = compact_prompt.trim();
    let mut compacted = runtime
        .block_on(async {
            timeout(Duration::from_secs(SUMMARY_TIMEOUT_SECONDS), async {
                let mut prompt = Prompt::default();
                prompt.input = sanitized_input;
                prompt.include_additional_instructions = false;
                if !prompt_instructions.is_empty() {
                    prompt.base_instructions_override = Some(prompt_instructions.to_string());
                }
                prompt.model_override = Some(model_slug.to_string());
                let family = find_family_for_model(model_slug)
                    .unwrap_or_else(|| derive_default_model_family(model_slug));
                prompt.model_family_override = Some(family);
                prompt.set_log_tag("auto/remote-compact");
                client.compact_conversation_history(&prompt).await
            })
            .await
        })
        .map_err(|_| {
            anyhow!(
                "remote compaction request timed out after {SUMMARY_TIMEOUT_SECONDS}s"
            )
        })??;

    if let Some((goal_idx, goal_item)) = goal_marker {
        ensure_goal_is_present(&mut compacted, goal_item, goal_idx);
    }

    Ok(compacted)
}

fn ensure_goal_is_present(
    conversation: &mut Vec<ResponseItem>,
    goal_item: ResponseItem,
    original_idx: usize,
) {
    let Some(goal_item) = sanitize_items_for_compact(vec![goal_item]).into_iter().next() else {
        return;
    };
    let Some(goal_text) = message_text(&goal_item) else {
        return;
    };

    let already_present = conversation.iter().any(|item| {
        matches!(item, ResponseItem::Message { role, .. } if role == "user")
            && message_text(item).as_deref() == Some(goal_text.as_str())
    });

    if already_present {
        return;
    }

    let insert_at = original_idx.min(conversation.len());
    conversation.insert(insert_at, goal_item);
}

fn message_text(item: &ResponseItem) -> Option<String> {
    let ResponseItem::Message { content, .. } = item else {
        return None;
    };
    let text = content_items_to_text(content)?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub(crate) fn compute_slice_bounds(conversation: &[ResponseItem]) -> Option<(usize, usize)> {
    let goal_idx = conversation.iter().position(|item| {
        matches!(item, ResponseItem::Message { role, .. } if role == "user")
    })?;

    if conversation.len() <= goal_idx + 3 {
        return None;
    }

    let after_goal = &conversation[goal_idx + 1..];
    let token_counts: Vec<usize> = after_goal.iter().map(estimate_item_tokens).collect();
    let total_tokens: usize = token_counts.iter().sum();
    let mut midpoint = goal_idx + 1;

    if total_tokens > 0 {
        let target = (total_tokens + 1) / 2;
        let mut running = 0usize;
        for (offset, count) in token_counts.iter().enumerate() {
            running = running.saturating_add(*count);
            if running >= target {
                midpoint = goal_idx + 1 + offset;
                break;
            }
        }
    } else {
        midpoint = goal_idx + 1 + (after_goal.len() + 1) / 2;
    }

    let slice_start = goal_idx + 1;
    let slice_end = advance_to_turn_boundary(conversation, midpoint + 1);

    if slice_end <= slice_start {
        return None;
    }

    Some((slice_start, slice_end))
}

pub(crate) fn apply_compaction(
    conversation: &mut Vec<ResponseItem>,
    bounds: (usize, usize),
    prev_summary_text: Option<&str>,
    summary_message: ResponseItem,
) -> Option<()> {
    let goal_idx = conversation.iter().position(|item| {
        matches!(item, ResponseItem::Message { role, .. } if role == "user")
    })?;

    let (slice_start, slice_end) = bounds;
    if slice_start <= goal_idx || slice_end > conversation.len() {
        return None;
    }

    let mut rebuilt = Vec::with_capacity(conversation.len() - (slice_end - slice_start) + 2);
    rebuilt.extend_from_slice(&conversation[..=goal_idx]);

    if let Some(prev_text) = prev_summary_text.filter(|text| !text.trim().is_empty()) {
        rebuilt.push(make_compaction_summary_message(&[], prev_text));
    }

    rebuilt.push(summary_message);
    rebuilt.extend_from_slice(&conversation[slice_end..]);
    *conversation = rebuilt;
    Some(())
}

pub(crate) fn build_checkpoint_summary(
    runtime: &tokio::runtime::Runtime,
    client: &ModelClient,
    model_slug: &str,
    items: &[ResponseItem],
    prev_summary: Option<&str>,
    compact_prompt: &str,
) -> (CheckpointSummary, Option<String>) {
    let snippets = collect_compaction_snippets(items);
    let mut warning: Option<String> = None;
    let summary_text = match summarize_with_model(
        runtime,
        client,
        model_slug,
        items,
        prev_summary,
        compact_prompt,
    ) {
        Ok(text) if !text.trim().is_empty() => text,
        Ok(_) => deterministic_summary(items, prev_summary),
        Err(err) => {
            warning = Some(format!(
                "checkpoint summary model request failed: {err:#}"));
            deterministic_summary(items, prev_summary)
        }
    };

    let message = make_compaction_summary_message(&snippets, &summary_text);
    (CheckpointSummary { message, text: summary_text }, warning)
}

fn summarize_with_model(
    runtime: &tokio::runtime::Runtime,
    client: &ModelClient,
    model_slug: &str,
    items: &[ResponseItem],
    prev_summary: Option<&str>,
    compact_prompt: &str,
) -> Result<String> {
    let mut aggregate_summary = prev_summary
        .filter(|text| !text.trim().is_empty())
        .map(|text| text.to_string());

    let flattened = flatten_items(items);
    let chunks = chunk_text(&flattened);
    if chunks.is_empty() {
        return Err(anyhow!("empty transcript chunk"));
    }

    for chunk in chunks {
        if chunk.trim().is_empty() {
            continue;
        }

        let current_prev = aggregate_summary.as_deref();
        let summary = runtime.block_on(async {
            timeout(Duration::from_secs(SUMMARY_TIMEOUT_SECONDS), async {
                let mut prompt = Prompt::default();
                prompt.store = false;
                prompt.text_format = Some(TextFormat {
                    r#type: "text".to_string(),
                    name: None,
                    strict: None,
                    schema: None,
                });
                prompt.model_override = Some(model_slug.to_string());
                let family = find_family_for_model(model_slug)
                    .unwrap_or_else(|| derive_default_model_family(model_slug));
                prompt.model_family_override = Some(family);

                push_compaction_prompt(&mut prompt, compact_prompt);

                let mut user_text = String::new();
                if let Some(prev) = current_prev {
                    user_text.push_str("Previous checkpoint summary:\n");
                    user_text.push_str(prev);
                    user_text.push_str("\n\n");
                }
                user_text.push_str("Conversation slice:\n");
                user_text.push_str(&chunk);

                prompt.input.push(plain_message("user", user_text));

                let mut stream = client.stream(&prompt).await?;
                let mut collected = String::new();
                let mut response_items = Vec::new();

                while let Some(event) = stream.next().await {
                    match event {
                        Ok(ResponseEvent::OutputTextDelta { delta, .. }) => {
                            collected.push_str(&delta)
                        }
                        Ok(ResponseEvent::OutputItemDone { item, .. }) => {
                            response_items.push(item);
                        }
                        Ok(ResponseEvent::Completed { .. }) => break,
                        Ok(_) => {}
                        Err(err) => return Err(anyhow!(err)),
                    }
                }

                if let Some(message) = response_items.into_iter().find_map(|item| match item {
                    ResponseItem::Message { role, content, .. } if role == "assistant" => {
                        Some(content)
                    }
                    _ => None,
                }) {
                    let mut text = String::new();
                    for chunk in message {
                        if let ContentItem::OutputText { text: chunk_text } = chunk {
                            text.push_str(&chunk_text);
                        }
                    }
                    if !text.trim().is_empty() {
                        return Ok(text);
                    }
                }

                Ok(collected)
            })
            .await
        });

        let summary = match summary {
            Ok(result) => result?,
            Err(_) => {
                return Err(anyhow!(
                    "checkpoint summary request timed out after {SUMMARY_TIMEOUT_SECONDS}s"
                ));
            }
        };

        if !summary.trim().is_empty() {
            aggregate_summary = Some(summary);
        }
    }

    aggregate_summary.ok_or_else(|| anyhow!("empty summary"))
}

fn deterministic_summary(items: &[ResponseItem], prev_summary: Option<&str>) -> String {
    let mut actions = Vec::new();
    let mut commands = Vec::new();
    for item in items {
        match item {
            ResponseItem::Message { role, content, .. } => {
                let text = content
                    .iter()
                    .filter_map(|chunk| match chunk {
                        ContentItem::InputText { text }
                        | ContentItem::OutputText { text } => Some(text.trim()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                if text.is_empty() {
                    continue;
                }
                actions.push(format!("{}: {}", role, text));
                if role == "assistant" {
                    if let Some(cmd) = text.lines().find(|line| line.trim_start().starts_with('$')) {
                        commands.push(cmd.trim().to_string());
                    }
                }
            }
            ResponseItem::FunctionCall { name, .. } => {
                actions.push(format!("Tool call: {name}"));
            }
            ResponseItem::FunctionCallOutput { output, .. } => {
                actions.push(format!("Tool output: {}", output.content));
            }
            _ => {}
        }
    }

    let mut lines = Vec::new();
    if let Some(prev) = prev_summary.filter(|text| !text.trim().is_empty()) {
        lines.push(format!("Building on previous checkpoint: {}", prev));
    }
    lines.push(format!(
        "Checkpoint covers {} exchanges and {} tool events.",
        actions.len(),
        items.iter().filter(|item| matches!(item, ResponseItem::FunctionCall { .. })).count()
    ));
    if !commands.is_empty() {
        let display = commands
            .into_iter()
            .take(MAX_COMMANDS_IN_SUMMARY)
            .collect::<Vec<_>>()
            .join(" | ");
        lines.push(format!("Key commands: {}", display));
    }
    if !actions.is_empty() {
        let display = actions
            .into_iter()
            .take(MAX_ACTION_LINES)
            .collect::<Vec<_>>()
            .join(" \n");
        lines.push(display);
    }
    lines.join("\n\n")
}

fn flatten_items(items: &[ResponseItem]) -> String {
    let mut buf = String::new();
    for item in items {
        match item {
            ResponseItem::Message { role, content, .. } => {
                let text = content
                    .iter()
                    .filter_map(|chunk| match chunk {
                        ContentItem::InputText { text }
                        | ContentItem::OutputText { text } => Some(text.as_str()),
                        ContentItem::InputImage { .. } => Some("<image>"),
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                if text.is_empty() {
                    continue;
                }
                buf.push_str(&format!("{role}: {text}\n"));
            }
            ResponseItem::FunctionCall { name, arguments, .. } => {
                buf.push_str(&format!("tool_call {name}: {arguments}\n"));
            }
            ResponseItem::FunctionCallOutput { output, .. } => {
                buf.push_str(&format!("tool_output: {}\n", output.content));
            }
            ResponseItem::CustomToolCall { name, input, .. } => {
                buf.push_str(&format!("custom_tool {name}: {input}\n"));
            }
            ResponseItem::CustomToolCallOutput { output, .. } => {
                buf.push_str(&format!("custom_tool_output: {}\n", output));
            }
            ResponseItem::Reasoning { summary, .. } => {
                for item in summary {
                    match item {
                        code_protocol::models::ReasoningItemReasoningSummary::SummaryText { text } => {
                            buf.push_str(&format!("reasoning: {text}\n"));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    buf
}

fn chunk_text(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }

    let mut chunks = Vec::new();
    let mut start = 0;
    let len = text.len();
    while start < len {
        let mut end = (start + MAX_TRANSCRIPT_BYTES).min(len);
        if end < len {
            while end > start && !text.is_char_boundary(end) {
                end -= 1;
            }
            if end == start {
                // The next character alone exceeds the byte budget; include it to make progress.
                end = start
                    + text[start..]
                        .chars()
                        .next()
                        .map(|c| c.len_utf8())
                        .unwrap_or(len - start);
            }
        }

        if end <= start {
            break;
        }

        let chunk = text[start..end].to_string();
        chunks.push(chunk);
        start = end;
    }

    chunks
}

fn advance_to_turn_boundary(items: &[ResponseItem], start_idx: usize) -> usize {
    let mut idx = start_idx;
    while idx < items.len() {
        if matches!(&items[idx], ResponseItem::Message { role, .. } if role == "user") {
            break;
        }
        idx += 1;
    }
    idx
}

pub(crate) fn estimate_item_tokens(item: &ResponseItem) -> usize {
    let byte_count = match item {
        ResponseItem::Message { content, .. } => content
            .iter()
            .map(|chunk| match chunk {
                ContentItem::InputText { text } | ContentItem::OutputText { text } => text.len(),
                ContentItem::InputImage { image_url } => image_url.len() / 10,
            })
            .sum(),
        ResponseItem::FunctionCall { name, arguments, .. } => name.len() + arguments.len(),
        ResponseItem::FunctionCallOutput { output, .. } => output.content.len(),
        ResponseItem::CustomToolCall { name, input, .. } => name.len() + input.len(),
        ResponseItem::CustomToolCallOutput { output, .. } => output.len(),
        ResponseItem::Reasoning { summary, content, .. } => {
            summary
                .iter()
                .map(|s| match s {
                    code_protocol::models::ReasoningItemReasoningSummary::SummaryText { text } => text.len(),
                })
                .sum::<usize>()
                + content
                    .as_ref()
                    .map(|segments| {
                        segments
                            .iter()
                            .map(|segment| match segment {
                                code_protocol::models::ReasoningItemContent::ReasoningText { text }
                                | code_protocol::models::ReasoningItemContent::Text { text } => text.len(),
                            })
                            .sum::<usize>()
                    })
                    .unwrap_or(0)
        }
        _ => 0,
    };
    byte_count.div_ceil(BYTES_PER_TOKEN)
}

fn plain_message(role: &str, text: String) -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: role.to_string(),
        content: vec![ContentItem::InputText { text }],
    }
}

fn push_compaction_prompt(prompt: &mut Prompt, compact_prompt: &str) {
    prompt
        .input
        .push(plain_message("developer", compact_prompt.to_string()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_core::codex::compact::CompactionSnippet;
    use code_core::content_items_to_text;

    fn user_message(text: &str) -> ResponseItem {
        plain_message("user", text.to_string())
    }

    fn assistant_message(text: &str) -> ResponseItem {
        plain_message("assistant", text.to_string())
    }

    fn system_message(text: &str) -> ResponseItem {
        plain_message("system", text.to_string())
    }

    #[test]
    fn computes_slice_bounds_midpoint() {
        let conversation = vec![
            system_message("System"),
            user_message("Goal"),
            assistant_message("Step 1"),
            user_message("Step 2"),
            assistant_message("Step 2 done"),
            user_message("Step 3"),
        ];

        let (start, end) = compute_slice_bounds(&conversation).expect("bounds");
        assert_eq!(start, 2);
        assert_eq!(end, 5);
    }

    #[test]
    fn apply_compaction_preserves_goal() {
        let mut conversation = vec![
            system_message("System"),
            user_message("Goal"),
            assistant_message("Old content"),
            user_message("More content"),
            assistant_message("Final"),
        ];

        let summary = make_compaction_summary_message(
            &collect_compaction_snippets(&conversation),
            "Summary",
        );
        apply_compaction(&mut conversation, (2, 5), Some("Prev"), summary).expect("compaction");

        assert_eq!(conversation.len(), 4);
        assert!(matches!(&conversation[1], ResponseItem::Message { role, .. } if role == "user"));
        assert!(matches!(&conversation[2], ResponseItem::Message { .. }));
    }

    #[test]
    fn apply_compaction_inserts_prev_summary() {
        let mut conversation = vec![
            system_message("System"),
            user_message("Goal"),
            assistant_message("Old"),
            user_message("Tail"),
        ];

        let summary = make_compaction_summary_message(
            &collect_compaction_snippets(&conversation),
            "New summary",
        );
        apply_compaction(&mut conversation, (2, 4), Some("Prev summary"), summary).expect("compaction");

        assert_eq!(conversation.len(), 4);
        let prev = &conversation[2];
        if let ResponseItem::Message { content, .. } = prev {
            let joined = content
                .iter()
                .filter_map(|chunk| match chunk {
                    ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                        Some(text.as_str())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(" ");
            assert!(joined.contains("Prev summary"));
        } else {
            panic!("expected message");
        }
    }

    #[test]
    fn ensure_goal_reinserted_when_missing_after_compaction() {
        let goal = user_message("Rewrite parser");
        let mut compacted = vec![assistant_message("Checkpoint summary")];

        ensure_goal_is_present(&mut compacted, goal.clone(), 1);

        let goal_count = compacted
            .iter()
            .filter(|item| message_text(item).as_deref() == Some("Rewrite parser"))
            .count();
        assert_eq!(goal_count, 1);
    }

    #[test]
    fn ensure_goal_not_duplicated_when_already_present() {
        let goal = user_message("Rewrite parser");
        let mut compacted = vec![goal.clone(), assistant_message("Checkpoint summary")];

        ensure_goal_is_present(&mut compacted, goal.clone(), 0);

        let goal_count = compacted
            .iter()
            .filter(|item| message_text(item).as_deref() == Some("Rewrite parser"))
            .count();
        assert_eq!(goal_count, 1);
    }

    #[test]
    fn flatten_items_preserves_full_messages() {
        let large = "a".repeat(MAX_TRANSCRIPT_BYTES * 2);
        let items = vec![assistant_message(&large)];

        let flattened = flatten_items(&items);
        assert!(flattened.contains(&large[..32]));
        assert!(flattened.contains(&large[large.len() - 32..]));
        assert!(flattened.len() > MAX_TRANSCRIPT_BYTES);
    }

    #[test]
    fn chunk_text_consumes_entire_string() {
        let text = "a".repeat(MAX_TRANSCRIPT_BYTES * 2 + 123);
        let chunks = chunk_text(&text);
        let reconstructed: String = chunks.concat();
        assert_eq!(reconstructed, text);
        assert!(chunks.iter().all(|chunk| chunk.len() <= MAX_TRANSCRIPT_BYTES));
    }

    #[test]
    fn chunk_text_respects_utf8_boundaries() {
        let text = "🙂".repeat((MAX_TRANSCRIPT_BYTES / 4) + 10);
        let chunks = chunk_text(&text);
        assert!(!chunks.is_empty());
        for chunk in &chunks {
            assert!(chunk.is_char_boundary(chunk.len()));
            assert!(chunk.len() <= MAX_TRANSCRIPT_BYTES);
        }
        assert_eq!(chunks.concat(), text);
    }

    #[test]
    fn compaction_summary_message_includes_snippets() {
        let snippets = vec![
            CompactionSnippet {
                role: "user".to_string(),
                text: "Investigate failing tests".to_string(),
            },
            CompactionSnippet {
                role: "assistant".to_string(),
                text: "Analyzed logs and proposed fix".to_string(),
            },
        ];
        let message = make_compaction_summary_message(&snippets, "Tests still red; patch script");
        let ResponseItem::Message { content, .. } = message else {
            panic!("expected message response item");
        };
        let rendered = content_items_to_text(&content).expect("text content");
        assert!(rendered.contains("(user) Investigate failing tests"));
        assert!(rendered.contains("Key takeaways"));
        assert!(rendered.contains("Tests still red"));
    }

    #[test]
    fn push_compaction_prompt_inserts_override_text() {
        let mut prompt = Prompt::default();
        push_compaction_prompt(&mut prompt, "Custom override text");

        assert_eq!(prompt.input.len(), 1);
        match &prompt.input[0] {
            ResponseItem::Message { role, content, .. } => {
                assert_eq!(role, "developer");
                let body = content
                    .iter()
                    .filter_map(|chunk| match chunk {
                        ContentItem::InputText { text } => Some(text.as_str()),
                        ContentItem::OutputText { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                assert_eq!(body, "Custom override text");
            }
            other => panic!("expected developer message, got {other:?}"),
        }
    }
}
