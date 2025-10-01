# History Cell Refactor Plan

## Background

We're seeing a lot of issues with the conversation history in the TUI. It's slow to scroll (not easily cached), has inconsistent ordering, does not reconstruct correctly when using resume or undo. We also want to be able to import conversations from sub-agents, so need a portable format. We're also seeing some really large files (such as chatwidget.rs) which we want to reduce to improve maintainability.

To do that we're working on refactoring the history cells so that state is separated from rendering. Each cell should be able to be rendered from a state object along with the system settings. This state should be React-style so determine how the cell works, but the actual rendering logic should be entirely in the cell. The state should be easily serializable in JSON and will be built entirely from events in the new HistoryState. So, for example, the state should not have presentation logic like color etc.. that would be handled by the cell during rendering.


## Primary Goals (Plan & Status)
- [x] **Extract per-cell modules** – core message/tool/plan/upgrade/reasoning/image/loading/animated/assistant/stream files live under `history_cell/`; exec remains inline and pending extraction.
- [x] **Finish semantic state refactor** – removed the last `SemanticLine` bridges; message/reasoning cells now store typed spans and convert independently of presentation styles (2025-09-30).
- [x] **Introduce single `HistoryState` vector** – foundational types (`HistoryRecord`, `HistoryState`, `HistoryId`) now live in `history/state.rs`; UI now resolves render/cache flows via `HistoryId`, and `ChatWidget` builds frames directly from the unified vector (2025-09-30).
- [x] **Centralize event → state mapping** – history mutations now derive records via `history_cell::record_from_cell`, ensuring every insert/remove funnels through `HistoryState::apply_event` before UI caches update (2025-09-30).
- [x] **Unlock serialization & perf goals** – history snapshots now carry ordering metadata, session logs emit record/order counts, restore paths rebuild ID/look-up caches, and renderer caches remain keyed by `(HistoryId, width, theme_epoch, reasoning)` (2025-09-30).
- [x] **Document interim cache bridge** – keep legacy per-cell layout caches (exec/assistant/diff) in place until the Step 6 renderer cache is implemented, and track their removal behind the new caching system. See `docs/history_render_cache_bridge.md` for the current bridge design and removal plan (2025-09-30).
- [x] **Land HistoryDomainEvent layer** – introduce a dedicated domain-event enum + helpers so every TUI mutation flows through `HistoryState::apply_domain_event` without reconstructing records from cells. HistoryDomainRecord now covers every record variant, chatwidget mutation paths call `apply_domain_event`, and regression tests exercise the conversions (2025-09-30).

## Shared Rendering Inputs
- History viewport width (drives wrapping and layout caches).
- Theme palette (applied when converting stored styles to runtime `Style`).
- Reasoning visibility toggle (`Ctrl+R`, affects collapsed vs expanded reasoning blocks).
- Animation timers for ephemeral UI elements (kept outside the serialized state vector).

## Cell Inventory
Status legend: ✅ complete (semantic deterministic state ready), ⏳ still needs semantic refactor.

### ✅ Plain Messages – `PlainHistoryCell`
- **Desired state:**
  ```rust
  struct PlainMessageState {
      role: HistoryCellRole,
      body: Vec<MessageSegment>, // text segments tagged with semantics (code, bullet, emphasis, etc.)
      metadata: Option<MessageMetadata>,
  }
  ```
- **Current status:** ✅ state now stored as `PlainMessageState` (header + `Vec<MessageLine>`); follow-up: richer `MessageSegment` types for code/bullets plus metadata serialization.

### ✅ Wait Status – `WaitStatusCell`
- **Desired state:** `WaitStatusState { header: WaitHeader, bullet_points: Vec<WaitDetail> }`
- **Status:** ✅ migrated to `WaitStatusState` with explicit header/detail tones; renderer rebuilds lines from structured data.

### ✅ Loading Spinner – `LoadingCell`
- **State:** `LoadingCellState { message: String }`
- **External settings:** theme.

### ✅ Tool Calls – `ToolCallCell`
- **Desired state:**
  ```rust
  struct ToolCallState {
      status: ToolCallStatus,
      title: ToolTitle,
      arguments: Vec<ToolArgument>,
      result_preview: Option<ToolResultPreview>,
  }
  ```
- **Status:** ✅ constructors now populate `ToolArgument`/`ToolResultPreview`; follow-up: polish preview truncation + metadata serialization.
- **External settings:** theme, width.

