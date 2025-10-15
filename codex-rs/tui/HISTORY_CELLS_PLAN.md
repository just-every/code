# History Cell Refactor Plan

## Background

We're seeing a lot of issues with the conversation history in the TUI. It's slow to scroll (not easily cached), has inconsistent ordering, does not reconstruct correctly when using resume or undo. We also want to be able to import conversations from sub-agents, so need a portable format. We're also seeing some really large files (such as chatwidget.rs) which we want to reduce to improve maintainability.

To do that we're working on refactoring the history cells so that state is separated from rendering. Each cell should be able to be rendered from a state object along with the system settings. This state should be React-style so determine how the cell works, but the actual rendering logic should be entirely in the cell. The state should be easily serializable in JSON and will be built entirely from events in the new HistoryState. So, for example, the state should not have presentation logic like color etc.. that would be handled by the cell during rendering.


## Primary Goals (Plan & Status)
- [x] **Extract per-cell modules** – core message/tool/plan/upgrade/reasoning/image/loading/animated files live under `history_cell/`; exec/diff/streaming still inline and pending extraction.
- [ ] **Finish semantic state refactor** – continue replacing ad-hoc `SemanticLine` usage with rich typed structs (`MessageSegment`, `ToolArgument`, etc.) so renderers never infer structure from strings.
- [~] **Introduce single `HistoryState` vector** – foundational types (`HistoryRecord`, `HistoryState`, `HistoryId`) now live in `history/state.rs`; still need to adopt them in `chatwidget.rs` and event handling.
- [ ] **Centralize event → state mapping** – funnel all `handle_*` mutations through `HistoryState::apply_event` and a dedicated render adapter.
- [ ] **Unlock serialization & perf goals** – once the state vector exists, wire serialization, resume snapshots, undo rewind, and cached layout per `(record_id, width)`.

## Shared Rendering Inputs
- History viewport width (drives wrapping and layout caches).
- Theme palette (applied when converting stored styles to runtime `Style`).
- Reasoning visibility toggle (`Ctrl+R`, affects collapsed vs expanded reasoning blocks).
- Animation timers for ephemeral UI elements (kept outside the serialized state vector).

## Cell Inventory
Status legend: ✅ complete (semantic deterministic state ready), ⏳ still needs semantic refactor.

- **Desired state:**
  ```rust
  struct PlainMessageState {
      role: HistoryCellRole,
      body: Vec<MessageSegment>, // text segments tagged with semantics (code, bullet, emphasis, etc.)
      metadata: Option<MessageMetadata>,
  }
  ```
- **Current status:** ✅ state now stored as `PlainMessageState` (header + `Vec<MessageLine>`); follow-up: richer `MessageSegment` types for code/bullets plus metadata serialization.

- **Desired state:** `WaitStatusState { header: WaitHeader, bullet_points: Vec<WaitDetail> }`
- **Status:** ✅ migrated to `WaitStatusState` with explicit header/detail tones; renderer rebuilds lines from structured data.

### ✅ Loading Spinner – `LoadingCell`
- **State:** `LoadingCellState { message: String }`
- **External settings:** theme.

- **Desired state:**
  ```rust
  struct ToolCallState {
      status: ToolCallStatus,
      title: ToolTitle,
      arguments: Vec<ToolArgument>,
      result_preview: Option<ToolResultPreview>,
  }
  ```
- **Status:** ✅ arguments/results now captured as semantic lines; TODO: evolve into explicit `ToolArgument`/`ToolResultPreview` structs.
- **External settings:** theme, width.

- **Desired state:** same struct but `arguments: Vec<ToolArgument>` instead of rendered lines.
- **Status:** ✅ running tool arguments emitted as semantic lines; TODO: convert to structured argument types.
- **External settings:** theme, width, current time (for elapsed label).

- **Desired state:** `PlanUpdateState { icon: PlanIcon, items: Vec<PlanLine>, completion: PlanCompletion }`
- **Status:** ✅ semantic lines in place; TODO: expand to structured plan items.
- **External settings:** theme, width.

