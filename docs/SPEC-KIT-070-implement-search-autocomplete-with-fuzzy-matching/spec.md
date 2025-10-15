**SPEC-ID**: SPEC-KIT-070-implement-search-autocomplete-with-fuzzy-matching
**Feature**: Search Autocomplete with Fuzzy Matching
**Status**: Backlog
**Created**: 2025-10-15
**Branch**: feat/spec-auto-telemetry
**Owner**: Code

**Context**: Codex TUI currently exposes separate search entry points (file search popup, composer slash-commands, SPEC jumpers) that require exact substrings. Typos and partial recall slow teams down, and lack of telemetry hides how search performs. This SPEC introduces unified, fuzzy-matched autocomplete with telemetry and accessibility guardrails.

---

## User Scenarios

### P1: Multi-domain command discovery

**Story**: As a developer, I hit Ctrl+K, type “stat”, and immediately trigger `/spec-status SPEC-KIT-060` even if I omit characters.

**Priority Rationale**: Reduces context switches between SPEC monitoring and implementation commands.

**Testability**: Seed a fixture corpus, run palette query “stat”, assert `/spec-status SPEC-KIT-060` appears in top three suggestions with highlight cues.

**Acceptance Scenarios**:
- Given the global palette is open, when I type “stat”, then the command suggestion appears with fuzzy highlight within 120 ms.
- Given I press Enter on the suggestion, then the command executes and telemetry records a `search.select` event with rank metadata.

### P2: SPEC jump in composer

**Story**: As a developer, entering “/imp 070” in the composer autocompletes to “/speckit.implement SPEC-KIT-070” despite missing characters.

**Priority Rationale**: Speeds up pipeline invocation without memorising entire commands.

**Testability**: Trigger composer popup with partial input, verify suggestion, selection behaviour, and telemetry capture.

**Acceptance Scenarios**:
- Given the composer, when I type “/imp 070”, then the autocomplete list shows `/speckit.implement SPEC-KIT-070` with SPEC metadata badge.
- Given I accept the suggestion, then the command inserts into the composer and the list closes without focus loss.

### P3: File/doc discovery with typos

**Story**: As a developer, typing “fil sarch.rs” still finds `file_search.rs` and opens it in the viewer.

**Priority Rationale**: Maintains velocity when switching between coding and documentation references.

**Testability**: Issue typo query, inspect highlight styling, confirm selection opens the resource, ensure telemetry records partial + final events.

**Acceptance Scenarios**:
- Given a typo query “fil sarch.rs”, when results load, then `file_search.rs` appears with underline + colour highlights.
- Given doc-index MCP is disabled, then results still include filenames sourced from the workspace without crashing.

---

## Edge Cases

- Large workspaces with thousands of files—ensure caches and domain caps prevent UI freezes.
- Unicode or mixed-case input—fuzzy engine must handle composed characters and case-insensitive matching.
- Offline or degraded doc-index service—gracefully drop doc domain while keeping other domains operational.
- Excessive telemetry volume—batch and sample keystroke events without breaking schema v1.
- Accessibility in reduced-colour terminals—highlight uses underline + textual badges.

---

## Requirements

### Functional Requirements

- **FR1**: Unified provider aggregates commands, SPEC IDs, docs, and files with fuzzy scoring and per-domain weights.
- **FR2**: Keyboard interactions (↑/↓/PgUp/PgDn/Tab/Enter/Esc) operate consistently across palette and composer surfaces.
- **FR3**: Evidence-ready telemetry events (`search.start`, `search.partial_result`, `search.final_result`, `search.select`, `search.cancel`, `search.error`) stored under standard SPEC-OPS evidence paths.
- **FR4**: Configurable toggles and thresholds allow conservative defaults and user overrides without code changes.

### Non-Functional Requirements

- **Performance**: Keystroke-to-first-result latency ≤120 ms p95 (≤50 ms warm), ≤500 ms p99; streaming partial batches allowed.
- **Reliability**: Cancel stale searches on new keystrokes, isolate failures per domain, fall back to substring matching when fuzzy scoring errors.
- **Accessibility**: High-contrast palette, underline + colour highlights, stable focus order, screen-reader labels for items.
- **Security**: Respect `.gitignore` and secret-filter rules when sourcing suggestions; no mutation of SPEC.md or docs via autocomplete.

---

## Success Criteria

- Dogfooding corpus shows ≥95 % of validated queries return the intended result in top three suggestions.
- ≥90 % of autocomplete sessions end in a selection; ≤5 % revert to manual substring search.
- Telemetry artifacts (including HAL summaries when enabled) validate schema v1 with zero validation errors.
- Documentation (`TUI.md`, `SPEC-KIT.md`, `telemetry-tasks.md`) updated with new UX details and event definitions.

---

## Evidence & Validation

**Telemetry Path**: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-070-implement-search-autocomplete-with-fuzzy-matching/`

**Consensus Evidence**: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-070-implement-search-autocomplete-with-fuzzy-matching/`

**Validation Hooks**: Local pre-commit hooks (`cargo fmt`, `cargo clippy -- -D warnings`, `cargo test --no-run`, `scripts/doc-structure-validate.sh --mode=templates`) must pass, plus targeted latency benchmarks and telemetry schema checks defined during `/plan` and `/tasks`.

**Manual Verification**: Capture screen recordings or screenshots of palette/composer interactions (before/after) for evidence bundle.

---

## Clarifications

### 2025-10-15 — Initial PRD synthesis

**Clarification needed**: Preferred fuzzy algorithm (`nucleo`, `fuzzy-matcher`, existing `codex_common::fuzzy_match`) and whether we expose recency weighting via config out of the gate.

**Resolution**: Decide during `M1` benchmarking milestone; defaults start conservative with configuration hooks for tuning.

**Updated sections**: Functional Requirements (FR1) acknowledges tunable thresholds.

---

## Dependencies

- Existing `codex-rs` file search streaming engine and selection popup utilities.
- Telemetry schema guard (`docs/SPEC-KIT-013-telemetry-schema-guard/spec.md`) for new event definitions.
- Potential optional dependency on `doc-index` MCP server when available.
- Configuration surface in `config.toml` / TUI settings panel to toggle feature and adjust weights.

---

## Notes

- Ship feature behind configuration flag (`search_autocomplete.unified=true`) defaulting to on, with telemetry gating to monitor adoption.
- Align documentation style (no pipe tables) and ensure `/spec-auto SPEC-KIT-070` flow captures consensus notes for downstream stages.