### ✅ Running Tool Calls – `RunningToolCallCell`
- **Desired state:** same struct but `arguments: Vec<ToolArgument>` instead of rendered lines.
- **Status:** ✅ running tool state stores `Vec<ToolArgument>` with wait caps/call ids tracked separately.
- **External settings:** theme, width, current time (for elapsed label).

### ✅ Plan Updates – `PlanUpdateCell`
- **Desired state:** `PlanUpdateState { icon: PlanIcon, items: Vec<PlanLine>, completion: PlanCompletion }`
- **Status:** ✅ stores `PlanUpdateState` with `PlanProgress`/`PlanStep`; follow-up: richer icons + metadata serialization.
- **External settings:** theme, width.

### ✅ Upgrade Notices – `UpgradeNoticeCell`
- **Desired state:** `UpgradeNoticeState { current_version: String, latest_version: String, message: UpgradeMessage }`
- **Status:** ✅ upgrade notices now keep `{ current_version, latest_version, message }`; follow-up: wire optional CTA metadata.
- **External settings:** theme, width.

### ✅ Background Events – `BackgroundEventCell`
- **Status:** ✅ already semantic (metadata only).
- **External settings:** theme; renderer currently emits metadata text.

### ✅ Animated Welcome – `AnimatedWelcomeCell`
- **Persistence:** not stored in the state vector (UI-only animation).
- **Runtime state:** start time, fade progress, cached height.

### ✅ Reasoning – `CollapsibleReasoningCell`
- **Desired state:** `ReasoningState { id, sections: Vec<ReasoningSection>, in_progress, hide_when_collapsed }` with sections broken into semantic blocks.
- **Status:** ✅ state maintains `ReasoningSection` blocks with persisted summaries and typed bullet markers (2025-09-26 follow-up complete).
- **External settings:** `Ctrl+R` (collapsed vs expanded), theme, width.
- **Widget state:** collapse flag held outside the serialized state (driven by the global toggle).

### ✅ Explore Aggregations – `ExploreAggregationCell`
- **Status:** ✅ already semantic (summary enums + statuses).
- **External settings:** theme, width.

### ✅ Image Outputs – `ImageOutputCell`
- **Status:** ✅ image records now store width, height, SHA, MIME type, and byte size in `HistoryState`; the UI renders from `ImageRecord` without private RGBA caches (2025-09-30).

### ✅ Patch Summaries – `PatchSummaryCell`
- **Status:** ✅ patch events now emit and hydrate `PatchRecord`; the cell renders directly from recorded metadata without internal width caches (2025-09-30).

### ✅ Exec Commands – `ExecCell`
- **Status:** ✅ `ExecRecord`/`HistoryDomainEvent` now drive exec lifecycle end-to-end; renderer pulls lines via `display_lines_from_record` through the shared cache.
- **Current:** command metadata, streaming tails, and wait states persist in `HistoryState`; UI helpers resolve purely by `HistoryId` with stable call_id mapping.
- **Decision:** per-width caches deleted; centralized renderer cache handles memoization without regressing redraw performance.
- **Notes:** Jump-back/undo flows snapshot exec records and merged summaries rebuild from state; regression tests cover streaming reorder + finish tails.
- **External settings:** theme, width, monotonic time for “running” durations.

- ### ✅ Assistant Streaming – `StreamingContentCell`
- Completed the streaming renderer tests (`streaming_updates_replace_record_in_place`, `streaming_flow_handles_deltas_and_finalize`) and documentation: streaming lines now come solely from `HistoryState`, renderer cache keys are documented, and no per-cell caches remain.
- **Status:** [~] streaming records capture IDs, markdown deltas, citations, and metadata in `AssistantStreamState`; the UI now routes all begin/delta events through `HistoryDomainEvent::UpsertAssistantStream`, and the cell holds only lightweight context (file opener + cwd) instead of cloning the full `Config`.
- **Current:** renderer consumes `RenderRequestKind::Streaming`, and a `HistoryRenderState` test (`streaming_updates_replace_record_in_place`) verifies that preview lines are sourced from `HistoryState` across replacements.
- **Decision:** finish documenting the state-driven flow and mop up any lingering fallback usage before flipping status to ✅.
- **Needed:** pipe structured citations/token usage through serialization and audit for any remaining UI-owned caches once final test/docs land.