- **Desired state:** snapshot rate-limit metrics and legend entries as structured data (percentages, reset times, etc.).
- **Status:** ⏳ currently storing rendered lines.
- **External settings:** theme, width.

- **Desired state:** `UpgradeNoticeState { current_version: String, latest_version: String, message: UpgradeMessage }`
- **Status:** ✅ semantic lines in place; TODO: replace with version/message struct.
- **External settings:** theme, width.

- **Status:** ✅ already semantic (metadata only).
- **External settings:** theme; renderer currently emits metadata text.

### ✅ Animated Welcome – `AnimatedWelcomeCell`
- **Persistence:** not stored in the state vector (UI-only animation).
- **Runtime state:** start time, fade progress, cached height.

- **Desired state:** `ReasoningState { id, sections: Vec<ReasoningSection>, in_progress, hide_when_collapsed }` with sections broken into semantic blocks.
- **Status:** ✅ semantic lines in place; ⏳ still need structured sections.
- **External settings:** `Ctrl+R` (collapsed vs expanded), theme, width.
- **Widget state:** collapse flag held outside the serialized state (driven by the global toggle).

- **Status:** ✅ already semantic (summary enums + statuses).
- **External settings:** theme, width.

- **Status:** ⏳ verify each uses structured data; replace any remaining rendered-line storage with semantic fields.

- **Status:** remains ⏳; in addition to layout caches, output/wait info must become serializable metadata (no rendered lines).
- **Current:** mixes command metadata with cached layout and mutable wait state.
- **Needed:** extract `ExecCellState { command, parsed, output, stream_preview, wait_snapshot, metadata, status }` with only serializable fields; move layout caches to renderer.
- **External settings:** theme, width, monotonic time for “running” durations.

- **Status:** ⏳ convert to semantic buffer of markdown delta segments.
- **Current:** stores rendered lines plus layout cache.
- **Needed:** persist stream id + raw markdown chunks; rebuild styled lines during render.

- **Status:** ⏳ store raw markdown + formatting options only.
- **Current:** retains raw markdown but also caches wrapped lines.
- **Needed:** keep raw markdown and formatting options only; rebuild on demand.

- **Status:** ⏳ reuse semantic exec state fragments.
- **Current:** holds a vec of rendered preamble/output line pairs.
- **Needed:** reuse the new `ExecCellState` fragments instead of storing rendered lines.

- **Status:** ⏳ store diff hunks, headers, metadata.
- **Current:** persists styled diff lines.
- **Needed:** store structured diff hunks + metadata; renderer applies styling.

- **Status:** ⏳ store URL, status, body.
- **Current:** cached display lines.
- **Needed:** serialize fetched URL, status, and body; render into lines at runtime.

- **Status:** ⏳ ensure producers emit semantic data before handing to cells.
- **Current:** some paths still inject raw `Vec<Line<'static>>` via `PlainHistoryCell` constructors.
- **Needed:** convert producers to build `RichTextLine` or structured records before handing off.

## Step 1 – Finish Semantic State Refactors *(In Progress)*
1.1 **Plain messages** – ✅ replaced `SemanticLine` caches with `PlainMessageState` (`plain.rs` now stores `MessageHeader + MessageLine` via shared conversion helpers). Follow-up: enrich headers with structured badges and surface metadata once available.
1.2 **Tool calls (running & completed)** – Introduce `ToolArgument`, `ArgumentValue`, and `ToolResultPreview`; ensure constructors populate structured args/results instead of emitting prefixed strings. Track wait caps and timestamps explicitly.
1.3 **Plan updates** – Model progress via `PlanProgress { completed, total }` and `Vec<PlanStep>`; remove reliance on unicode progress bars inside strings (rendered in UI only). Persist chosen `PlanIcon` variant instead of string literal.
1.4 **Upgrade notice** – Store `{ current_version, latest_version, message }` plus optional CTA metadata; renderers assemble styled lines at draw time.
1.5 **Reasoning** – Build `ReasoningSection/ReasoningBlock` hierarchy (headings, paragraphs, bullets, code). Collapse behavior becomes a pure view concern driven by global `Ctrl+R` toggle.
1.6 **Wait status & background notices** – ✅ wait tool output now records `WaitStatusState { header, details }`; background notices still pending.
1.7 **Documentation** – Update module docs/tests to reflect new structs; ensure all constructors return strongly typed states.

