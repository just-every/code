# PRD: Add Search Command To Find Text In Conversation History

**SPEC-ID**: SPEC-KIT-067
**Status**: Draft
**Created**: 2025-10-20
**Author**: Multi-agent consensus (gemini, claude, code)

---

## Problem Statement

**Current State**: The Codex TUI renders ongoing multi-agent conversations inside `codex-rs/tui/src/chatwidget/`, but there is no in-product search. Users must manually scroll through hundreds of messages to rediscover earlier context, decisions, or error output.

**Pain Points**:
- Manual scrolling wastes 5–10 minutes whenever users need to revisit prior discussion.
- Debugging requires re-running commands because earlier stack traces are hard to relocate.
- Multi-agent collaboration suffers because users cannot confirm what a specific agent said without combing the log.
- Long sessions (300+ messages) cause context loss and repeated questions during retries (Implement → Validate loops).

**Impact**: Lack of search erodes productivity, increases cognitive load, and undercuts expectations of parity with standard terminal tooling (e.g., `/` in less or `grep`). Adding fast, intuitive search unlocks quicker validation, better collaboration, and higher confidence in automated workflows.

---

## Target Users & Use Cases

### Primary User: Automation-Oriented Developer

**Profile**: Developers orchestrating Spec-Kit automation inside the TUI for multi-agent implementation, validation, and audit stages.

**Current Workflow**: Run `/speckit.*` commands, review streaming agent output, and manually scroll when context is needed.

**Pain Points**: Re-finding earlier agent output, comparing historic errors, and verifying who agreed to what consumes time and interrupts flow.

**Desired Outcome**: Query the conversation log in-place, jump directly to matches, and continue execution without manual searching.

### Secondary User: Reviewer or Operator

**Profile**: Stakeholders auditing Spec-Kit evidence or reviewing automation runs (quality gate reviewers, SREs on-call) inside the TUI.

**Use Case**: Validate that prior remediation steps occurred, confirm consensus summaries, or locate compliance-related snippets mid-run.

---

## Goals

### Primary Goals

1. **Enable rapid text lookup inside the TUI**  
   **Success Metric**: p95 search latency <100 ms for 500-message histories.

2. **Deliver intuitive keyboard-first UX**  
   **Success Metric**: ≥70% of long (100+ message) sessions include at least one search within 30 days of launch.

3. **Support exact and case-insensitive matching without regressions**  
   **Success Metric**: Zero open regression bugs and zero panics after 500 automated test runs.

### Secondary Goals

1. **Provide context-aware result navigation** (e.g., next/previous, contextual snippets).
2. **Allow optional filters (agent/role) and configurable context lines.**

---

## Non-Goals

**Explicitly Out of Scope**:
- Cross-session or archived log search (future enhancement).
- Semantic or embedding-based search.
- Exporting matches to external files.
- Full-text indexing infrastructure; MVP relies on in-memory scan.

**Rationale**: Focus on delivering an in-session productivity boost without introducing heavy dependencies or premature complexity.

---

## Scope & Assumptions

**In Scope**:
- New `/search` (alias `/history.search`) command parsed by Spec-Kit command registry.
- Linear search (with lightweight optimisations) over in-memory conversation history, including streaming assistant output.
- Highlighting and navigation inside the chat history view.
- Telemetry for usage, latency, cancellation, and empty-result events.

**Assumptions**:
- Conversation messages are stored in order and addressable by stable indices via `ChatWidget`.
- Tokio runtime is available for spawning cancellable background search tasks.
- Terminal width ≥40 columns; highlight styles can fall back gracefully.

**Constraints**:
- Must not block UI rendering or agent pipelines.
- Additional memory footprint must remain <1 MB per active search.
- Adhere to evidence and telemetry policies (`docs/spec-kit/evidence-policy.md`).

---

## Functional Requirements