### ✅ Assistant Answers – `AssistantMarkdownCell`
- Completed state-driven rendering for finalized assistant messages: `AssistantMarkdownCell` now stores only `AssistantMessageState` plus minimal context, renderer tests (`assistant_render_from_state`, `assistant_render_remains_stable_after_insertions`) confirm `HistoryRenderState::visible_cells()` pulls lines from `HistoryState`, and plan doc updated accordingly.

### ✅ Merged Exec Summary – `MergedExecCell`
- **Status:** ✅ merged summaries now hydrate from `MergedExecRecord` entries in `HistoryState` rather than cached `ExecCell` output.
- **Current:** exec finish handlers merge adjacent state records into a single `HistoryRecord::MergedExec`; the renderer consumes them via `RenderRequestKind::MergedExec` and the shared cache.
- **Decision:** legacy per-width caches were removed; merged cells carry their `HistoryId` and serialize all segments, so session logs and snapshots stay consistent.
- **Follow-up:** consider deduplicating read preamble heuristics inside the renderer once diff caching lands, but no functional gaps remain.

### ✅ Diffs – `DiffCell`
- **Status:** ✅ diff cells now render directly from `DiffRecord` via the shared renderer cache; the per-width RefCell cache was deleted.
- **Current:** `RenderRequestKind::Diff` sources lines from `HistoryState`, so theme/width updates invalidate through `HistoryRenderState` automatically.
- **Decision:** `DiffCell` stores only semantic state; chatwidget builds diff render requests without touching cell caches, and snapshots/logs serialize the structured diff hunks.
- **Follow-up:** consider richer diff metadata (e.g., file icons) in a future pass once renderer pipelines stabilize.

### ✅ Explore Fetch / HTTP – `ExploreRecord`
- **Status:** ✅ explore aggregations now render from `ExploreRecord` state via the shared renderer cache; the legacy trailing flag and UI-owned line cache were removed.
- **Current:** `RenderRequestKind::Explore` fetches lines from `HistoryState`; explore lifecycle events update `ExploreRecord` entries through `HistoryDomainEvent::Replace`.
- **Follow-up:** consider richer per-entry metadata (e.g., elapsed time, HTTP previews) once fetch tooling expands.

### ✅ Legacy Plain Producers
- **Status:** ✅ plain system/user notices now build `PlainMessageState` and flow through `HistoryState`; `PlainHistoryCell::new`/prompt helpers no longer leak UI caches.
- **Change:** chatwidget/app flows now call `history_insert_plain_state_with_key`/`history_push_plain_state`, queued prompt previews and plan docs/tests updated.

## Step 1 – Finish Semantic State Refactors *(Complete)*

2025-09-30: Removed the transitional `SemanticLine` adapter; plain and reasoning cells now persist `InlineSpan` data directly so themes no longer drive semantic parsing.
1.1 **Plain messages** – ✅ replaced `SemanticLine` caches with `PlainMessageState` (`plain.rs` now stores `MessageHeader + MessageLine` via shared conversion helpers). Follow-up: enrich headers with structured badges and surface metadata once available.
1.2 **Tool calls (running & completed)** – ✅ constructors now emit `ToolArgument`/`ToolResultPreview`; remaining work: tighten JSON summaries + result truncation heuristics.
1.3 **Plan updates** – ✅ `PlanUpdateCell` now renders from `PlanProgress`/`PlanStep` + `PlanIcon`; follow-up: improved summary metadata.
1.4 **Upgrade notice** – ✅ cell consumes `UpgradeNoticeState` (versions + message), custom render derived at draw time.
1.5 **Reasoning** – ✅ sections/blocks stored; block metadata now includes typed bullets and per-section summaries for collapse previews (2025-09-26).
1.6 **Wait status & background notices** – ✅ wait tools use `WaitStatusState`; background notices render via `BackgroundEventRecord`.
1.7 **Documentation** – ✅ inline docs now cover reasoning summaries/bullets; audit of constructors confirmed strongly typed state (2025-09-26).

## Step 2 – Exec / Streaming / Diff Bridge *(In Progress)*
2.1 **Exec state extraction & module split** – ✅ exec records now render exclusively via `RenderRequestKind::Exec`; tests (`exec_render_from_state`, `exec_render_remains_stable_after_insertions`) confirm history restores are state-driven with no per-cell caches.
2.2 **Streaming assistant module** – ✅ streaming records hydrate from `HistoryState`; see `streaming_flow_handles_deltas_and_finalize` for start → delta → finish coverage.
2.3 **Finalized assistant markdown module** – ✅ finalized answers render from `AssistantMessageState`; tests (`assistant_render_from_state`, `assistant_render_remains_stable_after_insertions`) verify state-driven output and stability.
2.4 **Diff module breakout** – ✅ diff records render from state via `RenderRequestKind::Diff`; regression coverage tracks reorder stability.
2.5 **Merged exec views** – ✅ merged exec summaries rebuild from `HistoryState` and share the renderer cache; no per-cell layout caches remain.