## Step 2 – Exec / Streaming / Diff Family *(Pending)*
2.1 **Exec state extraction & module split** – Create `ExecRecord { command, parsed, action, stdout_chunks, stderr_chunks, exit_code, wait_notes }`, move exec rendering/state into `history_cell/exec.rs`, and drop layout caches from the shared module.
2.2 **Streaming assistant module** – Capture raw markdown deltas + metadata in `AssistantStreamState`, relocate the streaming renderer to `history_cell/stream.rs`, and support merge/upsert by stream id. Renderer handles ellipsis and wrapping.
2.3 **Finalized assistant markdown module** – Use `AssistantMessageState` storing markdown + citations + token metadata. Move finalized assistant rendering into the streaming module or a dedicated `assistant.rs` while ensuring layout rebuild happens on demand.
2.4 **Diff module breakout** – Persist diff hunks (`DiffHunk`, `DiffLine`) and patch metadata, move diff rendering into `history_cell/diff.rs`, and have the renderer apply styling based on line kind.
2.5 **Merged exec views** – Rebuild aggregated exec cells from `Vec<ExecRecord>` snapshots rather than cached text blocks once the exec module exists.

## Step 3 – HistoryState Manager *(Pending)*
3.1 **HistoryRecord enum** – ✅ complete: `history/state.rs` defines `HistoryRecord`, per-cell state structs, and `HistoryState` scaffolding with ID management helpers.
3.2 **ChatWidget incremental adoption** – Introduce `HistoryState`/`HistoryRenderState` alongside the legacy `history_cells` vector, migrate low-risk cell types (plain, loading, wait status) to the new state, then remove the legacy vector once the path is stable.
3.3 **Apply-event pipeline** – Implement `HistoryState::apply_event(&mut self, event: &EventMsg)` covering all core/TUI event types (exec lifecycle, tool updates, background notices, resume snapshots, undo) and route migrated cells through it.
3.4 **Undo/resume hooks** – Expose `snapshot`, `restore`, and `truncate_after(id)` to support /undo and resume flows.
3.5 **ChatWidget full integration** – Delete `history_cells: Vec<Box<dyn HistoryCell>>`, wire all helper methods (`history_push`, `history_replace`, etc.) into `HistoryState`, and treat Step 4 as the follow-up for centralized rendering.

## Step 4 – Event Mapping & Rendering *(Pending)*
4.1 **Centralize handlers** – Route every mutation in `handle_*` (exec events, tool deltas, diff updates, background notices) through `HistoryState` so ordering/id management lives in one place.
4.2 **Stable IDs for streaming** – Store `HistoryId` + domain IDs (exec id, tool call id, stream id) to ensure in-place updates and dedupe.
4.3 **HistoryRenderState** – Provide adapter that consumes `HistoryRecord` + current settings (theme, width, reasoning collapsed) and produces cached `RenderedCell` structures. Cache keyed by `(HistoryId, width, theme_epoch)`.
4.4 **Renderer migration** – Update TUI drawing code to iterate `HistoryRenderState::visible_cells()`; drop direct use of `HistoryCell` trait.

## Step 5 – Serialization & Persistence *(Blocked on Step 3)*
- Define canonical JSON schema for each record variant (enum tags, tone names instead of colors).
- Add round-trip tests for snapshot/resume.
- Persist history vector in session logs; implement `/undo` by restoring prior snapshot.

## Step 6 – Performance Improvements *(Blocked on Step 4)*
- Memoize per-cell layouts keyed by `(HistoryId, width, theme_epoch)`.
- Add instrumentation to measure render latency and scrolling cost pre/post refactor.
- Benchmark resume/undo operations using new snapshots.
