# Background Ordering Refactor Plan

## Objective

Guarantee that all UI background events carry immutable order metadata captured at the moment the originating action begins, so banners can no longer drift into later turns. The refactor removes the fallback heuristic that re-evaluates global UI state at delivery time and replaces it with explicit handles that freeze the request ordinal and sequence.

## Scope

- `code-rs/tui/src/chatwidget.rs`
- `code-rs/tui/src/app_event_sender.rs`
- Async producers that currently call `AppEventSender::send_background_event*`
- Associated unit tests under `code-rs/tui/src/chatwidget.rs`

## Constraints & Signals

- Preserve existing `OrderMeta` semantics (`output_index = i32::MAX`) for tail background notices.
- Avoid regressions to snapshot/restore flows that rely on `UiBackgroundOrderHandle` sequence rehydration.
- Maintain compatibility for `BackgroundPlacement::BeforeNextOutput`, but refuse tail inserts without an explicit order.
- Update or add tests covering:
  - Slash commands (`/branch`) emitting banners after user prompts without a provider turn.
  - Ghost snapshot timeout notices.
  - Restored sessions emitting late events.

## Implementation Strategy

### 1. Promote Order Handles to Public API

- Expose a new lightweight wrapper (e.g., `BackgroundOrderTicket`) that encapsulates `UiBackgroundOrderHandle`.
- Store it in `code-rs/tui/src/app_event_sender.rs` so async producers can hold a clone even after moving off the main thread.
- Provide constructors on `ChatWidget`:
  - `fn make_background_tail_ticket(&mut self) -> BackgroundOrderTicket`
  - `fn make_background_before_next_output_ticket(&mut self) -> BackgroundOrderTicket`

### 2. Eliminate Orderless Tail Sends

- Deprecate `AppEventSender::send_background_event` and `send_background_event_before_next_output` by replacing them with:
  - `send_background_event_with_ticket(ticket, message)`
  - `send_background_before_next_output_with_ticket(ticket, message)`
- Enforce at compile-time: no tail placement path accepts `order: None`.
- Update `AppEvent::InsertBackgroundEvent` producers to require an `OrderMeta` (or ticket) argument.

### 3. Capture Tickets at Slash Command Dispatch

- In `ChatWidget::handle_branch_command`, grab a tail ticket before spawning the async block and pass it into the closure.
- Repeat for other slash commands (`/merge`, `/cmd`, `/update`, etc.) and any UI-only flows that spawn tasks.
- For synchronous notices (e.g., “Creating branch worktree…”), use the “before next output” ticket captured before the prompt increments the pending counter.

### 4. Reset Pending Prompt State on UI-Only Commands

- Introduce a helper `fn consume_pending_prompt_for_ui_only_turn(&mut self)` that decrements the counter once the slash command response is queued.
- Call it immediately after scheduling the command’s banners so the UI state reflects “current turn resolved.”
- Ensure it does not fire when a provider request truly starts (`TaskStarted` still clears the counter).

### 5. Update Async Helpers & Services

- `ghost_snapshot.rs`, auto-upgrade, GitHub Actions watcher, browser connectors, and cloud task helpers must accept a ticket captured at trigger time.
- For global helpers (`AppEventSender` clones passed into other modules), thread the ticket through their APIs so they no longer call the legacy `send_background_event`.

### 6. Remove Fallback Logic

- Delete the branch in `insert_background_event_with_placement` that synthesizes order metadata when `order.is_none()`.
- Simplify `system_order_key` and `background_tail_request_ordinal` to assume `order` is always present for tail placements.
- Add defensive logging/assertions that fire if new code attempts to insert without order metadata.

### 7. Expand Test Coverage

- Extend existing tests to cover the previously failing scenario:
  - Seed `pending_user_prompts_for_next_turn = 1`, capture a ticket, enqueue a tail event, assert `OrderKey.req` matches the current request.
- Add async-style tests that simulate `/branch` completion without `TaskStarted`, verifying history ordering remains stable after subsequent user prompts.
- Snapshot tests for restored sessions to confirm sequence rehydration still advances monotonically.

### 8. Migration & Cleanup

- Provide shims during transition (feature-gated) if needed, but plan to remove the old API once all call sites migrate.
- Update developer docs to explain ticket usage for background events.
- Run `./build-fast.sh` and targeted unit tests.

## Current Progress (2025-10-02)

### ✅ Step 1 – Ticket API

- `BackgroundOrderTicket` exposed with cloning semantics and production/test constructors.
- Tail and before-next-output helpers (`make_background_tail_ticket`, `make_background_before_next_output_ticket`) now power all UI-issued banners.
- Conversation bootstrap failures, `/branch`, ghost snapshots, auto-upgrade notices, GitHub Actions watcher, browser/CDP flows, cloud tasks, `/cloud` subcommands, and slash-command banners all mint tickets up front.

### ✅ Step 2 – Ticket Plumbing Throughout the UI

- All chatwidget flows (browser toggles, cloud tasks, branch/merge, Chrome connection, `/login`, `/update`, `/github`, etc.) now thread `BackgroundOrderTicket` instances into async closures and visible toasts.
- `consume_pending_prompt_for_ui_only_turn()` clears stale prompt counters so slash commands no longer misclassify delayed banners as the “next” turn.
- Bottom pane views (update settings, notifications, theme selection, login/add-account) and background helpers now require tickets at construction time.

### ✅ Step 3 – Approval & Modal flows

- `UserApprovalWidget` accepts a before-next-output ticket; decisions send ordered banners instead of relying on fallback synthesis.
- `ApprovalModalView` queues `(ApprovalRequest, BackgroundOrderTicket)` pairs so multiple approvals preserve per-request ordinals.
- Tests were updated to exercise the new APIs via `BackgroundOrderTicket::test_for_request`.

### ✅ Step 4 – Fallback Removal & API Cleanup

- `ChatWidget::insert_background_event_with_placement` now refuses tail inserts without explicit order metadata (logs and drops the message instead of synthesizing).
- Deprecated helpers (`send_background_event`, `send_background_event_before_next_output`) were removed from `AppEventSender`.
- `push_background_tail` and related helpers internally mint tickets; legacy `order: None` paths are gone.
- Regression test `background_event_before_next_output_precedes_later_cells` now verifies ordered insertions using explicit tickets.

## Remaining Focus Areas

1. **Additional regression coverage** – add tests capturing:
   - Pending prompt > 0 when slash commands emit multi-stage banners.
   - Session restore + late async completion to ensure monotonic sequence rehydration.
2. **Docs & developer guidance** – document ticket usage patterns for async producers (README/DEVELOPING or a dedicated dev note).
3. **Manual QA** – run `/branch`, `/cloud`, browser toggles, approval modals, and login flows end-to-end to confirm grouping across user prompts.

## Verification Checklist

- [ ] Add targeted unit tests covering pending prompt scenarios with explicit tickets.
- [ ] Document ticket best practices for new async producers.
- [ ] Manual walkthrough keeps `/branch` banners ordered after subsequent prompts.
- [ ] Manual walkthrough keeps approval decisions adjacent to the associated command/tool begin cell.
- [ ] `./build-fast.sh` and focused tests pass without warnings.

## Verification Checklist

- All background banners stay adjacent to their originating prompt across repeated `/branch`, `/merge`, ghost snapshot, and auto-upgrade scenarios.
- No warnings about “legacy background event without order” remain.
- `tail_background_event_keeps_position_vs_next_output` passes with pending prompts set both to 0 and 1.
- Manual TUI walkthrough confirms ordering stability after multiple delayed async completions.