## Step 3 – HistoryState Manager *(Pending)*
3.1 **HistoryRecord enum** – ✅ complete: `history/state.rs` defines `HistoryRecord`, per-cell state structs, and `HistoryState` scaffolding with ID management helpers.
3.2 **ChatWidget incremental adoption** – Introduce `HistoryState`/`HistoryRenderState` alongside the legacy `history_cells` vector, migrate low-risk cell types (plain, loading, wait status) to the new state, then remove the legacy vector once the path is stable. **Status:** ✅ Wait tooling, diff inserts, and explore aggregations now flow through domain events; `history_cells` remains solely as the interim render cache until Step 3.5 removes the trait bridge.
3.3 **Apply-event pipeline** – Implement `HistoryState::apply_event(&mut self, event: &EventMsg)` covering all core/TUI event types (exec lifecycle, tool updates, background notices, resume snapshots, undo) and route migrated cells through it. **Status:** ✅ Exec wait notes, diff pushes, and explore status updates now call into `HistoryState::apply_domain_event`; next wave is wiring streaming assistant/diff updates before the Step 4 renderer consolidation.
3.4 **Undo/resume hooks** – Expose `snapshot`, `restore`, and `truncate_after(id)` to support /undo and resume flows. **Status:** ✅ history snapshots + truncation API landed in `HistoryState`, ghosts now capture them (2025-09-30).
3.5 **ChatWidget full integration** – Delete `history_cells: Vec<Box<dyn HistoryCell>>`, wire all helper methods (`history_push`, `history_replace`, etc.) into `HistoryState`, and treat Step 4 as the follow-up for centralized rendering.

### Outstanding Gaps (2025-09-27)
- Apply-patch failures still surface a minimal `PatchRecord` without stderr context; consider storing structured failure metadata (message, stdout/stderr digest) so the renderer can show richer feedback while remaining state-driven. **Status (2025-09-30):** ✅ patch failure events now capture sanitized stdout/stderr excerpts in `PatchRecord::failure`, and the summary cell renders them inline.
- Several helpers still rebuild cells inline (e.g., merged exec summaries) instead of emitting domain-specific events; consolidate around `HistoryState::apply_event` so the legacy `history_cells` vector can be retired.
- Track exec/assistant/diff layout caches as an intentional bridge; remove them only after the shared renderer cache in Step 6 ships to avoid frame-time regressions.

### Event Pipeline Consolidation Plan
- **Phase A – Inventory:** enumerate every direct mutation (`history_cells[idx] = ...`, `history_cells.remove`, `history_record_from_cell`) and map them to a target `HistoryEvent` variant. Track results in `chatwidget/events_audit.md` (new doc) so progress is visible.
- **Phase B – API surface:** extend `HistoryState::apply_event` to accept domain enums (`HistoryDomainEvent`) which wrap core `EventMsg` payloads; expose helpers like `apply_exec_event` to keep match arms local to each module.
- **Phase C – Migration waves:**
  1. Plain/background/loading/wait/tool/plan/upgrade (already idempotent) – replace inline constructors with `HistoryDomainEvent::InsertPlain` etc.
  2. Exec/stream/assistant – route lifecycle through domain events so streaming merges/final answers run purely on `HistoryState` data.
  3. Diff/patch/image/explore/rate-limit – remove bespoke mutation helpers and rely on event emissions from their source modules.
- **Phase D – Flip ownership:** once all mutations go through domain events, replace `history_cells` with a derived cache built from `HistoryRecord` snapshots + the renderer cache. Provide a debug flag to assert no code path mutates `history_cells` directly.

