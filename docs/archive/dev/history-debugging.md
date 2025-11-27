# History Debugging Toolkit

The history view now exposes a trace channel that makes it easier to reason
about ordering bugs and stranded running cells. The traces are gated behind the
`CODEX_TRACE_HISTORY` environment variable so regular sessions stay silent.

## Enabling Traces

```bash
export CODEX_TRACE_HISTORY=1
```

With the flag set the TUI emits structured `trace!` logs on the
`codex_history` target and also buffers them in–process for tests.

## Manual Reproduction Flow

1. **Create a worktree** – run `/branch` or perform any action that yields the
   “Creating worktree…” / “Created worktree.” system banners. Confirm they are
   adjacent.
2. **Start a long running tool** – trigger a custom tool (for example an
   MCP-backed command that waits on I/O). Leave it running.
3. **Restore history** – use the existing “restore snapshot” action (or in dev
   builds, `/history restore latest`). This exercises the rehydration path.
4. **Let the tool complete** – allow the tool to finish or send its end event.
5. **Inspect logs** – look for:
   - `restore_history_snapshot.start/done` pair with cell/order counts.
   - `rehydrate_tool_state.*` entries detailing each running tool.
   - `custom_tool_end.*` / `mcp_tool_end.*` / `web_search_end.*` entries showing
     whether the completion was in-place or fallback.
   - `system_order_cache.reset` confirming the banner cache was rebuilt.
   You should not see spinners left over; the banners should stay contiguous.

## Programmatic Checks

In tests (or a debug REPL) you can read the buffered trace lines via
`ChatWidget::history_debug_events()`. This is how the new regression tests
verify that logging is wired correctly.

## Observing State

If you need a snapshot of caches at runtime, evaluate the helper methods:

- `ChatWidget::assert_history_consistent()` (in tests) validates matched
  lengths between `history_cells`, `cell_order_seq`, and `system_cell_by_id`.
- The trace messages include the counts of custom/web/wait/kill maps after
  each rehydrate or finalize phase.

These hooks should give enough signal to detect regressions without adding
noise to day-to-day sessions.

