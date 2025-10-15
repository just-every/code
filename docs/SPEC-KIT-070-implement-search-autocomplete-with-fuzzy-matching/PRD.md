# PRD: SPEC-KIT-070 — Search Autocomplete with Fuzzy Matching (Codex TUI)

## Overview
- Purpose: Deliver fast, fuzzy-matched autocomplete across Codex TUI surfaces so builders can recall commands, SPEC IDs, documentation, and files with partial inputs.
- Scope: Extend existing TUI search providers, composer autocomplete, and global palette to unify results, tolerate typos, and surface telemetry for iteration.
- Alignment: Upholds the Code Spec-Kit constitution (evidence-driven, SPEC.md authoritative) and product requirements v1.0 with multi-agent automation guardrails.
- Surfaces in scope:
  - Global search palette (Ctrl+K) aggregating commands, SPECs, docs, files.
  - Chat composer autocomplete for slash commands, SPEC IDs, docs, and files.
  - SPEC jump and quick navigation affordances (e.g., `SPEC-KIT-070`).

## Problem Statement
- Users rely on exact substring search across disjoint popups; typos or fuzzy recall frequently miss results.
- No unified list combines commands, SPEC IDs, docs, and files, generating friction when switching contexts mid-flow.
- Sparse telemetry prevents understanding whether search assists or frustrates users, limiting evidence-driven iteration.

## Goals
- Provide fuzzy-matched autocomplete with stable, keyboard-first navigation across all supported domains.
- Surface useful suggestions under 120 ms p95 (≤50 ms warm) while highlighting matched spans for clarity.
- Emit structured telemetry (schema v1) for every autocomplete lifecycle event to power evidence and regression detection.
- Respect guardrails: SPEC.md stays authoritative; solutions integrate with existing hooks, telemetry, and evidence capture.

## Non-Goals
- Building cross-repository federated search or indexing external knowledge sources.
- Replacing the underlying file search engine; we reuse and extend current streaming search.
- Designing web or GUI clients; focus remains on Codex TUI/CLI and MCP integrations.

## User Stories
- As a developer, pressing Ctrl+K and typing “stat” shows “/spec-status SPEC-KIT-060” in top suggestions even with partial input.
- As a developer, typing “/imp 070” in the composer suggests “/speckit.implement SPEC-KIT-070” despite missing characters.
- As a developer, typing “docs fuzzy” lists relevant SPEC docs even when “fuzzy” is misspelled.
- As a developer, entering “fil sarch.rs” still selects `file_search.rs` with highlighted similarity indices.
- As an engineering lead, I review telemetry to confirm ≥90 % of searches end in a selection from the top three suggestions.

## Functional Requirements
- **Data domains**
  - Commands: built-ins, `/speckit.*`, legacy aliases, and installed agent commands with descriptions.
  - SPEC IDs: parsed from `SPEC.md` tasks table plus directory metadata in `docs/SPEC-*/` (include titles where available).
  - Files: streamed via existing `codex_file_search::run_streaming`, capped per debounce tick.
  - Docs: filenames plus optional `doc-index` MCP headings when enabled; degrade gracefully when disabled.
- **Matching & scoring**
  - Reuse or extend `codex_common::fuzzy_match` and evaluate alternatives (`nucleo`, `fuzzy-matcher`); support Levenshtein-style errors up to distance 2.
  - Blend fuzzy score, recency, per-domain priors (commands > SPEC > docs > files by default), and exact token bonuses.
  - Highlight matched indices using shared selection-popup utilities; ensure case-insensitive matching and Unicode safety.
- **UX behaviour**
  - Global palette groups results by domain with keyboard navigation (↑/↓/PgUp/PgDn, Tab to switch groups, Enter to commit, Esc to exit).
  - Composer autocomplete appears for `/`, `SPEC-KIT-`, `@`, and `./` prefixes; updates stream without jumping focus.
  - Provide empty-state guidance (“Keep typing…”, “No results—try broader terms”) and preserve selection when lists refresh.
  - Actions: executing commands runs them (or prefills interactive prompts), SPEC selections open respective docs, file selections open viewer panes.
- **Configuration & guardrails**
  - Config flag to toggle unified autocomplete (default on) plus advanced tuning for domain weights and maximum per-domain results.
  - Respect `.gitignore`, hide secrets or private paths, and avoid mutating SPEC.md or doc artifacts during search.

## Non-Functional Requirements
- Performance: keystroke-to-first-result ≤120 ms p95 (≤50 ms warm cache), ≤500 ms p99; support streaming partial batches under 200 ms.
- Reliability: cancel stale searches on new keystrokes, isolate failures per domain, and fall back to substring filtering when fuzzy engine errors.
- Accessibility: ensure high-contrast colour palette, add underline to highlights, maintain focus order, and expose descriptive labels for screen-reader tooling.
- Robustness: use worker threads for scoring, pre-build caches on startup, and invalidate caches on file watcher events or SPEC updates.

## UX Specification
- Global palette layout with header input, grouped body, and footer key hints (“Enter select • Tab switch group • Esc close”).
- Composer popup limited to eight visible results with “searching…” placeholder; preserve cursor context and support PageUp/PageDn when list exceeds viewport.
- SPEC jump badges show SPEC title (if available) plus status indicator from `SPEC.md` (Backlog/In Progress/etc.).

