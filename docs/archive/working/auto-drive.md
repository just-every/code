# Auto Drive Phase Migration TODO

## Phase Invariants Snapshot

- `Idle` — Auto Drive inactive; no pending coordinator/diagnostics/review; countdown cleared.
- `AwaitingGoalEntry` — Auto Drive inactive; goal entry composer visible; legacy `awaiting_goal_input` flag collapses into this variant.
- `Launching` — Preparing first turn; mirrors `Idle` legacy booleans until launch success/failure.
- `Active` — Run active with no pending gates; diagnostics/review/manual/edit/transient flags cleared.
- `PausedManual { resume_after_submit, bypass_next_submit }` — Run active; manual editor visible; `resume_after_manual_submit` mirrors payload and bypass flag controls coordinator auto-submit.
- `AwaitingCoordinator { prompt_ready }` — Run active; prompt staged; coordinator waiting true regardless of `prompt_ready`; countdown enabled when legacy auto-submit applies.
- `AwaitingDiagnostics` — Awaiting model response (streaming); coordinator waiting false; review/manual flags cleared.
- `AwaitingReview { diagnostics_pending }` — Awaiting user review; diagnostics chip toggled by payload; other waits cleared.
- `TransientRecovery { backoff_ms }` — Backoff between restart attempts; transient wait flag true; coordinator/manual/review cleared.

- Remove legacy fields from `AutoDriveController` that duplicate phase state (`active`, `awaiting_submission`, `waiting_for_response`, `paused_for_manual_edit`, `resume_after_manual_submit`, `waiting_for_review`, `waiting_for_transient_recovery`, `coordinator_waiting`).
- Update remaining TUI call sites (outside ChatWidget hot paths) to use controller helpers (`is_active`, `is_paused_manual`, `resume_after_submit`, `awaiting_coordinator_submit`, `awaiting_review`, `in_transient_recovery`).
- Replace test harness helpers in `tui/tests` that mutate legacy flags with phase-aware helpers or controller transitions.
- Ensure ESC routing (`describe_esc_context`, `execute_esc_intent`) exclusively inspects `AutoRunPhase`/helpers and removes stop-gap flag checks.
- Add unit/VT100 coverage for manual pause/resume and transient recovery sequences under the new phase helpers.
- Acceptance: `./build-fast.sh` green; no direct reads/writes of legacy flags across the repo; ESC flows verified in snapshot tests.
# Auto Drive State Inventory

This document catalogs every `auto_state` field access across the TUI and controller so we can migrate toward single-phase semantics without missing any flag interactions.

## Sources Scanned

- `code-rs/tui/src/chatwidget.rs`
- `code-rs/tui/src/bottom_pane/auto_coordinator_view.rs`
- `code-rs/tui/src/bottom_pane/auto_drive_settings_view.rs`
- `code-rs/tui/src/bottom_pane/paste_burst.rs`
- `code-rs/tui/src/chatwidget/smoke_helpers.rs`
- `code-rs/code-auto-drive-core/src/controller.rs`

Each entry below lists read vs. write occurrences (line numbers and snippets). Counts help highlight high-traffic fields.

## Field Classification

| Field | Category | Notes |
| --- | --- | --- |
| `active` | Phase control | Primary on/off latch for Auto Drive; should collapse into `AutoRunPhase::Active`/`Idle`. |
| `awaiting_submission` | Phase control | Drives countdown and gating for prompt submission; redundant with `AutoRunPhase::AwaitingCoordinator`. |
| `waiting_for_response` | Phase control | Distinguishes coordinator wait vs. live-streaming response; overlaps with `AwaitingDiagnostics`. |
| `paused_for_manual_edit` | Phase control | Legacy manual-edit gate; duplicative of `AutoRunPhase::PausedManual`. |
| `resume_after_manual_submit` | Phase control | Remembers whether manual submits should resume automatically; belongs inside `PausedManual` payload. |
| `waiting_for_review` | Phase control | Tracks post-turn review gating; mirrors `AutoRunPhase::AwaitingReview`. |
| `waiting_for_transient_recovery` | Phase control | Marks exponential backoff windows; mirrors `AutoRunPhase::TransientRecovery`. |
| `coordinator_waiting` | UI view data | Indicates coordinator prompt handshake state; used for progress copy and hint toggles.

## `active` (phase control)