| ID | Requirement | Acceptance Criteria | Priority |
|----|-------------|---------------------|----------|
| FR1 | Provide `/search <query>` (alias `/history.search`) slash command | Command is registered, parsed, and routes to search handler | P0 |
| FR2 | Default to case-insensitive substring search | "error" matches "Error" and "ERROR" in all roles | P0 |
| FR3 | Support case-sensitive flag (`-s` / `--case-sensitive`) | Flag restricts matches to exact case; documented in `/help search` | P0 |
| FR4 | Support whole-word option (`-w` / `--word`) | Finds Unicode-aware word boundaries; toggled independently of case | P1 |
| FR5 | Filter by agent (`--agent claude,gpt_pro`) and/or role (`--role user|assistant|system|agent`) | Results limited to requested sources; invalid values produce friendly error | P1 |
| FR6 | Present paginated results showing message index, agent, timestamp, and highlighted snippet | Default page size 20; navigation keys cycle matches | P0 |
| FR7 | Allow keyboard navigation (`n` next, `p` previous, `Enter` jump, `q`/`Esc` exit) | Search mode status line reflects match `i/N`; viewport scrolls to current match | P0 |
| FR8 | Include streaming messages in search results | Partial assistant output is searchable; limitations documented | P1 |
| FR9 | Handle empty query or empty history gracefully | `/search` with no query or empty log returns usage guidance without panic | P0 |
| FR10 | Provide `/search --help` usage with examples | Usage includes flags, alias, and examples; accessible via command palette | P0 |
| FR11 | Emit telemetry events (`search_started`, `search_completed`, `search_canceled`, `search_no_results`) | Events recorded via evidence pipeline with latency metrics | P0 |
| FR12 | Persist last search state for quick repeat (`/search` reopens previous query) | Optional MVP enhancement; at minimum maintain state during active search | P2 |

---

## Non-Functional Requirements

| ID | Requirement | Target Metric | Validation Method |
|----|-------------|---------------|-------------------|
| NFR1 | Performance | p95 latency <100 ms for 500 messages; p99 <150 ms | Benchmark in CI against synthetic histories |
| NFR2 | UI responsiveness | No visible frame drops; input latency unaffected | Manual TUI profiling + integration tests |
| NFR3 | Memory usage | Additional allocations <1 MB per search; no leaks | Heap profiling + sustained search tests |
| NFR4 | Reliability | Zero panics across 10k fuzz/property test iterations | Property-based tests with proptest |
| NFR5 | Accessibility | Keyboard-only navigation; highlight meets contrast guidelines | UX review in light/dark themes |
| NFR6 | Observability | Telemetry fields: `duration_ms`, `match_count`, `flags`, `canceled` | Evidence artefacts in consensus + telemetry dirs |
| NFR7 | Compatibility | Works with existing pagination/history caching; no regressions | Regression suite (`/speckit.validate`) |

---

## User Experience

**Interaction Flow**:
1. User triggers `Ctrl+F` or types `/search failure`.
2. Status line shows “Searching…”.
3. Results panel lists matches with `[1/5] Message 142 (assistant, gemini)` style metadata and ±3 message context.
4. User presses `n`/`p` to cycle; viewport auto-scrolls and highlights match.
5. `Enter` jumps focus to the conversation; `q` or `Esc` exits search mode.

**Keyboard Shortcuts**:
- `/search <query>`: Initiate search.
- `Ctrl+F`: Shortcut to pre-fill `/search `.
- `n` / `p`: Navigate results in search mode.
- `Enter`: Jump to highlighted match in history.
- `q` / `Esc`: Exit search mode.
- `Ctrl+C`: Cancel active search task (falls back to normal mode).

**Visual Feedback**:
- Active match uses inverse or high-contrast highlight; other matches dimmed.
- Context lines styled with secondary text colour.
- “No matches for ‘query’” message with suggestions (toggle case, adjust filters).
- Spinner or subtle progress indicator for histories exceeding 1000 messages.

**Error States**:
- Empty query → “Search query cannot be empty. Usage: `/search <query>`”.
- Invalid flag combination → descriptive error without exiting command mode.
- Search timeout (>500 ms) → warning banner suggesting refined query; results still shown if available.

---

## Dependencies

