# Auto Drive Reliability Playbook

This note collects the timing guarantees, queue semantics, and observability
switches added while hardening the Auto Drive coordinator ↔ TUI pipeline. Use
it as the maintainer reference when investigating double responses, premature
turn endings, or countdown races.

## Sequencing Model

Each coordinator turn is guarded by a monotonically increasing **decision
sequence** (`decision_seq`). The core rules:

- The coordinator increments `decision_seq` immediately before emitting
  `AutoCoordinatorEvent::Decision` (see
  `code_auto_drive_core/src/auto_coordinator.rs`).
- The TUI keeps the current `pending_ack_seq` and refuses to process the next
  decision until the prior sequence has been acknowledged.
- ACKs travel back through the coordinator command channel via
  `AutoCoordinatorCommand::AckDecision { seq }`. The coordinator drops any ACK
  that does not match the active `pending_ack_seq`.
- Stop events have a dedicated `StopAck` to guarantee the coordinator does not
  tear down the run until the UI finishes draining history.

With this in place, every decision is serialized by `(request_ordinal,
output_index, decision_seq)` and cannot overtake an earlier turn.

## Queue Drain Rules

User messages that arrive while Auto Drive is still streaming are held in
`queued_user_messages` inside the chat widget. The drain logic guarantees:

- **Gate on activity** – messages only drain when Auto Drive is idle or the
  coordinator explicitly hands us control. While a turn is active the widget
  enqueues messages and displays them as “(queued)”.
- **Coordinator routing** – queued input is dispatched through
  `Op::QueueUserInput`, giving the coordinator a chance to serialize the next
  turn and ensuring the seq/ACK path stays intact.
- **Single dispatch** – a message may only be enqueued or dispatched in a given
  tick. The guard that previously allowed both in the same cycle has been
  removed to avoid duplicate submissions.

These rules eliminate the double-dispatch window that previously created
duplicate assistant turns when a user typed during streaming.

## Countdown & Decision Sequence

`StartCountdown` now carries the `decision_seq` that spawned it. The controller
tracks `countdown_decision_seq` and every tick validates:

- The countdown id matches the one currently active.
- The tick’s `decision_seq` equals the tracked value.
- Auto Drive is still waiting to submit (paused runs and manual review block
  countdowns).

Ticks that fail any predicate are ignored. This prevents stale timers from
submitting prompts after we pause or restart a run.

## Stop Acknowledgment

When the coordinator emits a terminal decision the UI responds with
`AutoCoordinatorCommand::StopAck`. The coordinator waits for that ack before
transitioning into the stopped state. This avoids the race where history updates
(or queue drains) would arrive after the coordinator already considered the run
finished.

## Observability & Metrics

- **Structured tracing** – key pathways log through the
  `auto_drive::coordinator`, `auto_drive::queue`, `auto_drive::history`, and
  `auto_drive::countdown` targets. Example:

  ```text
  INFO auto_drive::coordinator: Decision seq=21 dispatched; awaiting history update
  INFO auto_drive::history: History updated for seq=21; dedup applied
  INFO auto_drive::queue: Queued user input drained via QueueUserInput after turn completion
  WARN auto_drive::countdown: Ignoring stale countdown tick due to mismatched decision_seq
  ```

- **SessionMetrics** – the coordinator updates `duplicate_items` and
  `replay_updates` counters so we can spot dedup anomalies from telemetry.
- **Queue tracing** – enqueue and drain paths log batch sizes and the auto-drive
  active flag; watch for `[queue]` entries to confirm gated draining.

To enable verbose tracing during tests or repros set:

```bash
RUST_LOG=auto_drive::coordinator=debug,auto_drive::queue=debug,auto_drive::history=info,auto_drive::countdown=debug
RUST_LOG_STYLE=never
NO_COLOR=1
```

## How to Verify

1. **Unit tests**
   - `cargo test -p code-tui --features test-helpers` (covers queue gating and
     decision handling).
   - `cargo test -p code-auto-drive-core --lib` (covers controller countdown seq
     checks and coordinator pending ACK logic).
2. **Mid-turn queueing**
   - Run `cargo test -p code-tui --features test-helpers test_mid_turn_user_input_queueing -- --nocapture`.
   - Inspect the VT100 snapshot suite for regressions:
     `cargo test -p code-tui --test vt100_chatwidget_snapshot --features test-helpers -- --nocapture`.
3. **Trace snapshot**
   - `cargo test -p code-tui --features test-helpers test_trace_logging_snapshot -- --nocapture`
     for a concise log proving seq → history → ack ordering, queue drain, and
     countdown rejection.

## Common Pitfalls

| Symptom | Likely Cause | Fix |
|---------|--------------|-----|
| Duplicate assistant messages | Queued input drained while Auto Drive active | Confirm queued dispatch only happens when `auto_state.is_active()` is false and the coordinator has acked the prior decision. |
| Turn finishes early after pause | Countdown tick ignored but follow-up decision still pending | Check countdown traces for mismatched `decision_seq` and ensure `StopAck` has been observed. |
| Coordinator stops before UI flushes history | Missing `StopAck` or handler panic | Verify `StopAck` is emitted and check logs for `auto_drive::history` errors. |
| Continue decision races with final decision | Coordinator emitted next decision before receiving ACK | Inspect `auto_drive::coordinator` trace for `pending_seq` messages; ensure pending ACK gating is functioning. |
| Queue never drains | Coordinator still waiting for ACK | Look for `queueing conversation until ack` in coordinator logs and confirm the UI sent `AckDecision`. |
| Countdown fires instantly | `countdown_decision_seq` mismatch or override set to zero | Confirm controller’s countdown override and that ticks are aligned with the active decision seq. |

## References

- Coordinator sequencing & ACK gating:
  `code-rs/code-auto-drive-core/src/auto_coordinator.rs`
- Countdown guards and effects:
  `code-rs/code-auto-drive-core/src/controller.rs`
- TUI queue and decision handling:
  `code-rs/tui/src/chatwidget.rs`
- Trace harness for manual validation:
  `code-rs/tui/tests/mid_turn_queueing.rs`

Maintain this document as the contract for Auto Drive timing. Any new feature
that touches coordinator sequencing or the queue should update this playbook and
extend the trace/test checklists above.
