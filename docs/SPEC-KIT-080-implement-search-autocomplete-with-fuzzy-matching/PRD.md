# PRD: SPEC-KIT-080 — Search Autocomplete with Fuzzy Matching

**SPEC-ID**: SPEC-KIT-080-implement-search-autocomplete-with-fuzzy-matching
**Status**: Draft
**Created**: 2025-10-15
**Author**: Multi-agent consensus (gemini, claude, code)

---

## Problem Statement

**Current State**: Codex TUI exposes multiple search entry points (global palette, slash-command composer, SPEC jumper, file search popup) that mostly rely on exact substring matching. Typo tolerance is inconsistent, SPEC navigation requires precise IDs, and ranking heuristics vary across surfaces.

**Pain Points**:
- Typos or partial recall cause empty results, forcing users to retype full names or paths.
- SPEC IDs must be entered verbatim (e.g., `SPEC-KIT-080`), making rapid context switches cumbersome.
- Global palette and composer suggestions are not unified; users repeat queries in different surfaces.
- Lack of unified telemetry obscures how search is used or when it degrades.
- Accessibility gaps (focus jumps, ambiguous screen-reader output) make search difficult for keyboard-only users.

**Impact**: Developers lose momentum when navigating between commands, SPECs, docs, and files. New contributors struggle to discover capabilities. Missing telemetry prevents evidence-driven improvements, delaying guardrail enforcement and automation.

---

## Target Users & Use Cases

### Primary User: Codex CLI Power User

**Profile**: Maintainer or staff engineer using Codex TUI daily to move between SPECs, guardrail scripts, and evidence artifacts. Works keyboard-first and frequently references 10–50 active SPECs.

**Current Workflow**: Uses Ctrl+K, `/` composer, and `@` file search dozens of times per session. Memorises full command names and SPEC IDs to avoid search misses.

**Pain Points**: Typos such as `speckit.imlpement` produce no results; switching from SPEC plan to implementation requires retyping full IDs; file search cannot recover from minor spelling mistakes.

**Desired Outcome**: Enter abbreviations like `kt80` or `hlcfg`, receive accurate ranked results within 120 ms, and execute actions without leaving the keyboard.

### Secondary User: New Codex Contributor

**Profile**: Engineer onboarding to Spec-Kit workflows via Codex CLI.

**Use Case**: Learns commands and SPEC documentation via search; expects forgiving suggestions and contextual metadata (status badges, short help) to orient quickly.

---

## Goals

### Primary Goals

1. **Deliver unified fuzzy-matched autocomplete across TUI surfaces**: Global palette, composer, SPEC jumper, and file search share a single provider, consistent ranking, and highlighted matches.
   **Success Metric**: ≥90 % of benchmark queries return the intended item within the top 3 results under 120 ms p95.

2. **Enable SPEC abbreviation navigation**: Accept patterns like `kt80`, `ops4`, and numeric-only shorthand while prioritising exact IDs.
   **Success Metric**: Telemetry shows ≥60 % of SPEC navigations use abbreviations within 30 days.

### Secondary Goals

1. **Capture actionable telemetry**: Emit schema v1 events for query, selection, fallback, and errors to power evidence and HAL validation.
2. **Protect accessibility**: Maintain keyboard-only operation and screen-reader clarity while adding visual highlights.

---

## Non-Goals

- Natural language or semantic search (e.g., "show HAL telemetry doc").
- Indexing file contents; scope remains filenames, commands, docs, and SPEC metadata.
- Persisting cloud-based personalization or cross-repo federated search.
- Rewriting existing UI components beyond integrating the unified provider.

These remain future candidates once fuzzy text matching is proven.

---

## Scope & Assumptions

**In Scope**:
- Unified provider and ranking pipeline for commands, SPECs, docs, and files.
- Configurable fuzzy engine thresholds, per-domain weights, and MRU/recency boosts.
- SPEC ID parsing and abbreviation expansion with status metadata surfaced in results.
- Telemetry events (`search.query.submitted`, `search.autocomplete.shown`, etc.) plus HAL capture when enabled.
- Accessibility updates (focus order, announcements, contrast) aligned with WCAG 2.1 AA.

**Assumptions**:
- Existing `codex_common::fuzzy_match` can be extended or swapped for crates like `fuzzy-matcher` without violating licensing.
- `codex_file_search` streaming architecture supports pluggable scoring callbacks.
- SPEC naming convention `SPEC-<AREA>-<ID>-<slug>` remains stable.

**Constraints**:
- Keystroke-to-painted results ≤120 ms p95, ≤50 ms warm p50 on 10k-file fixture.
- Index memory footprint ≤100 MB on typical repositories.
- Must operate offline and respect `.gitignore`/secret filtering.
- Feature must be flaggable (`search.fuzzy.enabled`) with graceful degradation to substring mode.

---

## Functional Requirements

