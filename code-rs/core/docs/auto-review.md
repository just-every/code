# Auto-review / Auto-resolve Workflow (Exec, TUI, Auto-drive)

This note summarizes how automated reviews flow across the CLI layers and what invariants keep snapshots fresh and non-overlapping.

## Overview
- **Exec (/review + auto-resolve):** Captures a ghost snapshot, acquires the shared review lock, runs review, and may loop fixes → follow-up review until clean or limits hit.
- **TUI (interactive + background):** Triggers reviews, renders progress, and now shares the same lock + epoch rules so background reviews cannot overlap or reuse stale snapshots.
- **Auto-drive:** Orchestrates turns; when it triggers reviews/auto-resolve it also takes the global lock and relies on the same snapshot epoch to reject stale follow-ups.

## Core states
- **Review lock (file-based):** Only one review pipeline (exec/TUI/auto-drive) may run at a time. If lock acquisition fails, the review is skipped/deferred.
- **Snapshot epoch (monotonic counter):** Bumped on every git mutation we control (ghost snapshots, git helpers, worktree/remote ops). Follow-up reviews compare epochs and abort if the world moved.
- **Auto-resolve phases:**
  - `PendingFix` → apply patch
  - `AwaitingFix` → wait for model output
  - `WaitingForReview` → capture fresh snapshot, re-review
  - Abort when identical snapshot, stale base, or epoch advanced

## Ghost snapshots
- Ghost commits are taken via `CreateGhostCommitOptions` **with** `post_commit_hook(&|| bump_snapshot_epoch())` so every snapshot invalidates prior review targets.
- Stored metadata (commit hash + epoch) is compared before follow-up reviews; mismatch stops the loop.

## Lock + epoch rules
- Acquire the shared lock before sending a review or follow-up. Release on completion/stop/reset.
- If lock busy: skip/defer rather than overlap.
- If `current_snapshot_epoch` differs from the recorded epoch for the review context, abort and recapture.

## Git mutations: use the helpers
- Use `chatwidget::run_git_command` (TUI) or the auto-drive git helper; both bump epoch on mutating verbs (`pull`, `checkout`, `merge`, `apply`).
- Worktree/remote management goes through `core::git_worktree`, which bumps epoch on remove/add/remote updates.
- Ghost snapshots already bump epoch via the post-commit hook.
- New git touchpoints must either call these helpers or call `bump_snapshot_epoch()` immediately after a successful mutation. The `git_mutation_guard` test enforces this for TUI/auto-drive.

## Cleanup
- Session worktree cleanup acquires the shared lock with retries; successful removal bumps the epoch so stale reviews are invalidated. If lock stays busy, cleanup defers to a later run.

## Failure handling
- Follow-up review skipped when:
  - lock unavailable
  - snapshot identical to last reviewed commit
  - base snapshot no longer ancestor of HEAD
  - epoch advanced (stale snapshot)
- Apply/patch failure + resume must recapture; epoch drift detects staleness.

## What to remember when extending
- Add new git mutations via the shared helpers or bump the epoch manually.
- If you add a new review entrypoint, acquire the shared lock.
- When capturing snapshots, record the epoch and base commit; compare before re-reviewing.
- If you must bypass the guardrails, document the reason and keep the code path read-only.

