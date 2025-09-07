use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use tracing::debug;

/// Transcript of conversation history
#[derive(Debug, Clone, Default)]
pub(crate) struct ConversationHistory {
    /// The oldest items are at the beginning of the vector.
    items: Vec<ResponseItem>,
}

impl ConversationHistory {
    pub(crate) fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Returns a clone of the contents in the transcript.
    pub(crate) fn contents(&self) -> Vec<ResponseItem> {
        self.items.clone()
    }

    /// `items` is ordered from oldest to newest.
    pub(crate) fn record_items<I>(&mut self, items: I)
    where
        I: IntoIterator,
        I::Item: std::ops::Deref<Target = ResponseItem>,
    {
        for item in items {
            if !is_api_message(&item) {
                continue;
            }

            // De-duplicate assistant messages more robustly by leveraging IDs
            // when available and falling back to adjacency/prefix merging.
            match &*item {
                ResponseItem::Message {
                    id: new_id,
                    role: new_role,
                    content: new_content,
                } if new_role == "assistant" => {
                    // 1) If this assistant message has an ID, try to find an existing
                    //    assistant entry with the same ID anywhere in history and
                    //    replace its text with the new content.
                    if let Some(id) = new_id.as_ref() {
                        if let Some(idx) = find_assistant_index_by_id(&self.items, id) {
                            if let ResponseItem::Message {
                                id: existing_id,
                                content,
                                ..
                            } = &mut self.items[idx]
                            {
                                // Ensure the existing entry carries the id
                                if existing_id.is_none() {
                                    *existing_id = Some(id.clone());
                                }
                                replace_or_append_text(content, new_content);
                                continue;
                            }
                        }
                    }

                    // 2) Otherwise, attempt to merge with the immediately previous
                    //    assistant message (streamed deltas case).
                    if let Some(ResponseItem::Message {
                        id: last_id,
                        role: last_role,
                        content: last_content,
                    }) = self.items.last_mut()
                    {
                        if last_role == "assistant" {
                            // If new item has an ID, propagate it to the existing entry
                            if last_id.is_none() {
                                if let Some(id) = new_id.clone() {
                                    *last_id = Some(id);
                                }
                            }
                            replace_or_append_text(last_content, new_content);
                            continue;
                        }
                    }

                    // 3) No merge opportunity – push as a new assistant message.
                    self.items.push(item.clone());
                }
                ResponseItem::FunctionCallOutput { call_id, .. } => {
                    // Check if we already have an output for this call_id to prevent duplicates
                    let already_exists = self.items.iter().any(|existing| {
                        matches!(existing, ResponseItem::FunctionCallOutput { call_id: existing_id, .. } if existing_id == call_id)
                    });

                    if already_exists {
                        debug!(
                            "Skipping duplicate FunctionCallOutput for call_id: {} (already in history)",
                            call_id
                        );
                    } else {
                        debug!(
                            "Recording FunctionCallOutput to history for call_id: {}",
                            call_id
                        );
                        self.items.push(item.clone());
                    }
                }
                ResponseItem::FunctionCall { call_id, .. } => {
                    // Check if we already have this function call to prevent duplicates during retries
                    let already_exists = self.items.iter().any(|existing| {
                        matches!(existing, ResponseItem::FunctionCall { call_id: existing_id, .. } if existing_id == call_id)
                    });

                    if already_exists {
                        debug!(
                            "Skipping duplicate FunctionCall for call_id: {} (already in history)",
                            call_id
                        );
                    } else {
                        self.items.push(item.clone());
                    }
                }
                _ => {
                    self.items.push(item.clone());
                }
            }
        }
    }

    pub(crate) fn keep_last_messages(&mut self, n: usize) {
        if n == 0 {
            self.items.clear();
            return;
        }

        // Collect the last N message items (assistant/user), newest to oldest.
        let mut kept: Vec<ResponseItem> = Vec::with_capacity(n);
        for item in self.items.iter().rev() {
            if let ResponseItem::Message { role, content, .. } = item {
                kept.push(ResponseItem::Message {
                    // we need to remove the id or the model will complain that messages are sent without
                    // their reasonings
                    id: None,
                    role: role.clone(),
                    content: content.clone(),
                });
                if kept.len() == n {
                    break;
                }
            }
        }

        // Preserve chronological order (oldest to newest) within the kept slice.
        kept.reverse();
        self.items = kept;
    }
}

