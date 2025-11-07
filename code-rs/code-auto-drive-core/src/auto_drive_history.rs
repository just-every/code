use std::collections::VecDeque;

use code_core::protocol::TokenUsage;
use code_protocol::models::{ContentItem, ResponseItem};

use crate::session_metrics::SessionMetrics;

/// Token estimation: 4 bytes per token (same as core/truncate.rs)
const BYTES_PER_TOKEN: usize = 4;

/// Maintains the Auto Drive conversation transcript between coordinator turns.
///
/// `converted` mirrors what we previously derived from UI history and is used
/// when re-seeding the coordinator conversation. `raw` captures the exact
/// ResponseItems returned by the Auto Drive model so we can retain full
/// reasoning output without depending on UI rendering.
pub struct AutoDriveHistory {
    converted: Vec<ResponseItem>,
    raw: Vec<ResponseItem>,
    pending_duplicates: VecDeque<NormalizedMessage>,
    /// Summary from the previous compaction, if any
    prev_compact_summary: Option<String>,
    session_metrics: SessionMetrics,
}

impl AutoDriveHistory {
    pub fn new() -> Self {
        Self {
            converted: Vec::new(),
            raw: Vec::new(),
            pending_duplicates: VecDeque::new(),
            prev_compact_summary: None,
            session_metrics: SessionMetrics::default(),
        }
    }

    /// Replace the stored converted transcript. Returns any new tail items that
    /// were not present previously, preserving insertion order.
    pub fn replace_converted(&mut self, items: Vec<ResponseItem>) -> Vec<ResponseItem> {
        let prev_len = self.converted.len();
        self.converted = items;
        let tail: Vec<_> = if self.converted.len() <= prev_len {
            Vec::new()
        } else {
            self.converted
                .iter()
                .skip(prev_len)
                .cloned()
                .collect()
        };

        if tail.is_empty() {
            return tail;
        }

        if self.should_skip_entire_tail(&tail) {
            self
                .session_metrics
                .record_duplicate_items(tail.len().saturating_sub(1));
            self.session_metrics.record_replay();
            self.pending_duplicates.clear();
            return Vec::new();
        }

        if self.pending_duplicates.is_empty() {
            return tail;
        }

        let mut filtered = Vec::with_capacity(tail.len());
        let queue = &mut self.pending_duplicates;
        for item in tail.into_iter() {
            let matched = normalize_message(&item)
                .and_then(|message| queue.front().map(|expected| (message, expected)))
                .map(|(message, expected)| message == *expected)
                .unwrap_or(false);

            if matched {
                self.session_metrics.record_duplicate_items(1);
                queue.pop_front();
                continue;
            }

            if queue.front().is_some() {
                queue.clear();
            }

            filtered.push(item);
        }

        filtered
    }

    fn should_skip_entire_tail(&self, tail: &[ResponseItem]) -> bool {
        if self.pending_duplicates.is_empty() {
            return false;
        }

        if tail.len() != self.pending_duplicates.len().saturating_add(1) {
            return false;
        }

        let first_is_user = matches!(tail.first(), Some(ResponseItem::Message { role, .. }) if role == "user");
        if !first_is_user {
            return false;
        }

        tail.iter()
            .skip(1)
            .zip(self.pending_duplicates.iter())
            .all(|(item, expected)| {
                let Some(message) = normalize_message(item) else {
                    return false;
                };
                if message.role != expected.role {
                    return false;
                }

                let item_segments: Vec<&str> = message
                    .content
                    .iter()
                    .filter_map(content_text)
                    .collect();
                let expected_segments: Vec<&str> = expected
                    .content
                    .iter()
                    .filter_map(content_text)
                    .collect();

                item_segments == expected_segments
            })
    }

    pub fn append_raw(&mut self, items: &[ResponseItem]) {
        if items.is_empty() {
            return;
        }
        self.raw.extend(items.iter().cloned());
        for item in items.iter() {
            if let Some(message) = normalize_message(item) {
                self.pending_duplicates.push_back(message);
            }
        }
    }

    pub fn append_converted_tail(&mut self, items: &[ResponseItem]) {
        if items.is_empty() {
            return;
        }
        self.raw.extend(items.iter().cloned());
    }