**Technical**:
- `codex-rs/tui/src/chatwidget/command_registry.rs`: register new Search command.
- `codex-rs/tui/src/chatwidget/history_render.rs`: extend rendering to highlight matches.
- `codex-rs/tui/src/chatwidget/mod.rs`: maintain `SearchState` and navigation handlers.
- Optional utility module `history_search.rs` for reusable search logic (unit-tested).

**Organisational**:
- Coordination with maintainers for UX/UI review.
- QA sign-off on accessibility and regression testing.

**Data**:
- Relies solely on in-memory conversation buffers; no external data sources introduced.

---

## Risks & Mitigations

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|------------|
| Large histories cause perceptible lag | Medium | Medium | Chunked async scan with cooperative yields; cache lower-cased message text |
| Keyboard conflicts with existing shortcuts | High | Low | Introduce explicit search mode; audit current keymap |
| Unicode edge cases (emoji, CJK) mis-match | Medium | Medium | Use Unicode-aware case-folding, add property tests |
| Regex or advanced filtering expands scope prematurely | Medium | Low | Gate behind future flag; document non-goal |
| Streaming messages mutate mid-search | Low | Medium | Capture snapshot at search start; optionally diff new messages and merge |
| Evidence footprint growth | Low | Low | Follow evidence policy (compress after unlock, cap per-SPEC footprint) |

---

## Success Metrics

- ≥30% of sessions with >100 messages trigger the search command within 30 days.
- ≥1.5 result navigation keystrokes per search (indicates multi-match utility).
- <5% of searches end in cancellation due to performance issues.
- Qualitative feedback from internal dogfooding rates UX ≥4/5.

---

## Validation Plan

### Testing Strategy

1. **Unit Tests**: Search parser, flag combinations, Unicode word boundaries, filter logic.
2. **Integration Tests**: Simulated histories (empty, single match, many matches), navigation flow, streaming updates, cancel behaviour.
3. **E2E Tests**: Snapshot tests exercising TUI rendering and key events via integration harness.
4. **Performance Benchmarks**: Measure latency across 500/1000/5000 message histories as ignored benchmarks.
5. **Property Tests**: Fuzz random queries, unicode inputs, and message permutations to ensure no panics.

### Review Process

1. **PRD Review**: Spec-Kit maintainers and UX owner.
2. **Design Review**: Focus on history data access and highlight rendering.
3. **Code Review**: Peer review with mandatory test evidence and benchmark output.
4. **Security Review**: Ensure no command injection surfaces; confirm search input is escaped.

---

## Multi-Agent Consensus

- **Agents Run**: gemini, claude, code (Tier 2).
- **Agreements**:
  - Need for in-TUI search with case-insensitive default and case-sensitive toggle.
  - Emphasis on keyboard navigation, contextual snippets, and telemetry coverage.
  - Performance targets around sub-100 ms latency for typical histories.
- **Conflicts**: None detected. All agents aligned on MVP scope and phased enhancements (regex, search history) as future work.

---

## Evidence & Telemetry

- PRD artefact stored at `docs/SPEC-KIT-067-add-search-command-to-find-text-in-conversation-history/PRD.md`.
- Telemetry events to be persisted under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-067/`.
- Consensus summary to be stored in local-memory MCP tagged `spec:SPEC-KIT-067`, `stage:specify`.
- Benchmarks and validation artefacts to follow `docs/spec-kit/evidence-policy.md` retention rules (25 MB soft limit per SPEC).

---

## Open Questions

1. **Exact match semantics**: Should `--word` be part of MVP or deferred? (Recommended: include in MVP for clarity.)
2. **Default scope**: Should system/tool messages be included? (Recommended: include user + assistant + agent; allow opt-in for system via `--role`.)
3. **Result panel layout**: Side panel vs. inline overlay—requires UX prototype validation.
4. **Search repetition**: Should `/search` with no args repeat last query automatically? (Possible Phase 2 enhancement.)

---

## Changelog

- **2025-10-20**: Initial PRD drafted via multi-agent consensus; consensus confirmed with no conflicts.