| ID | Requirement | Acceptance Criteria | Priority |
| --- | --- | --- | --- |
| FR1 | Unified fuzzy provider | All four surfaces consume a shared provider that yields ranked, highlighted results for commands, SPECs, docs, and files. | P1 |
| FR2 | SPEC abbreviation support | Inputs like `kt80`, `SPEC-KIT-80`, or `80` rank SPEC-KIT-080 first; ties disambiguated by recency metadata. | P1 |
| FR3 | Typo tolerance | Queries with Levenshtein distance ≤2 (e.g., `serch_popup`) return the intended item within the top 3 results. | P1 |
| FR4 | Ranking strategy | Default ordering combines fuzzy score, prefix boosts, entity weighting (commands > SPECs > docs > files), MRU, and recency signals. | P1 |
| FR5 | Highlighting & UX cues | Matched character spans render consistently across surfaces with high-contrast styling and stable focus behaviour. | P2 |
| FR6 | Configuration & overrides | `config.toml` and env vars adjust fuzzy thresholds, per-domain weights, and source inclusion; defaults documented. | P2 |
| FR7 | Telemetry coverage | Each search lifecycle emits schema v1 events: query submitted, autocomplete shown, item selected, result opened, fallback, error. | P1 |
| FR8 | Graceful fallback | When fuzzy engine is disabled or fails, UI signals “Degraded: substring” and logs `search.fallback.degraded` while keeping search usable. | P1 |
| FR9 | Accessibility compliance | Screen readers announce result counts and selection changes; keyboard-only navigation passes manual testing. | P1 |

---

## Non-Functional Requirements

| ID | Requirement | Target Metric | Validation Method |
| --- | --- | --- | --- |
| NFR1 | Performance | ≤120 ms p95 keystroke-to-painted results on 10k-file fixture; ≤500 ms p99; ≤3 s cold index build. | Criterion or custom `cargo bench --bench fuzzy_search` plus telemetry capture. |
| NFR2 | Reliability | 99.5 %+ successful search executions; zero panics on malformed input; stale searches cancelled promptly. | Fuzz tests, cancellation unit tests, error telemetry review. |
| NFR3 | Security & Privacy | Queries sanitized; ignore lists enforced; no sensitive paths persisted or emitted in events. | Static analysis, security review during `/spec-ops-audit`, targeted tests with special characters. |
| NFR4 | Maintainability | Fuzzy logic isolated in `codex-rs/tui/src/search/`; ≥90 % unit coverage for ranking and fallback logic. | Coverage report via `cargo tarpaulin`, code review checklist. |
| NFR5 | Accessibility | WCAG 2.1 AA for highlights and focus; screen-reader scripts validated with NVDA/JAWS. | Manual accessibility QA, documented transcript stored in evidence. |

---

## User Experience

**Key Workflows**:

### Workflow 1: Unified Palette Search

**Steps**:
1. User presses Ctrl+K.
2. Types `spe jum`.
3. Palette shows grouped results (commands, SPECs, docs, files) with live highlighting and latency under 120 ms.
4. User navigates via ↑/↓ or Tab between groups and presses Enter to open SPEC jumper.

**Success Path**: Intended SPEC action selected within two keystrokes beyond the abbreviation.

**Error Paths**:
- If no fuzzy matches exceed configured threshold, palette shows “No fuzzy matches—press Tab for exact search” and offers recent items.
- When fuzzy engine fails, header badge reads “Degraded: substring” and telemetry records fallback reason.

### Workflow 2: SPEC Jumper Abbreviations

**Steps**:
1. User invokes SPEC jumper shortcut.
2. Types `kt80`.
3. SPEC-KIT-080 appears first with status badge (Backlog/In Progress etc.) plus document shortcuts.
4. Enter opens the chosen doc (default spec.md) or inserts reference in composer.

**Error Paths**:
- Ambiguous abbreviations show multiple SPECs, ordered by exactness then telemetry-driven recency; user can disambiguate via Right arrow to expand context.

### Workflow 3: Composer Command Discovery

**Steps**:
1. User types `/` in composer.
2. Enters `imp 080`.
3. Autocomplete suggests `/speckit.implement SPEC-KIT-080` with snippet preview.
4. User presses Enter to insert command.

**Error Paths**:
- If fuzzy disabled, composer header indicates “Exact match mode”; suggestions limit to substring results and guidance advises re-enabling fuzzy in settings.

---

## Dependencies

**Technical**:
- Potential new crates: `fuzzy-matcher` (Skim), `ignore`, `dashmap`/`arc-swap` for caches.
- TUI modules: `codex-rs/tui/src/{palette.rs, slash_command.rs, spec_jumper.rs, file_search.rs, search/*}`.
- Guardrail integration via `scripts/spec_ops_004/commands/spec_ops_*` for validation.

**Organizational**:
- Maintainer sign-off on crate additions and telemetry schema updates.
- UX review to validate accessibility improvements.

**Data**:
- SPEC metadata sourced from `SPEC.md` (status, titles) and docs directory naming.
- File system snapshots respecting `.gitignore`; MRU data persisted locally.

---

## Risks & Mitigations