    pub fn raw_snapshot(&self) -> Vec<ResponseItem> {
        self.raw.clone()
    }

    pub fn replace_all(&mut self, items: Vec<ResponseItem>) {
        self.converted = items.clone();
        self.raw = items;
        self.pending_duplicates.clear();
    }

    pub fn clear(&mut self) {
        self.converted.clear();
        self.raw.clear();
        self.pending_duplicates.clear();
        self.prev_compact_summary = None;
        self.session_metrics.reset();
    }

    pub fn converted_is_empty(&self) -> bool {
        self.converted.is_empty()
    }

    /// Replace the tracked metrics with the latest values reported by the coordinator.
    pub fn apply_token_metrics(
        &mut self,
        total: TokenUsage,
        last: TokenUsage,
        turn_count: u32,
        duplicate_items: u32,
        replay_updates: u32,
    ) {
        self.session_metrics.sync_absolute(total, last, turn_count);
        self.session_metrics.set_duplicate_items(duplicate_items);
        self.session_metrics.set_replay_updates(replay_updates);
    }

    /// Returns the cumulative token usage across all coordinator turns.
    pub fn total_tokens(&self) -> &TokenUsage {
        self.session_metrics.running_total()
    }

    /// Returns the token usage from the most recent coordinator turn.
    pub fn last_turn_tokens(&self) -> &TokenUsage {
        self.session_metrics.last_turn()
    }

    /// Returns the number of turns recorded so far.
    pub fn recorded_turns(&self) -> u32 {
        self.session_metrics.turn_count()
    }

    pub fn duplicate_items(&self) -> u32 {
        self.session_metrics.duplicate_items()
    }

    pub fn replay_updates(&self) -> u32 {
        self.session_metrics.replay_updates()
    }

    /// Returns the estimated prompt tokens for the next turn.
    pub fn estimated_next_prompt_tokens(&self) -> u64 {
        self.session_metrics.estimated_next_prompt_tokens()
    }

    /// Perform compaction by selecting a slice after the goal message (first user message),
    /// finding the 50% token midpoint, advancing to the end of a turn boundary, and replacing
    /// the slice with a compact summary item.
    ///
    /// Returns `Ok(true)` if compaction was performed, `Ok(false)` if skipped, or an error.
    pub fn compact_slice(&mut self, summarizer: impl FnOnce(&[ResponseItem]) -> String) -> Result<bool, String> {
        // Find the goal message (first user message)
        let goal_idx = self.converted.iter().position(|item| {
            matches!(item, ResponseItem::Message { role, .. } if role == "user")
        });

        let Some(goal_idx) = goal_idx else {
            // No goal message found; nothing to compact
            return Ok(false);
        };

        // We need at least a few items after the goal to make compaction worthwhile
        if self.converted.len() <= goal_idx + 3 {
            return Ok(false);
        }

        // Calculate total tokens after the goal message
        let items_after_goal = &self.converted[goal_idx + 1..];
        let total_tokens = estimate_tokens(items_after_goal);

        // We need a reasonable amount of content to compact
        if total_tokens < 1000 {
            return Ok(false);
        }

        // Find the 50% midpoint
        let target_tokens = total_tokens / 2;
        let mut accumulated_tokens = 0;
        let mut midpoint_idx = goal_idx + 1;

        for (i, item) in items_after_goal.iter().enumerate() {
            accumulated_tokens += estimate_item_tokens(item);
            if accumulated_tokens >= target_tokens {
                midpoint_idx = goal_idx + 1 + i;
                break;
            }
        }

        // Advance to the end of the turn boundary
        let slice_end = advance_to_turn_boundary(&self.converted, midpoint_idx);

        // The slice to compact is from (goal_idx + 1) to slice_end
        if slice_end <= goal_idx + 1 {
            return Ok(false);
        }

        let slice_to_compact = &self.converted[goal_idx + 1..slice_end];

        // Generate summary
        let summary_text = summarizer(slice_to_compact);

        // Build the compact summary item
        let compact_item = ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: format!(
                    "<compact_summary>\n{}\n</compact_summary>",
                    summary_text
                ),
            }],
        };

        // Replace the slice with the compact item
        let mut new_converted = Vec::new();
        new_converted.extend_from_slice(&self.converted[..=goal_idx]);
        new_converted.push(compact_item);
        new_converted.extend_from_slice(&self.converted[slice_end..]);

        self.converted = new_converted;
        self.prev_compact_summary = Some(summary_text);

        Ok(true)
    }

}