## Telemetry & Evidence (Schema v1 compliant)
- Common fields: `command`, `specId`, `sessionId`, `timestamp`, `schemaVersion`, `artifacts[]`.
- New events:
  - `search.start` { surface: palette|composer, query_len, domains_requested }
  - `search.partial_result` { surface, latency_ms, counts: {cmd,spec,doc,file} }
  - `search.final_result` { surface, latency_ms, top_scores[], total_results }
  - `search.select` { surface, item_type, item_identifier, rank, score, query }
  - `search.cancel` { surface, reason: superseded|escape|blur }
  - `search.error` { surface, error_code, message }
- Evidence storage:
  - Capture telemetry artifacts under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-070/search/` when executed within a SPEC context; otherwise bucket by session date.
  - Enable `SPEC_OPS_TELEMETRY_HAL=1` to attach `hal.summary` latency snapshots (healthy and degraded) and store them alongside telemetry.

## Success Metrics
- ≥95 % of intent queries (including typos) show the correct item within the top three results during dogfooding corpus tests.
- ≥90 % of autocomplete sessions end in a selection from the returned list; ≤5 % revert to manual substring search.
- Search latency meets ≤120 ms p95 / ≤500 ms p99 budgets across representative repo sizes.
- Telemetry capture rate ≥95 % with zero schema validation errors during HAL smoke runs.

## Architecture & Integration Notes
- Extend TUI modules (`codex-rs/tui/src/bottom_pane/command_popup.rs`, `file_search_popup.rs`, `chat_composer.rs`, `selection_popup_common.rs`, `app.rs`, `app_event.rs`) to share a unified provider and highlight utilities.
- Introduce provider traits for commands, SPECs, docs, and files; consider background index manager for incremental updates on file-system or SPEC changes.
- `codex-cli` should expose updated registries for commands and SPEC metadata (e.g., via MCP or JSON cache) so TUI can synchronise quickly.
- Provide integration points for optional doc-index MPC server; degrade gracefully if unavailable.

## Acceptance Criteria
- Typos like “speckit statu” surface “/speckit.status” with highlight cues.
- Entering “070” or “spec kit 70” offers “SPEC-KIT-070…” with metadata badge.
- Highlight styling combines colour + underline for colour-blind accessibility and remains stable while results refresh.
- Keyboard controls (↑/↓/Tab/Enter/Esc/PageUp/PageDn) operate as described and maintain focus when lists update.
- Telemetry events recorded for start, partial, final, select, cancel, and error cases; artifacts stored in evidence directory with schema v1 compliance (validated via hooks).
- Graceful degradation: when doc-index disabled or fuzzy engine fails, system falls back to substring search without crashing.

## Risks & Mitigations
- **Performance regressions** on large workspaces → Use debounce, background scoring threads, per-domain caps, and warm caches.
- **Overly noisy suggestions** from aggressive fuzzy thresholds → Start with conservative scores, expose config, run usability feedback loops.
- **Telemetry volume** from keystrokes → Batch events, sample low-signal surfaces, and ensure HAL summaries remain lightweight.
- **Accessibility regressions** → Pair highlights with underline, run manual screen-reader pass, add unit tests for focus order.

## Open Questions
- Should we surface recent actions or history as a separate domain within the palette?
- What default domain weights best match expert workflow vs. newcomer workflow?
- Do we expose the telemetry dashboards directly within `/spec-status` or rely on external analytics?
- How do we backfill SPEC titles when directories lack explicit metadata files?

## Milestones (indicative)
- **M1 – Discovery & Benchmarks (2d):** Evaluate fuzzy libraries, define scoring strategy, draft telemetry schema updates.
- **M2 – Commands & SPECs (3d):** Implement provider traits, integrate fuzzy matching, add SPEC parser, validate latency budgets.
- **M3 – Files & Docs (3d):** Wire streaming file search adapter, integrate doc-index fallback, implement highlight rendering.
- **M4 – Accessibility & UX polish (2d):** Complete keyboard handling, contrast tweaks, tooltip/footer hints, composer behaviour stabilization.
- **M5 – Evidence & Hardening (2d):** Capture HAL summaries, finalize telemetry wiring, document configuration toggles, prepare changelog imagery.

## Multi-Agent Consensus Summary
- **Gemini (model_release 2025-08, reasoning_mode research):** Emphasized benchmarking multiple fuzzy algorithms, telemetry depth (score distributions), and privacy-aware sampling.
- **Claude Code (model_release 2025-08, reasoning_mode synthesis):** Focused on UX clarity, accessibility acceptance checks, and discoverability aids (footer hints, tooltips).
- **GPT-5 Code (model_release 2025-09, reasoning_mode high):** Detailed architecture touchpoints, provider traits, and strict latency/performance budgets.
- **Consensus:** Reuse existing engines with unified provider, deliver strict latency targets, mandate telemetry + HAL evidence, and ship with configurable thresholds. Disagreement on default fuzzy aggressiveness resolved by adopting conservative defaults with user-tunable weights.

## Exit Criteria
- Acceptance criteria validated via automated tests, dogfooding corpus, and HAL telemetry review.
- Evidence artifacts archived under SPEC-OPS directories with schema v1 compliance reports.
- Documentation updates drafted for `TUI.md`, `SPEC-KIT.md`, and `telemetry-tasks.md` describing new commands and events.
- Changelog entry prepared with UX screenshots or recordings.
