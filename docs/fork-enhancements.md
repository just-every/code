# Fork Enhancements (Initial, Not Exhaustive)

This fork extends upstream `openai/codex` in several areas. These bullets are a starting point — not a complete list. Before merging, scan the codebase and history (CHANGELOG.md, recent commits) to discover additional or newer fork‑only behavior and preserve it.

- Browser Integration (codex-rs/browser, TUI + core wiring)
  - Internal CDP browser manager with global access and /browser command.
  - Tool family: `browser_*` (open, status, click, move, type, key, javascript, scroll, history, inspect, console, cleanup, cdp).
  - Screenshot capture with segmentation, cursor overlay, asset storage, and per‑turn injection/queueing; TUI rendering with friendly titles.
  - External Chrome attach via `/chrome` and headless profile management.

- Multi‑Agent Orchestration (core/agent_tool.rs, TUI panel)
  - Tool family: `agent_*` (run, check, result, cancel, wait, list) with persisted outputs under `.code/agents/<id>` and TUI agent panel.
  - Batch and per‑agent status updates; file emission (result/error/status logs).

- Tooling Extensions and Policy Integration
  - `web_fetch` custom tool with markdown‑aware TUI rendering and UA override.
  - `view_image` tool to attach local images.
  - Local shell + sandbox policy with escalation request (`with_escalated_permissions`, `justification`) and WorkspaceWrite flags (network access, allow_git_writes, tmpdir controls).
  - Streamable exec tool support kept off by default; preserved compatibility with classic shell tool.

- Protocol/Model Compatibility Tweaks
  - Responses/Chat Completions parity for tools; MCP tool schema sanitization to a safe subset.
  - FunctionCallOutput serialization kept as a plain string for Responses API compatibility.
  - Web search event mapping and `WebSearchCall` rendering in TUI; optional upstream web_search tool gated by policy.

- TUI Enhancements
  - Strict streaming ordering invariants (request_ordinal, output_index, sequence_number) and delta rendering.
  - Markdown renderer improvements and code-block snapshots; theme selection view; bottom pane chrome selection.
  - History cells with richer tool-specific titles and previews (browser_*, web_fetch, agents).
  - Fully state-driven history refactor: `HistoryState` + `HistoryDomainEvent`, shared renderer cache, and serialized snapshots. See `docs/tui-chatwidget-refactor.md`, `docs/history_state_schema.md`, and `docs/history_render_cache_bridge.md`.

- Version and User‑Agent Handling
  - Consistent `codex_version::version()` usage; `get_codex_user_agent(_default)` helper; MCP server/client UA tests.

- Build/CI/Workflow
  - `build-fast.sh` portability (exec bin autodetect, deterministic link tweaks) and zero‑warning policy.
  - Upstream merge guards and policy to prefer our core files; static verify checks (handlers↔tools parity, UA/version) to prevent regressions.

Note: These are representative areas. You must still scan for newer fork‑only behavior (e.g., additional TUI UX, new tools, rollout metrics, protocol fields) and preserve them when resolving conflicts.