#[derive(Clone, Debug, PartialEq, Eq)]
struct NormalizedMessage {
    role: String,
    content: Vec<NormalizedContent>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum NormalizedContent {
    InputText(String),
    OutputText(String),
    InputImage(String),
}

fn normalize_message(item: &ResponseItem) -> Option<NormalizedMessage> {
    if let ResponseItem::Message { role, content, .. } = item {
        let normalized = content
            .iter()
            .map(|chunk| match chunk {
                ContentItem::InputText { text } => NormalizedContent::InputText(text.clone()),
                ContentItem::OutputText { text } => NormalizedContent::OutputText(text.clone()),
                ContentItem::InputImage { image_url } => {
                    NormalizedContent::InputImage(image_url.clone())
                }
            })
            .collect();
        Some(NormalizedMessage {
            role: role.clone(),
            content: normalized,
        })
    } else {
        None
    }
}

fn content_text(content: &NormalizedContent) -> Option<&str> {
    match content {
        NormalizedContent::InputText(text)
        | NormalizedContent::OutputText(text)
        | NormalizedContent::InputImage(text) => Some(text.as_str()),
    }
}

/// Estimate the total tokens for a slice of ResponseItems.
fn estimate_tokens(items: &[ResponseItem]) -> usize {
    items.iter().map(estimate_item_tokens).sum()
}

/// Estimate tokens for a single ResponseItem.
/// Uses byte count divided by BYTES_PER_TOKEN (4) as fallback, same as core/truncate.rs.
fn estimate_item_tokens(item: &ResponseItem) -> usize {
    let byte_count = match item {
        ResponseItem::Message { content, .. } => {
            content.iter().map(|c| match c {
                ContentItem::InputText { text } | ContentItem::OutputText { text } => text.len(),
                ContentItem::InputImage { image_url } => image_url.len() / 10, // images are less token-heavy
            }).sum()
        }
        ResponseItem::FunctionCall { name, arguments, .. } => name.len() + arguments.len(),
        ResponseItem::FunctionCallOutput { output, .. } => output.content.len(),
        ResponseItem::CustomToolCall { name, input, .. } => name.len() + input.len(),
        ResponseItem::CustomToolCallOutput { output, .. } => output.len(),
        ResponseItem::Reasoning { summary, content, .. } => {
            summary.iter().map(|s| match s {
                code_protocol::models::ReasoningItemReasoningSummary::SummaryText { text } => text.len(),
            }).sum::<usize>()
                + content.as_ref().map(|c| c.iter().map(|item| match item {
                    code_protocol::models::ReasoningItemContent::ReasoningText { text } |
                    code_protocol::models::ReasoningItemContent::Text { text } => text.len(),
                }).sum()).unwrap_or(0)
        }
        // Catch-all for other types: Other, LocalShellCall, WebSearchCall, etc.
        _ => 0,
    };
    byte_count.div_ceil(BYTES_PER_TOKEN)
}

/// Advance from the given index to the end of the current turn boundary.
/// A turn boundary ends when we see a user message (the start of the next turn).
fn advance_to_turn_boundary(items: &[ResponseItem], start_idx: usize) -> usize {
    let mut idx = start_idx;

    // Scan forward to find the next user message
    while idx < items.len() {
        if matches!(&items[idx], ResponseItem::Message { role, .. } if role == "user") {
            // Found the start of the next turn; stop here
            return idx;
        }
        idx += 1;
    }

    // Reached the end of the history
    idx
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_user_message(text: &str) -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: text.to_string(),
            }],
        }
    }

    fn make_assistant_message(text: &str) -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: "assistant".to_string(),
            content: vec![ContentItem::OutputText {
                text: text.to_string(),
            }],
        }
    }

    fn make_usage(input: u64, output: u64) -> TokenUsage {
        TokenUsage {
            input_tokens: input,
            cached_input_tokens: 0,
            output_tokens: output,
            reasoning_output_tokens: 0,
            total_tokens: input + output,
        }
    }

    #[test]
    fn test_compact_slice_no_goal_message() {
        let mut history = AutoDriveHistory::new();
        history.converted = vec![
            make_assistant_message("Hello"),
        ];

        let result = history.compact_slice(|_| "SUMMARY".to_string());
        assert!(result.is_ok());
        assert!(!result.unwrap()); // Should skip compaction
    }

    #[test]
    fn test_compact_slice_insufficient_items() {
        let mut history = AutoDriveHistory::new();
        history.converted = vec![
            make_user_message("Goal"),
            make_assistant_message("Response 1"),
        ];

        let result = history.compact_slice(|_| "SUMMARY".to_string());
        assert!(result.is_ok());
        assert!(!result.unwrap()); // Should skip compaction
    }

    #[test]
    fn test_compact_slice_basic() {
        let mut history = AutoDriveHistory::new();
        // Create a history with a goal and enough content to compact
        let large_text = "x".repeat(4000); // ~1000 tokens
        history.converted = vec![
            make_user_message("Goal message"),
            make_assistant_message(&large_text),
            make_user_message("Turn 2"),
            make_assistant_message(&large_text),
            make_user_message("Turn 3"),
        ];

        let result = history.compact_slice(|items| {
            format!("Compacted {} items", items.len())
        });

        assert!(result.is_ok());
        assert!(result.unwrap()); // Compaction should occur

        // Verify structure: goal + compact summary inserted ahead of remaining turns
        assert_eq!(history.converted.len(), 5);
        assert!(matches!(&history.converted[0], ResponseItem::Message { role, .. } if role == "user"));
        assert!(matches!(&history.converted[1], ResponseItem::Message { role, content, .. }
            if role == "user" && content.iter().any(|c| matches!(c, ContentItem::InputText { text } if text.contains("<compact_summary>")))));
        assert!(matches!(&history.converted[2], ResponseItem::Message { role, content, .. }
            if role == "user" && content.iter().any(|c| matches!(c, ContentItem::InputText { text } if text.contains("Turn 2")))));
    }

    #[test]
    fn test_apply_token_metrics_updates_totals() {
        let mut history = AutoDriveHistory::new();
        history.apply_token_metrics(make_usage(10, 5), make_usage(4, 2), 3, 0, 0);

        assert_eq!(history.total_tokens().input_tokens, 10);
        assert_eq!(history.last_turn_tokens().input_tokens, 4);
        assert_eq!(history.recorded_turns(), 3);
        assert_eq!(history.estimated_next_prompt_tokens(), 4);
    }

    #[test]
    fn replace_converted_records_duplicate_items() {
        let mut history = AutoDriveHistory::new();
        let goal = make_user_message("Goal");
        let first_reply = make_assistant_message("First reply");
        let duplicate_reply = make_assistant_message("Duplicate reply");

        history.replace_converted(vec![goal.clone(), first_reply.clone()]);
        history.append_raw(&[duplicate_reply.clone()]);

        // Introducing a new tail that begins with a user message followed by the duplicate.
        let new_history = vec![
            goal,
            first_reply,
            make_user_message("Follow-up"),
            duplicate_reply,
        ];
        let tail = history.replace_converted(new_history);
        assert!(tail.is_empty());
        assert_eq!(history.duplicate_items(), 1);
        assert_eq!(history.replay_updates(), 1);
    }

    #[test]
    fn test_advance_to_turn_boundary() {
        let items = vec![
            make_user_message("Goal"),
            make_assistant_message("Response 1"),
            make_assistant_message("Response 2"),
            make_user_message("Turn 2"),
            make_assistant_message("Response 3"),
        ];

        // Starting from index 1 should advance to index 3 (next user message)
        let end = advance_to_turn_boundary(&items, 1);
        assert_eq!(end, 3);

        // Starting from index 4 should advance to the end (no more user messages)
        let end = advance_to_turn_boundary(&items, 4);
        assert_eq!(end, 5);
    }

    #[test]
    fn test_estimate_tokens() {
        let items = vec![
            make_user_message("Hello world"), // ~11 chars / 4 = ~2-3 tokens
            make_assistant_message("How are you?"), // ~12 chars / 4 = ~3 tokens
        ];

        let tokens = estimate_tokens(&items);
        assert!(tokens > 0);
        assert!(tokens < 100); // Reasonable estimate
    }
}