/// Anything that is not a system message or "reasoning" message is considered
/// an API message.
fn is_api_message(message: &ResponseItem) -> bool {
    match message {
        ResponseItem::Message { role, .. } => role.as_str() != "system",
        ResponseItem::FunctionCallOutput { .. }
        | ResponseItem::FunctionCall { .. }
        | ResponseItem::CustomToolCall { .. }
        | ResponseItem::CustomToolCallOutput { .. }
        | ResponseItem::LocalShellCall { .. }
        | ResponseItem::Reasoning { .. } => true,
        ResponseItem::WebSearchCall { .. } | ResponseItem::Other => false,
    }
}

/// Helper to append the textual content from `src` into `dst` in place.
fn append_text_content(dst: &mut Vec<ContentItem>, src: &Vec<ContentItem>) {
    for c in src {
        if let ContentItem::OutputText { text } = c {
            append_text_delta(dst, text);
        }
    }
}

/// Append a single text delta to the last OutputText item in `content`, or
/// push a new OutputText item if none exists.
fn append_text_delta(content: &mut Vec<ContentItem>, delta: &str) {
    if let Some(ContentItem::OutputText { text }) = content
        .iter_mut()
        .rev()
        .find(|c| matches!(c, ContentItem::OutputText { .. }))
    {
        text.push_str(delta);
    } else {
        content.push(ContentItem::OutputText {
            text: delta.to_string(),
        });
    }
}

/// Concatenate all OutputText segments into a single string.
fn collect_text_content(content: &Vec<ContentItem>) -> String {
    let mut out = String::new();
    for c in content {
        if let ContentItem::OutputText { text } = c {
            out.push_str(text);
        }
    }
    out
}

/// Replace the text content with a single OutputText item containing `text`.
fn replace_text_content(content: &mut Vec<ContentItem>, text: &str) {
    content.clear();
    content.push(ContentItem::OutputText {
        text: text.to_string(),
    });
}

/// Merge strategy: if the new text starts with the old, replace; otherwise append deltas.
fn replace_or_append_text(existing: &mut Vec<ContentItem>, new_content: &Vec<ContentItem>) {
    let prev = collect_text_content(existing);
    let new_full = collect_text_content(new_content);
    if !prev.is_empty() && new_full.starts_with(&prev) {
        replace_text_content(existing, &new_full);
    } else {
        append_text_content(existing, new_content);
    }
}

fn find_assistant_index_by_id(items: &Vec<ResponseItem>, id: &str) -> Option<usize> {
    // Search from the end (most recent first) for better performance in practice.
    for (idx, it) in items.iter().enumerate().rev() {
        if let ResponseItem::Message {
            id: Some(existing),
            role,
            ..
        } = it
        {
            if role == "assistant" && existing == id {
                return Some(idx);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_protocol::models::ContentItem;

    fn assistant_msg(text: &str) -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: "assistant".to_string(),
            content: vec![ContentItem::OutputText {
                text: text.to_string(),
            }],
        }
    }

    fn user_msg(text: &str) -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::OutputText {
                text: text.to_string(),
            }],
        }
    }

    #[test]
    fn filters_non_api_messages() {
        let mut h = ConversationHistory::default();
        // System message is not an API message; Other is ignored.
        let system = ResponseItem::Message {
            id: None,
            role: "system".to_string(),
            content: vec![ContentItem::OutputText {
                text: "ignored".to_string(),
            }],
        };
        h.record_items([&system, &ResponseItem::Other]);

        // User and assistant should be retained.
        let u = user_msg("hi");
        let a = assistant_msg("hello");
        h.record_items([&u, &a]);

        let items = h.contents();
        assert_eq!(
            items,
            vec![
                ResponseItem::Message {
                    id: None,
                    role: "user".to_string(),
                    content: vec![ContentItem::OutputText {
                        text: "hi".to_string()
                    }]
                },
                ResponseItem::Message {
                    id: None,
                    role: "assistant".to_string(),
                    content: vec![ContentItem::OutputText {
                        text: "hello".to_string()
                    }]
                }
            ]
        );
    }
}
