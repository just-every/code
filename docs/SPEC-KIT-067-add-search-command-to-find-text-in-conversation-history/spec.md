**SPEC-ID**: SPEC-KIT-067
**Feature**: Add search command to find text in conversation history
**Status**: Backlog
**Created**: 2025-10-20
**Branch**: (pending)
**Owner**: Code

**Context**: The Codex TUI currently lacks an in-session search capability, forcing users to scroll through long conversation histories to rediscover decisions, agent outputs, or error logs. This SPEC anchors delivery of the `/search` command defined in `docs/SPEC-KIT-067-add-search-command-to-find-text-in-conversation-history/PRD.md`.

---

## User Scenarios

### P1: Locate critical error details fast

**Story**: As a developer running `/speckit.implement`, I want to search for a specific error string so that I can compare new failures with previous agent output without re-running commands.

**Priority Rationale**: Directly unblocks debugging loops during Implement → Validate cycles.

**Testability**: Integration test that `/search timeout` highlights prior assistant message and jumps viewport on `Enter`.

**Acceptance Scenarios**:
- Given a 300-message conversation containing "timeout" in an assistant message, when I run `/search timeout`, then the results list includes that message with highlighted context.
- Given the search results panel, when I press `Enter` on a match, then the history view scrolls to that message and highlights the match.
- Given I provide `/search` with no query, then the TUI returns a usage error without altering history state.

### P2: Filter by agent ownership

**Story**: As a reviewer auditing consensus, I want to search only the `gpt_pro` aggregation outputs so that I can confirm consensus summaries quickly.

**Priority Rationale**: High-value for compliance and evidence review but secondary to core debugging flow.

**Testability**: `/search --agent gpt_pro summary` returns only aggregator messages; tested via synthetic log fixtures.

**Acceptance Scenarios**:
- Given a conversation with gemini and gpt_pro outputs, when I run `/search --agent gpt_pro summary`, then only gpt_pro messages appear in results.
- Given an invalid agent filter, when I run `/search --agent unknown foo`, then I receive a descriptive error and the search is aborted.

### P3: Navigate across many matches

**Story**: As an operator reviewing long sessions, I want to step through multiple matches using the keyboard so that I can audit context efficiently.

**Priority Rationale**: Improves usability for high-volume sessions; tertiary relative to P1/P2.

**Testability**: Snapshot-driven TUI test verifying `n`/`p` navigation updates the highlighted match and status label (e.g., "Match 2/7").

**Acceptance Scenarios**:
- Given multiple matches, when I press `n`, then focus advances to the next match and the status label updates accordingly.
- Given I press `q` or `Esc` in search mode, then the TUI exits search mode and removes match highlights.

---

## Edge Cases

- Empty conversation buffer should emit "No messages to search" without triggering errors.
- Unicode queries (emoji, CJK) must match case-insensitively when possible and never panic on invalid folding.
- Long single messages (>10 kB) should render truncated snippets with ellipses without breaking layout.
- Concurrent searches should cancel the previous search task within 50 ms to avoid race conditions.
- Streaming assistant messages should either include partial text in search results or clearly document any limitation.
- Terminal sessions without colour support should fall back to bold/underline highlight styles.

---

## Requirements

### Functional Requirements

- **FR1**: Register `/search` and `/history.search` commands via the Spec-Kit command registry, parsing query and option flags.
- **FR2**: Execute searches asynchronously over in-memory message data, respecting case sensitivity and agent/role filters.
- **FR3**: Render a results panel showing match index, agent, timestamp, and snippet with highlighted matches.
- **FR4**: Provide keyboard navigation (`n`, `p`, `Enter`, `q`/`Esc`) and auto-scroll behaviour for highlighted matches.
- **FR5**: Emit telemetry events for search lifecycle stages, capturing duration, match count, filters, and cancellation state.

### Non-Functional Requirements

- **Performance**: Maintain p95 latency <100 ms for 500 messages and p99 <150 ms; benchmark inside CI.
- **Reliability**: Zero panics or crashes across 10k fuzz/property test iterations; recover gracefully from invalid input.
- **Memory**: Keep incremental memory usage under 1 MB per active search and release allocations immediately on exit.
- **Observability**: Store telemetry under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-067/` and local-memory entries tagged `spec:SPEC-KIT-067`.
- **Compatibility**: Ensure no regressions to existing history rendering, pagination, or key bindings.

---

## Success Criteria

- `/search` adopted in ≥30% of sessions exceeding 100 messages within 30 days post-launch.
- No open regression bugs or performance alerts attributed to the feature after two weekly release cycles.
- Positive qualitative feedback (≥4/5) from internal dogfooding survey on usability and responsiveness.

---

## Evidence & Validation

**Acceptance Tests**: Add integration tests covering query parsing, filter enforcement, navigation, and empty-result states.

**Telemetry Path**: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-067/` for command telemetry and benchmarks.

**Consensus Evidence**: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-067/` to capture agent outputs and synthesis.

**Validation Commands**:
```bash
/speckit.plan SPEC-KIT-067
/speckit.tasks SPEC-KIT-067
/speckit.implement SPEC-KIT-067

# Full automation when ready
/speckit.auto SPEC-KIT-067

# Observe status
/speckit.status SPEC-KIT-067
```

---

## Clarifications

### 2025-10-20 - Initial Spec Creation

**Clarification needed**: UX layout decision (side panel vs. inline overlay) for results presentation.

**Resolution**: Pending UX review; prototype both options before implementation.

**Updated sections**: To be revised after UX decision (User Experience, Acceptance Criteria).

---

## Dependencies

- `codex-rs/tui/src/chatwidget/commands/` for new command module (`search.rs`).
- `codex-rs/tui/src/chatwidget/history_render.rs` for highlighting and viewport control.
- `codex-rs/tui/src/chatwidget/mod.rs` for search state management and keyboard event handling.

---

## Notes

- Coordinate with evidence policy maintainers to ensure new telemetry files stay within the 25 MB per-SPEC soft cap.
- Phase-two enhancements (regex, search history recall, cross-session search) are intentionally deferred until MVP adoption data is collected.
