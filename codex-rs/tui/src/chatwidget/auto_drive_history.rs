use codex_protocol::models::ResponseItem;

/// Maintains the Auto Drive conversation transcript between coordinator turns.
///
/// `converted` mirrors what we previously derived from UI history and is used
/// when re-seeding the coordinator conversation. `raw` captures the exact
/// ResponseItems returned by the Auto Drive model so we can retain full
/// reasoning output without depending on UI rendering.
pub(crate) struct AutoDriveHistory {
    converted: Vec<ResponseItem>,
    raw: Vec<ResponseItem>,
    pending_skip: usize,
}

impl AutoDriveHistory {
    pub(crate) fn new() -> Self {
        Self {
            converted: Vec::new(),
            raw: Vec::new(),
            pending_skip: 0,
        }
    }

    /// Replace the stored converted transcript. Returns any new tail items that
    /// were not present previously, preserving insertion order.
    pub(crate) fn replace_converted(&mut self, items: Vec<ResponseItem>) -> Vec<ResponseItem> {
        let prev_len = self.converted.len();
        self.converted = items;
        let mut tail: Vec<_> = if self.converted.len() <= prev_len {
            Vec::new()
        } else {
            self.converted
                .iter()
                .skip(prev_len)
                .cloned()
                .collect()
        };

        if !tail.is_empty() && self.pending_skip > 0 {
            if self.pending_skip >= tail.len() {
                self.pending_skip -= tail.len();
                tail.clear();
            } else {
                tail.drain(0..self.pending_skip);
                self.pending_skip = 0;
            }
        }

        tail
    }

    pub(crate) fn append_raw(&mut self, items: &[ResponseItem]) {
        if items.is_empty() {
            return;
        }
        self.raw.extend(items.iter().cloned());
        self.pending_skip = self.pending_skip.saturating_add(items.len());
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
        self.pending_skip = 0;
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
        assert_eq!(history.pending_skip, 0);

        // Coordinator delivers transcript; these entries should be skipped on the next rebuild.
        let transcript = vec![text_message("assistant", "ack")];
        history.append_raw(&transcript);
        assert_eq!(history.pending_skip, transcript.len());

        // UI rebuild includes the coordinator output; nothing new should be appended and skip resets.
        let second = vec![text_message("user", "hi"), text_message("assistant", "ack")];
        let tail = history.replace_converted(second.clone());
        assert!(tail.is_empty());
        assert_eq!(history.pending_skip, 0);

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
        assert_eq!(history.pending_skip, 0);
    }
}
