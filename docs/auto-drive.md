# Auto Drive

What Auto Drive is, how to start it, and how it behaves in Every Code.

## Start points
- TUI: `/auto <goal>`. If you omit the goal and there is recent history, Code proposes one for you. `/auto settings` jumps straight to the Auto Drive pane.
- CLI: `code exec --auto "<goal>"` or `code exec "/auto <goal>"`. A goal is required when launching headless.
- Precondition: Full Auto mode (danger-full-access + approval=never) must be selected in the TUI; otherwise you’ll see a warning and Auto Drive will not start.

## Goal handling
- Images passed on the CLI are attached before the first turn.
- If you supply no goal and there is nothing in history to derive from, Auto Drive will not start.

## How it runs
- Each turn Auto Drive drafts a plan, prepares commands, optionally assigns agents, and waits for your confirmation (or the countdown) before running.
- The transcript is kept in memory and compacted automatically; you’ll see a notice if history was trimmed.
- If an `AUTO_AGENTS.md` exists, its guidance is applied to the run alongside any AGENTS.md rules.

## Agents
- Auto Drive can spawn helper agents during a turn. Toggle this with `agents_enabled` in Auto Drive settings.
- Outside a git repo, Auto Drive forces those agents to run read-only to avoid unintended writes.

## Observer
- A lightweight watchdog reviews the run every `auto_drive_observer_cadence` turns (default 5). If it spots trouble it surfaces guidance in the banner. Set the cadence to `0` to disable.

## Sandbox and approvals
- TUI: needs `danger-full-access` plus `approval_policy=never` so turns aren’t blocked by prompts.
- CLI: `--auto` forces approvals off; add `--full-auto` (or `--dangerously-bypass-approvals-and-sandbox`) if you want Auto Drive to make edits and run commands.

## Continue and countdown modes
- `continue_mode`: `immediate`, `ten-seconds` (default), `sixty-seconds`, `manual`.
- In countdown modes, the Auto Drive card shows a timer; Enter proceeds early, Esc reopens the draft, 0 auto-submits.
- Manual mode pauses after each prepared prompt until you confirm.

## Stop and pause
- Press Esc while Auto Drive is active to pause or stop (context-dependent). Countdown modes show this hint in the footer.
- Approval dialogs never capture Esc; it always reaches Auto Drive.

## Review, QA, diagnostics
- `review_enabled` (default true) can insert a review gate; the card shows “Awaiting review.”
- `qa_automation_enabled` and `cross_check_enabled` (default true) allow diagnostics and cross-check turns before continuing.
- `auto_resolve_review_attempts` limits how many times Auto Drive will auto-resolve review feedback (default 5).

## Models
- Defaults: model `gpt-5.1`, reasoning effort `high`.
- Toggle “use chat model” in settings to reuse your current chat model/effort instead of the dedicated Auto Drive model.

## UI surfaces
- Auto Drive card shows status (Ready, Waiting, Thinking, Running, Awaiting review, Failed/Stopped), the goal, action log, token/time counters, countdown, and a celebration on success.
- Bottom pane header mirrors status and shows hints (Ctrl+S settings, Esc stop, whether agents/diagnostics are on).

## Resume and persistence
- History is kept in memory; there’s no Auto Drive–specific history file. If trimmed, you’ll see a note.
- You can resume a session as usual; Auto Drive can derive a goal from restored history.
- CLI `--output-last-message` still works here if you only need the final reply.

## Settings (config.toml)
- Top-level keys: `auto_drive_use_chat_model` (default false), `auto_drive_observer_cadence` (default 5).
- `[auto_drive]` defaults: `review_enabled=true`, `agents_enabled=true`, `qa_automation_enabled=true`, `cross_check_enabled=true`, `observer_enabled=true`, `coordinator_routing=true`, `continue_mode="ten-seconds"`, `model="gpt-5.1"`, `model_reasoning_effort="high"`, `auto_resolve_review_attempts=5`.
- All of these can be changed from `/auto settings` in the TUI or directly in `config.toml`.

## Tips
- Stay in the TUI if you want countdowns and visual status; use `code exec --auto` for CI or scripted flows.
- If Auto Drive stops because it couldn’t derive a goal, rerun `/auto <goal>` with a short, specific instruction.
- Turn off agents in `/auto settings` if you want a single-model run.
