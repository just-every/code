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
