**SPEC-ID**: SPEC-KIT-080-implement-search-autocomplete-with-fuzzy-matching
**Feature**: Search Autocomplete with Fuzzy Matching
**Status**: Backlog
**Created**: 2025-10-15
**Branch**: feat/spec-auto-telemetry
**Owner**: Multi-agent consensus (gemini, claude, code)

**Context**: Codex TUI currently exposes multiple search entry points (global palette, slash-command composer, SPEC jumper, file search popup) that rely on exact substring matching or inconsistent fuzzy heuristics. Users must recall precise command names, SPEC IDs, or file paths, and telemetry does not capture search effectiveness. This SPEC delivers a unified fuzzy-matched provider that powers every search surface, preserves accessibility standards, and captures schema v1 telemetry for evidence-driven improvements.

---

## User Scenarios

### P1: Fuzzy Palette & Composer Navigation

**Story**: As a Codex maintainer, I want Ctrl+K and the slash-command composer to understand abbreviations like `spe jum` or `imp 080` so that I can jump between SPEC workflows without typing full commands or IDs.

**Priority Rationale**: Palette and composer searches occur dozens of times per session; improving them unlocks the largest productivity gain.

**Testability**: Automated integration tests verify ranked results for representative abbreviations and typos; telemetry confirms selections occur within top 3 results.

**Acceptance Scenarios**:
- Given the palette is open, when the user types `spe jum`, then `/spec-jump` appears first with highlighted matches and executes on Enter.
- Given the composer is active, when the user types `/imp 080`, then `/speckit.implement SPEC-KIT-080` is suggested with short help and inserts on Enter.
- Given fuzzy mode is disabled, when the user repeats the query, then the UI shows a “Degraded: substring” badge and logs `search.fallback.degraded`.

### P2: SPEC Jumper Abbreviation Support

**Story**: As an engineer juggling multiple SPECs, I want to type `kt80` or `80` in the SPEC jumper to locate SPEC-KIT-080 with status context so that I can switch documents quickly.

**Priority Rationale**: SPEC switching is essential during multi-agent reviews; abbreviation support removes friction and aligns with telemetry goals.

**Testability**: Unit tests cover abbreviation parsing and scoring; manual QA confirms ambiguous inputs show disambiguation metadata.

**Acceptance Scenarios**:
- Given SPEC-KIT-080 exists, when the user types `kt80`, then SPEC-KIT-080 ranks first with status badge (Backlog/In Progress etc.).
- Given multiple SPECs share numeric postfixes, when the user types `80`, then results order by exactness then recency, and each entry exposes area slug to disambiguate.
- Given no matching SPEC exists, when the user submits `kt99`, then the jumper shows “No SPEC found” with guidance to run `/new-spec` (future work) and logs the miss in telemetry.

### P3: Accessible File Search and Fallback Behaviour

**Story**: As a screen-reader user, I want the `@` file search popup to announce result counts, selection positions, and highlight cues so that I can select the intended file even when fuzzy search degrades to substring mode.

**Priority Rationale**: Accessibility is a constitutional requirement; ensuring parity across fuzzy and fallback modes protects all users.

**Testability**: Manual NVDA/JAWS scripts validate announcements; automated checks verify highlight contrast ≥4.5:1.

**Acceptance Scenarios**:
- Given fuzzy mode is active, when the user types `@hlcfg`, then `docs/hal/hal_config.toml` ranks first, highlights matched characters with contrast ≥4.5:1, and screen reader announces “1 of N results, high match.”
- Given fuzzy engine fails during startup, when the user types `@cmd pop`, then substring results appear with “Degraded: substring” banner and telemetry records fallback=true.
- Given a long-running query is cancelled by additional typing, when the user continues entering characters, then stale results are discarded and the screen reader updates the count once new results arrive.

---

## Edge Cases

- Empty or whitespace-only queries show recent items (MRU-first) without invoking fuzzy scoring.
- Single-character queries either degrade to prefix search or prompt the user to type additional characters (configurable threshold).
- Unicode and diacritic inputs (e.g., `résumé`, `ß`, CJK characters) match case-insensitively without panics.
- Extremely long queries are truncated to a safe length (e.g., 120 chars) to protect performance; UI displays a truncation hint.
- Duplicate names include disambiguators (entity type, relative path) to avoid user confusion.
- Index corruption triggers automatic rebuild and emits `search.index.refresh.started/completed` events.
- Telemetry instrumentation must drop payloads gracefully when the buffer is full, logging sampled warnings.
- Fallback mode must respect `.gitignore` and secret filters identically to fuzzy mode.

---

## Requirements

### Functional Requirements

