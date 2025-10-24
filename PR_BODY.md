## Overview
- Stabilize Auto Drive state transitions so manual overrides no longer resume automation unexpectedly.
- Ensure Esc reliably cancels running agents before exiting Auto Drive.

## Key changes
- Add `AutoRunPhase` to the controller with helpers (`set_phase`, `is_auto_active`, `current_phase`, `set/clear/should_bypass_coordinator_next_submit`) and a one-shot bypass flag.
- Update ChatWidget: PausedManual sets bypass; manual submit clears bypass and leaves Auto Idle unless `/auto` explicitly restarts it; all coordinator/resume paths are gated on `!should_bypass`; `/auto` with no goal resets to Idle.
- Esc routing now cancels agents on the first press and stops Auto Drive on the second.
- Test harness uses `auto_spawn_countdown` / `auto_handle_countdown`; the core crate re‑exports `TurnComplexity`.
- Added regression tests for PausedManual manual submit, post-`auto_stop` submit, and Esc cancel→exit flows.

## Tests
- `./build-fast.sh`
- `cargo test -p code-tui` (warnings only)

## Risk / Rollback
- Low risk; changes are scoped to Auto Drive plumbing and are test covered. Roll back via `git revert` if manual control regresses.

## Verification
- Manual: start Auto Drive in the TUI, press Esc once (agents cancel) then Esc again (Auto Drive exits).
- Manual: pause Auto Drive for manual edit, submit the edited prompt — automation remains Idle until `/auto` is issued.
- Automated: `cargo test -p code-tui -- auto_drive`
