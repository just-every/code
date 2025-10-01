# History Render Cache Bridge

This document captures the current bridge between the new `HistoryState` data
model and the legacy per-cell layout caches that still live inside a few
history cells (`ExecCell`, streaming/assistant cells, and diff cells). The
bridge allows `ChatWidget` to render the unified history vector without
re-computing expensive layouts every frame while we complete the Step 6 renderer
work.

## Rendering pipeline overview

1. The draw loop in `ChatWidget::render_history` assembles a list of
   `RenderRequest` values. Each request contains the `HistoryId` of the record
   being rendered, the existing `HistoryCell` cache if one exists, and a set of
   "fallback" lines derived on demand from the semantic `HistoryRecord`.
2. `HistoryRenderState::visible_cells()` receives those requests along with the
   current `RenderSettings` (width, theme epoch, and reasoning visibility
   toggle). The render state stores cached layouts in a `HashMap<CacheKey,
   CachedLayout>`, where `CacheKey` is `(HistoryId, width, theme_epoch,
   reasoning_visible)`.
3. When a cache entry exists, `visible_cells()` returns the layout immediately.
   Otherwise it invokes the builder closure supplied in the request. The
   closure uses the `HistoryCell` if one exists, and falls back to the semantic
   lines so the renderer can still make forward progress even when we no longer
   materialize a trait object.
4. The resulting `VisibleCell` bundles the resolved layout (plus the assistant
   streaming plan, if applicable). `ChatWidget` uses these `VisibleCell`
   snapshots to copy pre-rendered buffer rows into the terminal buffer.

## Cache invalidation

`HistoryRenderState` exposes several targeted invalidation helpers:

- `invalidate_history_id(HistoryId)` removes cached layouts for a single
  record. We call this whenever a record’s semantic state changes (for example
  when streaming assistant output arrives).
- `handle_width_change(width)` prunes cache entries whose width no longer
  matches the viewport. Width changes also clear prefix sums so we rebuild the
  height map.
- `invalidate_all()` clears the layout cache and prefix sums. We use this when
  restoring snapshots or performing operations that massively reshuffle
  history.

The prefix sum cache (`prefix_sums`, `last_prefix_*` fields) is separate from
the layout cache. It stores cumulative heights so that scrolling computations
are O(1) per frame.

## Fallback behaviors

- `RenderRequest::fallback_lines` are derived from the semantic
  `HistoryRecord`. This ensures that even if a legacy `HistoryCell` cache is
  missing (for example, after deserializing a snapshot from disk) we still have
  content to render.
- `ChatWidget::draw_history` keeps the old `HistoryCell` objects around only as
  an optimization; any cache miss rebuilds the layout from semantic state and
  stores the result in the shared cache.
- Cells that still own per-width caches (`ExecCell`, diff/assistant streaming)
  remain responsible for invalidating their internal caches. The history render
  cache treats those cells as opaque; once Step 6 lands we can remove the
  per-cell caches and rely exclusively on `HistoryRenderState`.

## Removal roadmap

The interim bridge exists solely to prevent regressions while we finish Step 6
of the plan. Once all cells render from structured state and the shared cache
proves stable, we can:

1. Delete the per-cell layout caches (`ExecCell::cached_layout`, assistant
   streaming wrappers, diff caches).
2. Convert all render paths to supply semantic state only, removing
   `HistoryCell` trait objects from the steady-state draw loop.
3. Simplify `ChatWidget::draw_history` so it only iterates over
   `HistoryState.records`.

Tracking this work in the plan keeps the team aligned on why the bridge exists
and what remains before it can be removed.
