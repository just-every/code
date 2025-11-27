
# TUI ChatWidget Refactor Plan

This document tracks the refactor of `code-rs/tui/src/chatwidget.rs` into smaller, maintainable modules and state bundles. It also captures what has been completed, and what remains so another engineer can continue seamlessly.

## Goals

- Break the 389kB `chatwidget.rs` into cohesive modules by responsibility.
- Group related state into small structs to reduce field sprawl and accidental misuse.
- Centralize common history operations (push/replace/remove) behind a tiny API.
- Keep behavior identical and builds green at every step (`./build-fast.sh`).
- Maintain zero warnings policy.

---

## Status Summary (as of this commit)

Completed extractions (modules):
- `chatwidget/perf.rs`: `PerfStats` (data + helpers)
- `chatwidget/diff_ui.rs`: `DiffOverlay`, `DiffBlock`, `DiffConfirm`
- `chatwidget/message.rs`: `UserMessage`, `create_initial_user_message`
- `chatwidget/streaming.rs`: streaming deltas and finalize helpers (`on_commit_tick`, `is_write_cycle_active`, `handle_streaming_delta`, `finalize_active_stream`)
- `chatwidget/exec_tools.rs`: exec lifecycle, tool merge helpers; move-before-assistant helpers
- `chatwidget/tools.rs`: Web Search and MCP tool lifecycle
- `chatwidget/layout_scroll.rs`: HUD toggles, layout_areas, PageUp/PageDown, mouse wheel scroll
- `chatwidget/diff_handlers.rs`: all Diff overlay key handling

State groupings:
- `StreamState` on `ChatWidget`
  - `current_kind`, `closed_answer_ids`, `closed_reasoning_ids`, `next_seq`, `seq_answer_final`, `drop_streaming`
- `LayoutState` on `ChatWidget`
  - `scroll_offset`, `last_max_scroll`, `last_history_viewport_height`, `vertical_scrollbar_state`, `scrollbar_visible_until`, `last_hud_present`, `browser_hud_expanded`, `agents_hud_expanded`, `last_frame_height`
- `DiffsState` on `ChatWidget`
  - `session_patch_sets`, `baseline_file_contents`, `overlay`, `confirm`, `body_visible_rows`
- `PerfState` on `ChatWidget`
  - `enabled`, `stats: RefCell<PerfStats>` replacing `perf_enabled` and `perf`

History helper shim:
- Added to `ChatWidget`:
  - `history_push(cell)`: wrapper for `add_to_history`
  
Additional references:
- Renderer cache bridge rationale: [`history_render_cache_bridge.md`](history_render_cache_bridge.md)
- History record schema + snapshot details: [`history_state_schema.md`](history_state_schema.md)
- Migrated usages in `exec_tools.rs` and `tools.rs` (some hotspots) to use the shim.

Build + policy:
- All steps validated with `./build-fast.sh`.
- Zero compiler warnings kept intact.

---

## New File/Module Map

- `tui/src/chatwidget.rs` (shrinking coordinator)
  - owns `ChatWidget` and wires events → submodules
  - houses grouped states: `StreamState`, `LayoutState`, `DiffsState`, `PerfState`
  - history helper shim (temporary; see Future consolidation)
- `tui/src/chatwidget/perf.rs` → performance structs
- `tui/src/chatwidget/diff_ui.rs` → diff overlay data types
- `tui/src/chatwidget/message.rs` → user message building
- `tui/src/chatwidget/streaming.rs` → unified streaming operations
- `tui/src/chatwidget/exec_tools.rs` → exec lifecycle + merges
- `tui/src/chatwidget/tools.rs` → Web Search + MCP lifecycle
- `tui/src/chatwidget/layout_scroll.rs` → layout + scrolling + HUD controls
- `tui/src/chatwidget/diff_handlers.rs` → diff overlay key handling

---

## Behavior-Preserving Changes Made

- No intentional UX/logic changes. Extracted code calls the same operations as before.
- Replaced direct field access with grouped state access (e.g., `self.stream_state.current_kind`).
- Centralized common history mutations and used them in a few hotspots to reduce duplication.

---

## Next Steps (Recommended Order)

