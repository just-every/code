//! Message composition helpers and types for the chat widget.

use codex_core::protocol::InputItem;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct UserMessage {
    /// What to show in the chat history (keeps placeholders like "[image: name.png]")
    pub display_text: String,
    /// Items to send to the core/model in the correct order, with inline
    /// markers preceding images so the LLM knows placement.
    pub ordered_items: Vec<InputItem>,
}

impl From<String> for UserMessage {
    fn from(text: String) -> Self {
        let mut ordered = Vec::new();
        if !text.trim().is_empty() {
            ordered.push(InputItem::Text { text: text.clone() });
        }
        Self {
            display_text: text,
            ordered_items: ordered,
        }
    }
}

pub fn create_initial_user_message(text: String, image_paths: Vec<PathBuf>) -> Option<UserMessage> {
    if text.is_empty() && image_paths.is_empty() {
        None
    } else {
        let mut ordered: Vec<InputItem> = Vec::new();
        if !text.trim().is_empty() {
            ordered.push(InputItem::Text { text: text.clone() });
        }
        for path in image_paths {
            let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("image");
            ordered.push(InputItem::Text {
                text: format!("[image: {}]", filename),
            });
            ordered.push(InputItem::LocalImage { path });
        }
        Some(UserMessage {
            display_text: text,
            ordered_items: ordered,
        })
    }
}
