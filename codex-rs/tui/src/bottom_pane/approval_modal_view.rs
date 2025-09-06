use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::WidgetRef;

use crate::app_event_sender::AppEventSender;
use crate::user_approval_widget::ApprovalRequest;
use codex_core::protocol::ReviewDecision;
use crate::user_approval_widget::UserApprovalWidget;

use super::BottomPane;
use super::BottomPaneView;
use super::CancellationEvent;

/// Modal overlay asking the user to approve/deny a sequence of requests.
pub(crate) struct ApprovalModalView<'a> {
    current: UserApprovalWidget<'a>,
    queue: Vec<ApprovalRequest>,
    app_event_tx: AppEventSender,
}

impl<'a> ApprovalModalView<'a> {
    pub fn new(request: ApprovalRequest, app_event_tx: AppEventSender) -> Self {
        Self {
            current: UserApprovalWidget::new(request, app_event_tx.clone()),
            queue: Vec::new(),
            app_event_tx,
        }
    }

    pub fn enqueue_request(&mut self, req: ApprovalRequest) {
        self.queue.push(req);
    }

    /// Advance to next request if the current one is finished. If the
    /// current decision was Abort and there are no further queued
    /// approvals, immediately clear the running status so the user can
    /// continue without needing an extra Ctrl-C.
    fn maybe_advance(&mut self, pane: &mut BottomPane<'a>) {
        if self.current.is_complete() {
            let last = self.current.last_decision();
            if let Some(req) = self.queue.pop() {
                self.current = UserApprovalWidget::new(req, self.app_event_tx.clone());
            } else if matches!(last, Some(ReviewDecision::Abort)) {
                // Locally drop the running indicator so the UI becomes idle
                // immediately after an Abort decision. Core also receives the
                // decision and will perform cleanup on its side.
                pane.set_task_running(false);
                pane.clear_ctrl_c_quit_hint();
                pane.update_status_text(String::new());
            }
        }
    }
}

impl<'a> BottomPaneView<'a> for ApprovalModalView<'a> {
    fn handle_key_event(&mut self, pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        self.current.handle_key_event(key_event);
        self.maybe_advance(pane);
    }

    fn on_ctrl_c(&mut self, pane: &mut BottomPane<'a>) -> CancellationEvent {
        self.current.on_ctrl_c();
        self.queue.clear();
        // Mirror Abort behavior for immediate UX: clear running state now.
        pane.set_task_running(false);
        pane.clear_ctrl_c_quit_hint();
        pane.update_status_text(String::new());
        CancellationEvent::Handled
    }

    fn is_complete(&self) -> bool {
        self.current.is_complete() && self.queue.is_empty()
    }

    fn desired_height(&self, width: u16) -> u16 {
        self.current.desired_height(width)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        (&self.current).render_ref(area, buf);
    }

    fn try_consume_approval_request(&mut self, req: ApprovalRequest) -> Option<ApprovalRequest> {
        self.enqueue_request(req);
        None
    }
}

#[cfg(all(test, feature = "legacy_tests"))]
mod tests {
    use super::*;
    use crate::app_event::AppEvent;
    use std::sync::mpsc::channel;

    fn make_exec_request() -> ApprovalRequest {
        ApprovalRequest::Exec {
            id: "test".to_string(),
            command: vec!["echo".to_string(), "hi".to_string()],
            reason: None,
        }
    }

    #[test]
    fn ctrl_c_aborts_and_clears_queue() {
        let (tx_raw, _rx) = channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let first = make_exec_request();
        let mut view = ApprovalModalView::new(first, tx);
        view.enqueue_request(make_exec_request());

        let (tx_raw2, _rx2) = channel::<AppEvent>();
        let mut pane = BottomPane::new(super::super::BottomPaneParams {
            app_event_tx: AppEventSender::new(tx_raw2),
            has_input_focus: true,
            enhanced_keys_supported: false,
            placeholder_text: "Ask Codex to do anything".to_string(),
            disable_paste_burst: false,
        });
        assert_eq!(CancellationEvent::Handled, view.on_ctrl_c(&mut pane));
        assert!(view.queue.is_empty());
        assert!(view.current.is_complete());
        assert!(view.is_complete());
    }
}