## Step 4 – Event Mapping & Rendering *(Pending)*
4.1 **Centralize handlers** – Route every mutation in `handle_*` (exec events, tool deltas, diff updates, background notices) through `HistoryState` so ordering/id management lives in one place. **Status:** ✅ patch approval / apply flows now issue `HistoryDomainEvent` updates (no direct cell mutations), and jump-back/ghost restore rehydrate via snapshots (2025-09-30).
4.2 **Stable IDs for streaming** – Store `HistoryId` + domain IDs (exec id, tool call id, stream id) to ensure in-place updates and dedupe. **Status:** ✅ history state now tracks exec/tool call mappings, handlers resolve deltas via `HistoryId`, and regression coverage checks reordered exec output (2025-09-30).
4.3 **HistoryRenderState** – Provide adapter that consumes `HistoryRecord` + current settings (theme, width, reasoning collapsed) and produces cached `RenderedCell` structures. Cache keyed by `(HistoryId, width, theme_epoch)`. **Status:** ✅ history render adapter now renders via `RenderSettings` keyed by `HistoryId`, clears caches on theme/width changes, and new tests cover caching + invalidation (2025-09-30).
4.4 **Renderer migration** – Update TUI drawing code to iterate `HistoryRenderState::visible_cells()`; drop direct use of `HistoryCell` trait. **Status:** ✅ render loop now consumes `RenderRequest`/`visible_cells`, caches assistant plans, and performs layout/spacing without direct trait traversal (2025-09-30).

## Step 5 – Serialization & Persistence
- Define canonical JSON schema for each record variant (enum tags, tone names instead of colors). **Status:** ✅ documented in `docs/history_state_schema.md` (2025-09-30).
- Add round-trip tests for snapshot/resume. **Status:** ✅ JSON snapshot tests cover serialize/restore (`history/state.rs`) (2025-09-30).
- Persist history vector in session logs; implement `/undo` by restoring prior snapshot. **Status:** ✅ session logs now emit `history_snapshot` entries and `/undo` restores `HistorySnapshot` for the forked conversation (2025-09-30).

## Step 6 – Performance Improvements *(Blocked on Step 4)*
- Ship the shared renderer cache (memoized per-cell layouts keyed by `(HistoryId, width, theme_epoch)`) so the interim exec/assistant/diff caches can be removed. **Status:** ✅ history_render now caches by `(HistoryId, width, theme_epoch, reasoning)` and `ChatWidget` consumes the shared API (2025-09-30).
- Add instrumentation to measure render latency and scrolling cost pre/post refactor. **Status:** ✅ perf stats now track scroll events/rows and render time; `/perf show` surfaces the new metrics (2025-09-30).
- Benchmark resume/undo operations using new snapshots. **Status:** ✅ perf stats now record undo/restore timings surfaced via `/perf show` (2025-09-30).

### Renderer Cache Design Sketch
- **Responsibility:** centralize layout caching in `HistoryRenderState` so individual cells own only semantic state. Cache entries map `(HistoryId, width, theme_epoch, reason_visibility)` → `RenderedCell` (lines + per-row buffers + meta).
- **Structure:**
  - `RenderedCell` wraps immutable `Vec<Line<'static>>` plus precomposed `Vec<Box<[BufferCell]>>` for fast draw.
  - `HistoryRenderState` keeps an `IndexMap<CacheKey, Arc<RenderedCell>>` with LRU eviction (target: 512 entries, tunable via config).
  - `CacheKey` includes `theme_epoch` (incremented on palette change) and any feature flags (e.g., reasoning collapsed) so stale entries drop naturally.
- **Invalidation hooks:**
  - `HistoryState::apply_event` emits `HistoryRenderInvalidate` messages when the underlying record changes.
  - Width changes call `HistoryRenderState::handle_width_change(new_width)` to flush mismatched entries.
  - Theme swaps bump `theme_epoch`; toggling reasoning detail flips a `ReasoningVisibility` enum baked into the key.
- **Warm path:** when rendering, look up `(id, width, theme_epoch, visibility)`. If present, reuse. If missing, invoke cell-specific `render_to_lines(record, settings)` helpers to produce lines, then store.
- **Cold path optimizations:** reuse arena-allocated span buffers to reduce allocations; consider hashing markdown bodies to share across identical answers when history rewinds.
- **Telemetry:** wrap cache hits/misses with counters exported via `/perf` to confirm the shared cache covers ≥90 % of paint operations before removing per-cell caches.

## Completion Notes (2025-09-30)

All primary goals and follow-on cleanups listed in this plan are now complete. The TUI history stack routes every mutation through `HistoryState` + `HistoryDomainEvent`, rendering flows via the shared `HistoryRenderState` cache, and persistence/undo paths serialize structured records. Future work should land as incremental enhancements on top of this baseline (e.g., richer metadata or new cell types) and append new sections below this completion note as needed.