- **FR1**: Provide a shared search provider module (`codex-rs/tui/src/search/provider.rs`) consumed by palette, composer, SPEC jumper, and file search.
- **FR2**: Support commands, SPEC IDs/titles/status badges, docs (PRDs/specs/plans/tasks), and files while respecting `.gitignore` and secret filters.
- **FR3**: Implement fuzzy scoring tolerant of insertions, deletions, and transpositions; return match positions for highlighting.
- **FR4**: Default ranking combines fuzzy score, prefix boosts, entity weighting, MRU signal, and recency decay; allow configuration overrides.
- **FR5**: Highlight matched spans with consistent styling and maintain stable focus order across surfaces.
- **FR6**: Expose configuration keys (`search.fuzzy.enabled`, thresholds, weights, source toggles) in `config.toml` and via env overrides.
- **FR7**: Emit schema v1 telemetry (`search.query.submitted`, `search.autocomplete.shown`, `search.autocomplete.selected`, `search.result.opened`, `search.index.refresh.started/completed`, `search.fallback.degraded`, `search.error`).
- **FR8**: Provide graceful fallback to case-insensitive substring search with explicit UI indicator and telemetry when the fuzzy engine is unavailable or disabled.
- **FR9**: Maintain screen-reader announcements and keyboard-only navigation parity across fuzzy and fallback modes.

### Non-Functional Requirements

- **Performance**: Meet ≤120 ms p95 keystroke-to-painted latency (≤50 ms warm p50) on 10k-file fixture; cold index build ≤3 s.
- **Reliability**: Cancel stale queries, avoid panics on malformed input, and auto-rebuild corrupted indices.
- **Security & Privacy**: Sanitize input, redact sensitive paths, and avoid emitting secret-bearing telemetry payloads.
- **Maintainability**: Localise fuzzy logic to `codex-rs/tui/src/search/*` with ≥90 % unit coverage and clear documentation for weight tuning.
- **Accessibility**: Validate keyboard focus, highlight contrast ≥4.5:1, and screen-reader transcripts stored in evidence.

---

## Success Criteria

- Unified provider powers all four search surfaces with consistent ranking and highlighting.
- Benchmark evidence demonstrates ≤120 ms p95 latency and ≤5 % fallback usage during dogfooding.
- Telemetry bundles (including HAL summaries when `SPEC_OPS_TELEMETRY_HAL=1`) cover ≥95 % of search sessions without schema violations.
- Accessibility review confirms announcements and focus behaviour; no regressions recorded.
- Degraded mode tested and documented, including telemetry artifact.

---

## Evidence & Validation

**Acceptance Tests**: Mapped in `docs/SPEC-KIT-080-implement-search-autocomplete-with-fuzzy-matching/tasks.md` (to be generated during `/spec-auto`). Expected test modules:
- `codex-rs/tui/tests/search_autocomplete.rs` — fuzzy correctness, ranking weights, abbreviation handling, fallback paths.
- `codex-rs/tui/tests/accessibility.rs` — keyboard focus and screen-reader hooks.
- Criterion bench `codex-rs/benches/fuzzy_search.rs` — latency measurements.

**Telemetry Path**: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-080-implement-search-autocomplete-with-fuzzy-matching/`

**Consensus Evidence**: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-080-implement-search-autocomplete-with-fuzzy-matching/`

**Validation Commands**:
```bash
/speckit.plan SPEC-KIT-080
/speckit.tasks SPEC-KIT-080
/speckit.implement SPEC-KIT-080
/speckit.auto SPEC-KIT-080
/speckit.status SPEC-KIT-080
SPEC_OPS_TELEMETRY_HAL=1 /spec-ops-validate SPEC-KIT-080
```

---

## Clarifications

### 2025-10-15 - Initial Spec Creation

**Clarification needed**: Selection of fuzzy engine (`codex_common::fuzzy_match` extension vs `fuzzy-matcher` vs `skim`) and default debounce window remain open research tasks.

**Resolution**: Prototype and benchmark options during plan/tasks stages; document chosen engine, thresholds, and telemetry sampling in evidence artifacts before implementation closes.

**Updated sections**: Requirements (FR3/FR4/FR7), Success Criteria, Evidence & Validation, Notes.

---

## Dependencies

- `codex-rs/tui/src/{palette.rs, slash_command.rs, spec_jumper.rs, file_search.rs, search/*}` for integration.
- `codex_common` and `codex_file_search` crates for fuzzy utilities and streaming adapters.
- Guardrail scripts `scripts/spec_ops_004/commands/spec_ops_*` for validation and telemetry capture.
- Local configuration (`config.toml`, env overrides) to expose fuzzy settings.

---

## Notes

- Keep defaults conservative; expose tuning knobs but ship balanced presets overseen by maintainers.
- Persist MRU/frequency data locally only after documenting privacy considerations; ensure opt-out path exists.
- Align documentation updates (`TUI.md`, `telemetry-tasks.md`, command reference) with release to avoid drift.
- Future work: evaluate semantic search or history-ranked suggestions after fuzzy baseline proves reliable.
