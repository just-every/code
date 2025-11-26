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
