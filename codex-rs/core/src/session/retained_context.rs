//! Orders retained inputs and records host-verified facts at the context checkpoint boundary.

use crate::context::GuardianContextMode;
use codex_history::RetainedContextEvent;
use codex_history::RolloutItem;

use super::Session;
use super::thread_settings;

impl Session {
    /// Legacy mode neither reserves a sequence nor takes the session-state lock.
    pub(crate) async fn reserve_user_input_order(&self) -> Option<u64> {
        if self.guardian_context_mode == GuardianContextMode::Legacy {
            return None;
        }
        Some(self.state.lock().await.history.reserve_input_order())
    }

    pub(crate) async fn record_retained_context(&self, mut event: RetainedContextEvent) {
        event.bound();
        // Share the checkpoint persistence lock so a fact cannot land on the wrong side
        // of the checkpoint/suffix boundary. Ephemeral threads use the same live state.
        let _guard = thread_settings::acquire_persistence_lock(self).await;
        if self
            .state
            .lock()
            .await
            .history
            .record_retained_context(&event)
        {
            self.persist_rollout_items(&[RolloutItem::RetainedContext(event)])
                .await;
        }
    }
}
