use std::collections::HashMap;
use std::sync::Arc;

use codex_core::CodexConversation;
use codex_core::config::Config;
use tokio::sync::Mutex;
use uuid::Uuid;

/// In-memory session entry tracking an active Codex conversation and its config.
#[derive(Clone)]
pub struct SessionEntry {
    pub conversation: Arc<CodexConversation>,
    pub config: Arc<Mutex<Config>>,
}

impl SessionEntry {
    pub fn new(conversation: Arc<CodexConversation>, config: Config) -> Self {
        Self {
            conversation,
            config: Arc::new(Mutex::new(config)),
        }
    }
}

pub type SessionMap = Arc<Mutex<HashMap<Uuid, SessionEntry>>>;
