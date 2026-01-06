# Hooks PRD (Work Status)

## Project overview (handoff)
Every Code is a Rust CLI/TUI fork of openai/codex. Hooks are being added to mirror Claude Code’s hook system (prompt + command hooks, multiple lifecycle events). The repo is in `/home/willr/Applications/code` and Rust sources live under `code-rs/` (do not edit `codex-rs`).

Key constraints for this repo:
- Always validate with `./build-fast.sh` from repo root; it can take 20+ minutes.
- Fix all warnings (treat warnings as errors).
- **Never run rustfmt.**
- Use `apply_patch` for single-file edits.

Current status snapshot:
- Hooks config/types/matching + hook client setup are already implemented (see Completed below).
- Prompt hooks + approval flow + event coverage + UI + docs/tests are still missing.
- There are unrelated diffs and stray `.bak` files that should be cleaned before finalizing.

## Completed (already implemented)
- **Config + types**
  - `HooksSettings` and `HooksSettingsOverride` (global + per‑project).
  - Defaults: `codex-mini-latest`, `ReasoningEffort::Low`.
  - Hook config supports `type: command|prompt`, `matcher`, `prompt`.
  - Hook events enum includes: PreToolUse, PostToolUse, Stop, SubagentStop, UserPromptSubmit, SessionStart, SessionEnd, PreCompact, Notification.
- **Matching + storage**
  - Hook matcher supports exact, wildcard, regex, and `|` lists.
  - Project hooks load + match across events.
- **Session wiring**
  - Session stores hooks settings + hooks model provider + hooks client.
  - SessionStart/SessionEnd hooks are triggered.
- **Hook model setup**
  - `hooks_client` created with cheaper model/provider; reasoning summary disabled.
- **Existing command hook execution**
  - Command hooks run for exec/file events:
    - `tool.before`, `tool.after`, `file.before_write`, `file.after_write`.

## Remaining (to implement)
- **Prompt hook runner**
  - Execute `type: prompt` hooks with `hooks_client`.
  - Provide Claude-style JSON I/O contract (see below).
  - Parse outputs: `continue`, `suppressOutput`, `systemMessage`, `hookSpecificOutput.permissionDecision`, `updatedInput`, and Stop/SubagentStop `decision`.
- **Hook approvals**
  - Add HookApproval request/response events in protocol + TUI.
  - On `deny`/`ask`, stop and request explicit user approval (even in yolo).
  - Resume with updated input when approved.
- **Full event coverage**
  - UserPromptSubmit: run on incoming user input (Op::UserInput / QueueUserInput).
  - Stop/SubagentStop: fire at end of turn / subagent completion.
  - PreCompact: fire before compaction and allow systemMessage injection.
  - Notification: fire on notifications (e.g., turn complete).
  - Pre/PostToolUse for *all tools*, not just exec/file hooks.
- **Hook payload contract (Claude-compatible)**
  - Input fields: `session_id`, `transcript_path`, `cwd`, `permission_mode`, `hook_event_name`.
  - Event fields:
    - Pre/PostToolUse: `tool_name`, `tool_input`, `tool_result`, `tool_use_id`.
    - UserPromptSubmit: `user_prompt`.
    - Stop/SubagentStop: `reason` (+ subagent info when available).
    - Notification: notification payload.
- **Settings UI**
  - Add Hooks section in `/settings` with:
    - enabled toggle
    - model selector
    - provider selector
  - Persist to `[hooks]` in `config.toml`.
  - Per-project overrides take precedence.
- **Docs + tests**
  - Update `docs/config.md` for `[hooks]`.
  - Add tests for matcher, prompt output parsing, approval flow.
  - Run `./build-fast.sh` (required).

## Notes / cleanup needed
- **Unrelated diffs exist** in:
  - `code-rs/core/src/chat_completions.rs` (reasoning_content streaming for GLM)
  - `code-rs/core/src/client.rs` (forced streaming mode)
  - `code-rs/core/src/model_provider_info.rs` (Z.AI base URL)
- **Stray files to remove**:
  - `code-rs/core/src/chat_completions.rs.bak2`
  - `code-rs/core/src/client.rs.bak3`
  - `code-rs/core/src/client.rs.bak4`
  - `code-rs/core/src/client.rs.bak5`
  - `code-rs/core/src/client.rs.bak6`
  - `code-rs/core/src/model_provider_info.rs.bak`
  - `zai/zai-streaming.md`
  - `zai/zai-tool-streaming.md`
