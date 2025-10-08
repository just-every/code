// (none)

use codex_core::protocol::ApplyPatchApprovalRequestEvent;
use codex_core::protocol::ExecApprovalRequestEvent;
use codex_core::protocol::ExecCommandBeginEvent;
use codex_core::protocol::ExecCommandEndEvent;
use codex_core::protocol::McpToolCallBeginEvent;
use codex_core::protocol::McpToolCallEndEvent;
use codex_core::protocol::PatchApplyEndEvent;

use super::ChatWidget;
use super::tools;

#[derive(Debug)]
pub(crate) enum QueuedInterrupt {
    ExecApproval {
        seq: u64,
        id: String,
        ev: ExecApprovalRequestEvent,
    },
    ApplyPatchApproval {
        seq: u64,
        id: String,
        ev: ApplyPatchApprovalRequestEvent,
    },
    ExecBegin {
        seq: u64,
        ev: ExecCommandBeginEvent,
        order: Option<codex_core::protocol::OrderMeta>,
    },
    ExecEnd {
        seq: u64,
        ev: ExecCommandEndEvent,
        order: Option<codex_core::protocol::OrderMeta>,
    },
    McpBegin {
        seq: u64,
        ev: McpToolCallBeginEvent,
        order: Option<codex_core::protocol::OrderMeta>,
    },
    McpEnd {
        seq: u64,
        ev: McpToolCallEndEvent,
        order: Option<codex_core::protocol::OrderMeta>,
    },
    PatchEnd {
        seq: u64,
        ev: PatchApplyEndEvent,
    },
}

#[derive(Default)]
pub(crate) struct InterruptManager {
    queue: Vec<QueuedInterrupt>,
}

impl InterruptManager {
    pub(crate) fn new() -> Self {
        Self { queue: Vec::new() }
    }

    pub(crate) fn push_exec_approval(
        &mut self,
        seq: u64,
        id: String,
        ev: ExecApprovalRequestEvent,
    ) {
        self.queue
            .push(QueuedInterrupt::ExecApproval { seq, id, ev });
    }

    pub(crate) fn push_apply_patch_approval(
        &mut self,
        seq: u64,
        id: String,
        ev: ApplyPatchApprovalRequestEvent,
    ) {
        self.queue
            .push(QueuedInterrupt::ApplyPatchApproval { seq, id, ev });
    }

    pub(crate) fn push_exec_begin(
        &mut self,
        seq: u64,
        ev: ExecCommandBeginEvent,
        order: Option<codex_core::protocol::OrderMeta>,
    ) {
        self.queue
            .push(QueuedInterrupt::ExecBegin { seq, ev, order });
    }

    pub(crate) fn push_exec_end(
        &mut self,
        seq: u64,
        ev: ExecCommandEndEvent,
        order: Option<codex_core::protocol::OrderMeta>,
    ) {
        self.queue.push(QueuedInterrupt::ExecEnd { seq, ev, order });
    }

    pub(crate) fn push_mcp_begin(
        &mut self,
        seq: u64,
        ev: McpToolCallBeginEvent,
        order: Option<codex_core::protocol::OrderMeta>,
    ) {
        self.queue
            .push(QueuedInterrupt::McpBegin { seq, ev, order });
    }

    pub(crate) fn push_mcp_end(
        &mut self,
        seq: u64,
        ev: McpToolCallEndEvent,
        order: Option<codex_core::protocol::OrderMeta>,
    ) {
        self.queue.push(QueuedInterrupt::McpEnd { seq, ev, order });
    }

    pub(crate) fn push_patch_end(&mut self, seq: u64, ev: PatchApplyEndEvent) {
        self.queue.push(QueuedInterrupt::PatchEnd { seq, ev });
    }

    // Plan updates are inserted near-time immediately; no interrupt queue entry needed.

    pub(crate) fn flush_all(&mut self, chat: &mut ChatWidget<'_>) {
        // Ensure stable order
        self.queue.sort_by(|a, b| seq_of(a).cmp(&seq_of(b)));
        for q in self.queue.drain(..) {
            match q {
                QueuedInterrupt::ExecApproval { id, ev, .. } => {
                    chat.handle_exec_approval_now(id, ev)
                }
                QueuedInterrupt::ApplyPatchApproval { seq: _, id, ev } => {
                    chat.handle_apply_patch_approval_now(id, ev);
                }
                QueuedInterrupt::ExecBegin {
                    seq: _, ev, order, ..
                } => {
                    match order.as_ref() {
                        Some(ord) => chat.handle_exec_begin_now(ev, ord),
                        None => {
                            tracing::warn!(
                                "missing OrderMeta in queued ExecBegin; rendering with synthetic order"
                            );
                            // Fall back to immediate render with synthetic ordering inside handler paths.
                            // Use a minimal OrderMeta surrogate by anchoring to last seen request via internal key downstream.
                            chat.handle_exec_begin_now(
                                ev,
                                &codex_core::protocol::OrderMeta {
                                    request_ordinal: chat.last_seen_request_index,
                                    output_index: Some(i32::MAX as u32),
                                    sequence_number: Some(0),
                                },
                            );
                        }
                    }
                }
                QueuedInterrupt::ExecEnd { ev, order, .. } => match order.as_ref() {
                    Some(ord) => chat.handle_exec_end_now(ev, ord),
                    None => {
                        tracing::warn!(
                            "missing OrderMeta in queued ExecEnd; rendering with synthetic order"
                        );
                        chat.handle_exec_end_now(
                            ev,
                            &codex_core::protocol::OrderMeta {
                                request_ordinal: chat.last_seen_request_index,
                                output_index: Some(i32::MAX as u32),
                                sequence_number: Some(1),
                            },
                        );
                    }
                },
                QueuedInterrupt::McpBegin {
                    seq: _, ev, order, ..
                } => {
                    let ok = match order.as_ref() {
                        Some(om) => super::ChatWidget::order_key_from_order_meta(om),
                        None => {
                            tracing::warn!(
                                "missing OrderMeta in queued McpBegin; using synthetic key"
                            );
                            chat.next_internal_key()
                        }
                    };
                    tools::mcp_begin(chat, ev, ok);
                }
                QueuedInterrupt::McpEnd { ev, order, .. } => {
                    let ok = match order.as_ref() {
                        Some(om) => super::ChatWidget::order_key_from_order_meta(om),
                        None => {
                            tracing::warn!(
                                "missing OrderMeta in queued McpEnd; using synthetic key"
                            );
                            chat.next_internal_key()
                        }
                    };
                    tools::mcp_end(chat, ev, ok)
                }
                QueuedInterrupt::PatchEnd { seq: _, ev } => {
                    chat.handle_patch_apply_end_now(ev);
                }
            }
        }
    }
}

fn seq_of(q: &QueuedInterrupt) -> u64 {
    match q {
        QueuedInterrupt::ExecApproval { seq, .. }
        | QueuedInterrupt::ApplyPatchApproval { seq, .. }
        | QueuedInterrupt::ExecBegin { seq, .. }
        | QueuedInterrupt::ExecEnd { seq, .. }
        | QueuedInterrupt::McpBegin { seq, .. }
        | QueuedInterrupt::McpEnd { seq, .. }
        | QueuedInterrupt::PatchEnd { seq, .. } => *seq,
    }
}