- Reads (8)
  - `code-rs/tui/src/chatwidget.rs:14153` — guard before mutating review UI while idle.
  - `…:14307` — skip coordinator pane when idle.
  - `…:14587` — hide goal banner unless active or awaiting goal.
  - `…:14613` — bail on streaming renderer unless active.
  - `…:14646` — hide progress plaque while idle.
  - `…:14675` — stop summary aggregation if run stopped.
  - `…:14762` — guard reasoning title updates.
  - `…:23380` — keep review shortcuts disabled when idle.
- Writes (10)
  - `code-rs/tui/src/chatwidget.rs:21879` — smoke helper seeds active state for tests.
  - Additional nine test helpers (`…:22020`, `…:22047`, `…:22128`, `…:22194`, `…:22250`, `…:22304`, `…:22333`, `…:22394`, `chatwidget/smoke_helpers.rs:311`).
  - Controller mirror updates appear in `sync_booleans_from_phase()` (`code-auto-drive-core/src/controller.rs:263`, `273`, `283`, `293`, `303`, `313`, `323`, `479`, `510`).

## `awaiting_submission` (phase control)

- Reads (5)
  - `code-rs/tui/src/chatwidget.rs:14570` — decides whether to elide ellipsis on summary lines during pending submit.
  - Controller helper logic uses it (`code-auto-drive-core/src/controller.rs:678`, `699`, `774`, `841`).
- Writes (9)
  - All originate from controller transitions (`controller.rs:264`, `274`, `284`, `294`, `304`, `314`, `324`, `568`, `655`).
  - No TUI caller writes directly.

## `waiting_for_response` (phase control)

- Reads (10)
  - Reactive UI checks (`chatwidget.rs:13215`, `14153`, `14403`, `14434`, `14514`, `14528`, `14773`).
  - Test assertions (`chatwidget.rs:22037`, `22115`, `22184`).
- Writes (10)
  - TUI clears after finalization (`chatwidget.rs:13557`).
  - Smoke helpers and fixtures seed state (`chatwidget.rs:22021`, `22049`, `22130`, `22396`).
  - Controller toggles across transitions (`controller.rs:265`, `275`, `285`, `295`, `305`, `315`, `325`, `336`, `498`, `566`).

## `paused_for_manual_edit` (phase control)

- Reads (6)
  - Manual editor banner (`chatwidget.rs:14369`).
  - Controller helper guards (`controller.rs:678`, `700`, `775`, `830`, `841`).
- Writes (8)
  - Solely controller-managed (`controller.rs:267`, `277`, `287`, `297`, `307`, `317`, `327`, `569`).

## `resume_after_manual_submit` (phase control)

- Reads (1)
  - Manual resume decision (`controller.rs:836`).
- Writes (9)
  - Controller clears or copies flag during transitions (`controller.rs:268`, `278`, `288`, `298`, `308`, `318`, `328`, `356`, `570`).

## `waiting_for_review` (phase control)

- Reads (13)
  - Review UI gating and tests (`chatwidget.rs:13819`, `14182`, `14378`, `22032`, `22093`, `22099`, `22174`, `22180`, `22221`, `22437`, `22452`, `23380`).
  - Phase helper fallback (`controller.rs:845`).
- Writes (9)
  - Controller transitions (`controller.rs:269`, `279`, `289`, `299`, `309`, `319`, `329`, `571`).
  - ChatWidget clears on forced stop (`chatwidget.rs:23386`).

## `waiting_for_transient_recovery` (phase control)

- Reads (1)
  - Phase helper fallback (`controller.rs:850`).
- Writes (9)
  - ChatWidget clears before scheduling restart (`chatwidget.rs:13456`).
  - Controller transitions (`controller.rs:270`, `280`, `290`, `300`, `310`, `320`, `330`, `383`, `565`).

## `coordinator_waiting` (UI view data)

- Reads (3)
  - Coordinator progress hints (`chatwidget.rs:14403`, `14434`, `14529`).
- Writes (12)
  - ChatWidget resets on completion (`chatwidget.rs:13400`, `13558`).
  - Controller mirrors during transitions (`controller.rs:266`, `276`, `286`, `296`, `306`, `316`, `326`, `340`, `499`, `567`).

## Current Boolean Combinations