1) Finish History helper adoption — DONE
- Replaced remaining ad-hoc `history_cells[idx] = …`, `remove(i)`, and `add_to_history` sites with `history_replace_at`, `history_remove_at`, `history_push` (kept direct remove+insert only for move-before-assistant helpers where the removed cell is reused).
- Swept: `chatwidget.rs`, `chatwidget/tools.rs`, `chatwidget/exec_tools.rs`, `chatwidget/interrupts.rs`.

2) History merge API consolidation — DONE
- Added `history_push_and_maybe_merge`, `history_replace_and_maybe_merge`, and `history_maybe_merge_tool_with_previous` on `ChatWidget`.
- Adopted across exec completion and web search completion paths; explicit merge code removed from call sites.

3) Enum-ize exec actions and newtype IDs — DONE
- Added `ExecAction` enum + `action_enum_from_parsed()` and adopted across `history_cell`, `exec_tools`, and `chatwidget`.
- Introduced ID newtypes: `ExecCallId`, `ToolCallId`, `StreamId`; migrated state maps/sets and call sites.
- Removed legacy `action_from_parsed` helper.

4) Streaming API cleanup — DONE
- In `streaming.rs`, added `begin(kind,id)`, `delta_text(kind,id,text)`, `finalize(kind, follow_bottom)` facades.
- Replaced direct `current_kind` mutations at call sites with the facade; kept internal control inside streaming module.

5) Tests (valuable quick wins)
- Unit tests
  - Exec merges: combinations for Read/Search/List/Run and exit statuses.
  - Parse/paste: `[image: …]`, `[file: …]`, direct path handling, size thresholds, non‑UTF8 case.
  - Diff selection math: selecting block by offset and generating undo/explain prompts.
- Replay tests (vt100)
  - Cancel mid-stream (drop deltas immediately, then finalize when backend completes).
  - “Final answer closes lingering tools/execs” (already coded; test it). 

6) Optional: rename for clarity
- Consider `ChatWidget` → `ChatView` (render/controller) once responsibilities are clearer.

---

## Tips for the Next Engineer

- Always validate with `./build-fast.sh`; warnings are treated as failures.
- Avoid touching any code related to `CODEX_SANDBOX_*` env vars.
- Keep steps small and behavior‑preserving; prefer many tiny commits over one big one.
- When moving code, first copy the function into a submodule, compile, then replace the original with a delegate call. Only then remove the original body.
- For scrolling math: all offsets are measured from the bottom; `LayoutState` centralizes the values you need.
- For diffs popup: `DiffsState` now owns overlay/confirm and the session patch context.
- For perf: read/write via `self.perf_state.enabled` and `self.perf_state.stats.borrow_mut()`.

---

## Quick Pointers

- Stream state: `StreamState` on `ChatWidget` (search for `stream_state`)
- Layout state: `LayoutState` on `ChatWidget` (search for `layout.`)
- Diff state: `DiffsState` on `ChatWidget` (search for `diffs.`)
- History helpers: `history_push`, `history_replace_at`, `history_remove_at` in `chatwidget.rs`
- Web search lifecycle: `chatwidget/tools.rs`
- Exec lifecycle + merges: `chatwidget/exec_tools.rs`
- Streaming functions: `chatwidget/streaming.rs`
- Diff overlay keys: `chatwidget/diff_handlers.rs`
- HUD/layout: `chatwidget/layout_scroll.rs`

---

## Done Checklist

- [x] Extract perf/diff/message data types
- [x] Extract streaming and exec/tool handlers
- [x] Extract layout/scroll + diff key handling
- [x] Group StreamState, LayoutState, DiffsState, PerfState
- [x] Add history helper shim and adopt in hotspots
- [x] Keep builds green with zero warnings at each step

## Todo Checklist

- [x] Sweep remaining history mutations to use the shim
- [x] Consolidate history merge API (adopt helpers across call sites)
- [x] Replace all action string checks with `ExecAction`
- [x] Add ID newtypes (Exec/Tool/Stream) and migrate maps
- [x] Tighten streaming API (begin/delta/finalize facade)
- [x] Add state-driven renderer/unit tests for exec, assistant, streaming, diff

---

## How to Validate Locally

```bash
./build-fast.sh
```

Expect: success with zero warnings. The script builds the Rust workspace in a fast profile and maintains symlinks required by the CLI wrapper.

---

If anything is unclear or you hit conflicts, start by grepping for the state bundle or module name and follow the delegate calls from `ChatWidget` into the submodule.
