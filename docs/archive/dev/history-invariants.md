# History & Tool-State Invariants

This document records the assumptions the TUI relies on when rendering
history cells, managing running tools, and replaying snapshots. Whenever we
touch the chat history or tool bookkeeping, we should keep these invariant
checks in mind.

## Running Tool Maps

- The `running_custom_tools`, `running_web_search`, `running_wait_tools`, and
  `running_kill_tools` maps must always reflect a visible running cell in
  `history_cells`. After a snapshot restore or any rebuild, we must rehydrate
  them from `history_state` before handling new events.
- Rehydrate helpers may synthesize lookup entries, but they must not invent
  new history rows—only map call IDs to existing running cells.

## Finalization Rules

- When finalizing (during TaskComplete or an explicit end event), only clear a
  running map entry if we actually replaced/collapsed the associated running
  cell. Entries that could not be resolved must remain so the next pass can
  retry.
- End events are idempotent. Receiving the same `*ToolCallEnd` twice may
  re-run finalization, but it must not create duplicates or resurrect a
  spinner.
- Spinner rows should always be collapsed: after finalization there should be
  at most one completed tool row per call ID.

## Ordering

- Preserve the `OrderKey` whenever a running row is replaced in place. If a
  fallback insertion is needed, reuse the incoming order key and remove the
  stale running row first.
- System banners (“Creating worktree…”, “Created worktree.”, etc.) should stay
  contiguous even across restores and tool completions. If we need to recompute
  caches, do so atomically and keep the order vector in sync.
- After any rebuild or rehydrate, we must ensure `history_cells.len() ==
  cell_order_seq.len() == history_cell_ids.len()`, and every stored system
  index must remain in bounds.

## Fallback Heuristics

- Prefer matching running cells by `HistoryId` first (when available), or by
  `call_id`. If neither yields a match, scan for a generic running tool cell as
  a last resort before inserting a replacement.
- Any fallback insertion must inherit the original order and remove the
  spinner row so we do not present two entries for the same call.

Keeping these invariants explicit makes it easier to reason about history
refactors and reduces the risk of reintroducing ordering bugs.

