use std::sync::Arc;

use async_trait::async_trait;

use crate::codex::TurnContext;
use crate::codex::compact;
use crate::codex::compact_remote;
use crate::protocol::InputItem;
use crate::state::TaskKind;

use super::SessionTask;
use super::SessionTaskContext;

#[derive(Clone, Copy, Default)]
pub(crate) struct CompactTask;

#[async_trait]
impl SessionTask for CompactTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Compact
    }

    async fn run(
        self: Arc<Self>,
        session: Arc<SessionTaskContext>,
        ctx: Arc<TurnContext>,
        sub_id: String,
        input: Vec<InputItem>,
    ) -> Option<String> {
        let session_arc = session.clone_session();
        if compact::should_use_remote_compact_task(&session_arc).await {
            compact_remote::run_remote_compact_task(session_arc, ctx, sub_id, input).await;
        } else {
            compact::run_compact_task(session_arc, ctx, sub_id, input).await;
        }
        None
    }
}
