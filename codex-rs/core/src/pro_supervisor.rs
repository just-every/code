use std::sync::Arc;
use std::time::Duration;

use tokio::task::AbortHandle;

use crate::codex::{Session, PRO_SUBMISSION_ID};
use crate::protocol::{ProEvent, ProPhase, ProStats};

/// Lightweight Pro Mode supervisor that periodically publishes status ticks.
pub struct ProSupervisorHandle {
    abort: AbortHandle,
}

impl ProSupervisorHandle {
    pub fn abort(self) {
        if !self.abort.is_finished() {
            self.abort.abort();
        }
    }
}

pub fn spawn(session: Arc<Session>) -> ProSupervisorHandle {
    let handle = tokio::spawn(async move {
        loop {
            session
                .emit_pro_event(
                    PRO_SUBMISSION_ID,
                    ProEvent::Status {
                        phase: ProPhase::Idle,
                        stats: ProStats::default(),
                    },
                )
                .await;
            tokio::time::sleep(Duration::from_millis(1_500)).await;
        }
    })
    .abort_handle();

    ProSupervisorHandle { abort: handle }
}
