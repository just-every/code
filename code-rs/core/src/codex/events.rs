use super::*;

impl Session {
    pub(crate) async fn send_event(&self, event: Event) {
        if let Err(e) = self.tx_event.send(event).await {
            error!("failed to send tool call event: {e}");
        }
    }

    /// Persist an event into the rollout log if appropriate.
    fn persist_event(&self, event: &Event) {
        if !crate::rollout::policy::should_persist_event_msg(&event.msg) {
            return;
        }
        let Some(msg) = crate::protocol::event_msg_to_protocol(&event.msg) else {
            return;
        };
        let recorder = {
            let guard = self.rollout.lock().unwrap();
            guard.as_ref().cloned()
        };
        if let Some(rec) = recorder {
            let order = event
                .order
                .as_ref()
                .map(crate::protocol::order_meta_to_protocol);
            let protocol_event = code_protocol::protocol::RecordedEvent {
                id: event.id.clone(),
                event_seq: event.event_seq,
                order,
                msg,
            };
            tokio::spawn(async move {
                if let Err(e) = rec.record_events(&[protocol_event]).await {
                    warn!("failed to persist rollout event: {e}");
                }
            });
        }
    }

    /// Create a stamped Event with a per-turn sequence number.
    fn stamp_event(&self, sub_id: &str, msg: EventMsg) -> Event {
        let mut state = self.state.lock().unwrap();
        let seq = match msg {
            EventMsg::TaskStarted => {
                // Reset per-sub_id sequence at the start of a turn.
                // We increment request_ordinal per HTTP attempt instead
                // (see `begin_http_attempt`).
                let e = state
                    .event_seq_by_sub_id
                    .entry(sub_id.to_string())
                    .or_insert(0);
                *e = 0;
                0
            }
            _ => {
                let e = state
                    .event_seq_by_sub_id
                    .entry(sub_id.to_string())
                    .or_insert(0);
                *e = e.saturating_add(1);
                *e
            }
        };
        Event {
            id: sub_id.to_string(),
            event_seq: seq,
            msg,
            order: None,
        }
    }

    pub(crate) fn make_event(&self, sub_id: &str, msg: EventMsg) -> Event {
        let event = self.stamp_event(sub_id, msg);
        self.persist_event(&event);
        event
    }

    /// Same as make_event but allows supplying a provider sequence_number
    /// (e.g., Responses API SSE event). We DO NOT overwrite `event_seq`
    /// with this hint because `event_seq` must remain monotonic per turn
    /// and local to our runtime. Provider ordering is carried via
    /// `OrderMeta` when applicable.
    pub(super) fn make_event_with_hint(
        &self,
        sub_id: &str,
        msg: EventMsg,
        _seq_hint: Option<u64>,
    ) -> Event {
        let event = self.stamp_event(sub_id, msg);
        self.persist_event(&event);
        event
    }

    pub(super) fn make_event_with_order(
        &self,
        sub_id: &str,
        msg: EventMsg,
        order: crate::protocol::OrderMeta,
        _seq_hint: Option<u64>,
    ) -> Event {
        let mut ev = self.stamp_event(sub_id, msg);
        ev.order = Some(order);
        self.persist_event(&ev);
        ev
    }

    // Kept private helpers focused on ctx-based flow to avoid misuse.

    pub(crate) async fn send_ordered_from_ctx(&self, ctx: &ToolCallCtx, msg: EventMsg) {
        let order = ctx.order_meta(self.current_request_ordinal());
        let ev = self.make_event_with_order(&ctx.sub_id, msg, order, ctx.seq_hint);
        let _ = self.tx_event.send(ev).await;
    }

    pub(super) fn current_request_ordinal(&self) -> u64 {
        let state = self.state.lock().unwrap();
        state.request_ordinal
    }
}