| Risk | Impact | Probability | Mitigation | Owner |
| --- | --- | --- | --- | --- |
| Performance regression on large repos | High | Medium | Benchmark candidate engines early; cache results; cap per-domain results; document knobs for tuning. | GPT-5 Codex agent |
| Excessive telemetry volume | Medium | Medium | Debounce `search.query.submitted` (≥150 ms idle), sample high-frequency sessions, compress artifacts before committing. | Gemini agent |
| Accessibility regressions | High | Low | Maintain contrast/focus acceptance tests; schedule manual NVDA/JAWS pass before release. | Claude agent |

---

## Success Metrics

**Launch Criteria**:
- P1 functional requirements demonstrated via automated tests and manual dogfooding.
- Telemetry schema validated with HAL (`SPEC_OPS_TELEMETRY_HAL=1`) and stored in evidence directory.
- p95 latency benchmark ≤120 ms and documented in evidence.
- Degraded mode exercise captured with corresponding telemetry event and README note.

**Post-Launch Metrics** (30-day window):
- ≥85 % of selections occur within top 3 ranked items; ≤5 % of sessions revert to exact-match fallback.
- Median keystrokes per navigation drop ≥30 % compared to baseline telemetry.
- ≥95 % of search sessions emit complete telemetry bundles without schema violations.
- Positive qualitative sentiment gathered via guardrail feedback channel.

---

## Validation Plan

### Testing Strategy

1. **Unit Tests**: Cover fuzzy scoring, ranking weights, abbreviation parsing, fallback switching (`codex-rs/tui/tests/search_autocomplete.rs`).
2. **Integration Tests**: Full-surface flows (palette, composer, SPEC jumper, file search) using synthetic fixtures and mock SPEC metadata.
3. **Performance Tests**: Criterion or custom benches for latency and index build; run under CI load profile.
4. **Accessibility Tests**: Manual NVDA/JAWS walkthrough with transcript stored in evidence; automated contrast checks where feasible.

### Review Process

1. **PRD Review**: Multi-agent sign-off plus maintainer review ensuring constitution alignment.
2. **Design Review**: UX/accessibility walkthrough of prototypes and telemetry dashboards.
3. **Code Review**: Pair review focusing on perf, security, and guardrail integration.
4. **Security Review**: Validate sanitisation and ignore rules within `/spec-ops-audit` stage.

---

## Multi-Agent Consensus

### PRD Quality Assessment

**Completeness**: High — covers all search surfaces, telemetry, accessibility, and fallback behaviour with acceptance criteria.

**Clarity**: Requirements and metrics are measurable (latency budgets, accuracy thresholds, telemetry coverage).

**Testability**: Validation plan maps to unit, integration, performance, and accessibility checks; telemetry enables post-launch verification.

### Conflicts Resolved

- **Weighting Strategy**: Gemini preferred MRU-heavy ranking; Claude favoured pure fuzzy score. Consensus: balanced default (score + prefix boost + entity weighting + moderate MRU/recency) with configurable overrides.
- **Telemetry Volume**: Concern about per-keystroke events. Resolution: debounce submissions (≥150 ms) and always log selections/fallbacks; sample long sessions when necessary.
- **Scope Creep into Semantic Search**: Code agent suggested intent matching. Consensus: defer natural-language features to future SPEC once fuzzy foundation proves stable.

---

## Evidence & Telemetry

**Telemetry Events**:
- `search.query.submitted`
- `search.autocomplete.shown`
- `search.autocomplete.selected`
- `search.result.opened`
- `search.index.refresh.started` / `search.index.refresh.completed`
- `search.fallback.degraded`
- `search.error`

All events include schema v1 common fields (`command`, `specId`, `sessionId`, `timestamp`, `schemaVersion`, `artifacts[]`) plus surface, latency, result counts, match scores, and degradation flags as applicable.

**Evidence Storage**: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-080-implement-search-autocomplete-with-fuzzy-matching/`

**Validation Hooks**: `/speckit.analyze SPEC-KIT-080`, `/spec-ops-plan|tasks|implement|validate|audit|unlock SPEC-KIT-080`, and HAL smoke runs with `SPEC_OPS_TELEMETRY_HAL=1` capturing `hal.summary` artifacts.

---

## Open Questions

1. Which fuzzy engine (extended `codex_common::fuzzy_match`, `fuzzy-matcher`, or `skim`) offers the best latency/accuracy trade-off under Spec-Kit workloads? (Blocker: Yes — prototype required in plan stage.)
2. Should MRU/frequency persist across sessions or remain ephemeral? (Impact: Medium — affects telemetry interpretation.)
3. What debounce window best balances telemetry fidelity with volume? (Impact: Medium — candidate values 150–300 ms.)

---

## Changelog

### 2025-10-15 - Initial PRD
- Drafted via multi-agent consensus (gemini, claude, code).
- Captured performance, telemetry, and accessibility guardrails for unified fuzzy search.
- Logged consensus resolutions on ranking weights, telemetry sampling, and semantic search deferral.
