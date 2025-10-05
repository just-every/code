use std::collections::VecDeque;

use codex_protocol::models::{ContentItem, ResponseItem};

/// Maintains the Auto Drive conversation transcript between coordinator turns.
///
/// `converted` mirrors what we previously derived from UI history and is used
/// when re-seeding the coordinator conversation. `raw` captures the exact
/// ResponseItems returned by the Auto Drive model so we can retain full
/// reasoning output without depending on UI rendering.
pub(crate) struct AutoDriveHistory {
    converted: Vec<ResponseItem>,
    raw: Vec<ResponseItem>,
    pending_duplicates: VecDeque<NormalizedMessage>,
}

impl AutoDriveHistory {
    pub(crate) fn new() -> Self {
        Self {
            converted: Vec::new(),
            raw: Vec::new(),
            pending_duplicates: VecDeque::new(),
        }
    }

    /// Replace the stored converted transcript. Returns any new tail items that
    /// were not present previously, preserving insertion order.
    pub(crate) fn replace_converted(&mut self, items: Vec<ResponseItem>) -> Vec<ResponseItem> {
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

    pub(crate) fn append_raw(&mut self, items: &[ResponseItem]) {
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

    pub(crate) fn append_converted_tail(&mut self, items: &[ResponseItem]) {
        if items.is_empty() {
            return;
        }
        self.raw.extend(items.iter().cloned());
    }

    pub(crate) fn raw_snapshot(&self) -> Vec<ResponseItem> {
        self.raw.clone()
    }

    pub(crate) fn clear(&mut self) {
        self.converted.clear();
        self.raw.clear();
        self.pending_duplicates.clear();
    }

    pub(crate) fn converted_is_empty(&self) -> bool {
        self.converted.is_empty()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn raw_items(&self) -> &[ResponseItem] {
        &self.raw
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_protocol::models::{ContentItem, ResponseItem};

    fn text_message(role: &str, text: &str) -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: role.to_string(),
            content: vec![ContentItem::InputText {
                text: text.to_string(),
            }],
        }
    }

    #[test]
    fn converted_tail_survives_after_pending_skip() {
        let mut history = AutoDriveHistory::new();

        // Initial export from UI history.
        let first = vec![text_message("user", "hi")];
        let tail = history.replace_converted(first.clone());
        assert_eq!(tail.len(), 1);
        history.append_converted_tail(&tail);
        assert_eq!(history.raw_snapshot(), first);
        assert!(history.pending_duplicates.is_empty());

        // Coordinator delivers transcript; these entries should be skipped on the next rebuild.
        let transcript = vec![text_message("assistant", "ack")];
        history.append_raw(&transcript);
        assert_eq!(history.pending_duplicates.len(), 1);

        // UI rebuild includes the coordinator output; nothing new should be appended and skip resets.
        let second = vec![text_message("user", "hi"), text_message("assistant", "ack")];
        let tail = history.replace_converted(second.clone());
        assert!(tail.is_empty());
        assert!(history.pending_duplicates.is_empty());

        // A new user turn appears; ensure it is not dropped by the skip logic.
        let third = vec![
            text_message("user", "hi"),
            text_message("assistant", "ack"),
            text_message("user", "followup"),
        ];
        let tail = history.replace_converted(third.clone());
        assert_eq!(tail.len(), 1);
        history.append_converted_tail(&tail);

        let snapshot = history.raw_snapshot();
        assert_eq!(snapshot, third);
        assert!(history.pending_duplicates.is_empty());
    }

    #[test]
    fn skips_only_matching_duplicates() {
        let mut history = AutoDriveHistory::new();

        // Coordinator appends an auto-generated assistant message.
        let transcript = vec![ResponseItem::Message {
            id: Some("msg-123".to_string()),
            role: "assistant".to_string(),
            content: vec![ContentItem::OutputText {
                text: "auto step".to_string(),
            }],
        }];
        history.append_raw(&transcript);
        assert_eq!(history.pending_duplicates.len(), 1);

        // UI rebuild produces an unrelated tail entry; it should not be dropped.
        let conversation = vec![
            text_message("user", "hi"),
            text_message("assistant", "auto step"),
            text_message("assistant", "manual edit"),
        ];
        let tail = history.replace_converted(conversation.clone());
        assert_eq!(tail.len(), 3);
        assert!(history.pending_duplicates.is_empty());
    }

    #[test]
    fn dedupe_ignores_ids() {
        let mut history = AutoDriveHistory::new();

        let transcript = vec![ResponseItem::Message {
            id: Some("msg-xyz".to_string()),
            role: "assistant".to_string(),
            content: vec![ContentItem::OutputText {
                text: "coordinator turn".to_string(),
            }],
        }];
        history.append_raw(&transcript);
        assert_eq!(history.pending_duplicates.len(), 1);

        let rebuilt = vec![
            text_message("user", "hi"),
            text_message("assistant", "coordinator turn"),
        ];
        let tail = history.replace_converted(rebuilt);
        assert!(tail.is_empty(), "turn should be skipped despite differing ids");
        assert!(history.pending_duplicates.is_empty());
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