- `waiting_for_response && !coordinator_waiting` — drives “model is thinking” messaging and hides coordinator countdown. (`chatwidget.rs:14403`)
- `awaiting_submission && !paused_for_manual_edit` — determines when countdown and auto-submit button should be live. (`controller.rs:678`, `chatwidget.rs:14501`)
- `active && waiting_for_review` — blocks manual resume until review flow resolves. (`chatwidget.rs:22093`)
- `is_paused_manual() && should_bypass_coordinator_next_submit()` — keeps manual edit overlay while skipping coordinator prompt; helper now derives directly from `AutoRunPhase::PausedManual { bypass_next_submit }`. (`chatwidget.rs:14045` vicinity)
- `in_transient_recovery() && waiting_for_transient_recovery` — redundant gating around restart timer, still split between phase helper and boolean. (`controller.rs:850`)

These combinations highlight where the new `AutoRunPhase` variants must carry the data currently modeled via multiple booleans, allowing legacy mirrors to be dropped once consumers migrate.
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
# Auto QA Orchestration

## Overview

- Auto Drive now runs the coordinator and observer threads in parallel.
- The coordinator handles CLI execution and agent planning.
- QA orchestration owns review cadence, observer bootstrap, and the
  forced cross-check before completion.

## Observer Lifecycle

1. **Bootstrap** – The ChatWidget starts the observer worker and sends a
   bootstrap prompt. The observer performs a read-only scan, records a
   baseline summary, and emits `AppEvent::AutoObserverReady`. Cadence
   triggers remain paused until this event arrives.
2. **Delta ingestion** – After bootstrap only new user and assistant
   turns (no reasoning) are forwarded. ChatWidget tracks
   `observer_history.last_sent_index` so the observer never replays the
   full transcript.
3. **Thinking stream** – During bootstrap, cadence, and cross-check
   prompts the observer streams reasoning via
   `AppEvent::AutoObserverThinking`. ChatWidget stores each frame in
   `ObserverHistory` and labels it in the Auto Threads overlay
   (Bootstrap / Observer / Cross-check thinking).
4. **Cadence checks** – On the configured cadence the observer reviews
   the latest delta. Failures push banners such as “Observer guidance:
   …” and can replace the CLI prompt; successes only update telemetry.
5. **Cross-check reuse** – When the coordinator reports
   `finish_success`, it slices the observer transcript starting at
   `observer_history.bootstrap_len` and issues `BeginCrossCheck`. The
   observer reuses that slice, runs with a stricter tool policy, and
   only if the cross-check passes does the coordinator forward the
   pending decision. Failures convert to a restart banner and abort
   completion.

## Tool Policies by Mode

- **Bootstrap (`ObserverMode::Bootstrap`)** – Read-only tools (web
  search only) so the observer can assess the repository without
  modifying files.
- **Cadence (`ObserverMode::Cadence`)** – Limited tools (web search) for
  light guidance while the run is in flight.
- **Cross-check (`ObserverMode::CrossCheck`)** – Full audit tools (local
  shell plus web search) so the observer can rerun commands and verify
  results before finish.

## UI and History

- `ObserverHistory` persists observer exchanges and reasoning frames.
  Auto Threads overlay entries now include the observer mode in their
  label.
- Banners surface milestones: “Observer bootstrap completed.”,
  “Cross-check in progress.”, “Cross-check successful.” Failures include
  guidance text or restart notices.

## Teardown and Restart

- `auto_stop` and automatic restarts clear `ObserverHistory`, reset the
  readiness flag, and send `ResetObserver` so the coordinator rebuilds
  state before the next run.
- The QA orchestrator handle is stopped alongside the observer. All
  cadence and cross-check state is reconstructed on the next launch.

## QA Orchestrator Responsibilities

- Emit `AppEvent::AutoQaUpdate { note }` every cadence window (default
  three turns).
- Emit `AppEvent::AutoReviewRequest { summary }` when diff-bearing turns
  satisfy the review cooldown. These events are now the sole trigger for
  automated reviews.
- Reset cadence state on shutdown and send a final review request if
  diffs remain when automation stops.

## Environment Knobs

- `CODE_QA_CADENCE` — number of turns between observer cadence updates
  (default three).
- `CODE_QA_REVIEW_COOLDOWN_TURNS` — diff-bearing turns before
  `AutoReviewRequest` fires (default one).
- *(Future)* expose tool-policy overrides if operators need to restrict
  cross-check access.

## Future Work

- Collapse multiple QA toggles into a single `qa_automation_enabled`
  flag in `AutoDriveSettings`.
- Expand observer regression coverage when the VT100 harness exposes
  observer fixtures *(TODO).*
