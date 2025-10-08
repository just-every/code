# Plan: SPEC-KIT-035 spec-status diagnostics
## Inputs
- Spec: docs/SPEC-KIT-035-spec-status-diagnostics/spec.md (2025-10-08 revision)
- Constitution: memory/constitution.md (hash 4e159c7eccd2cba0114315e385584abc0106834c)

## Work Breakdown
1. **Baseline inventory.** Verify SPEC packet scaffolding (PRD/spec/plan/tasks) and SPEC.md tracker entries exist; record any gaps the dashboard should surface.
2. **Rust telemetry reader.** Implement `codex-rs/tui/src/spec_status.rs` to gather packet health, guardrail baseline results (schema v1/v2), consensus verdicts/conflicts, agent participation, timestamps, and evidence directory sizes.
3. **Quick-view scoring.** Translate telemetry into simple status cues (✅/⚠/⏳) plus configurable stale indicators (default 24 h) without referencing HAL or policy feeds.
4. **Evidence footprint sentinel.** Compute combined evidence sizes (commands + consensus) inside Rust, raise warnings at ≥20 MB and critical alerts at ≥25 MB, and list the heaviest directories for clean-up.
5. **TUI rendering.** Wire `/spec-status <SPEC-ID>` through slash command plumbing and chat rendering to display a concise markdown summary—packet health, SPEC.md tracker note, evidence warnings, per-stage table, agent mix, stale badges—entirely inside the TUI.
6. **Fixtures & automated tests.** Create fixtures for healthy, stale, missing-consensus, missing-doc, and oversized-evidence scenarios; add unit tests for parsing/scoring/footprint thresholds and integration tests asserting rendered cues.
7. **Documentation & evidence.** Update `docs/slash-commands.md` and `docs/spec-kit/spec-status-diagnostics.md` with interpretation guidance; capture example screenshots/logs for evidence once implementation lands.

## Acceptance Mapping
| Requirement (Spec) | Validation Step | Test/Check Artifact |
| --- | --- | --- |
| R1: Packet & tracker health reported | `cargo test -p codex-tui spec_status::tests::packet_health` using missing-doc fixture | Unit test log |
| R2: Stage cues show guardrail + consensus status | Integration test `spec_status_integration.rs` renders ✅/⚠ per stage | Integration test log + snapshot |
| R3: Stale telemetry flagged | Fixture older than threshold triggers stale badge in integration test | Integration test log |
| R4: Evidence footprint warnings raised at ≥20 MB/≥25 MB | Oversized fixture triggers warning banner listing top directories | Integration test log |
| R5: Agent participation listed | Integration test validates agent mix taken from consensus artifacts | Integration test log |
| R6: Docs refreshed | `scripts/doc-structure-validate.sh --mode=templates` after updating docs | Doc lint log |

## Risks & Unknowns
- Telemetry schema drift beyond v2 could break parsing; add version guards and fixtures for new fields.
- Evidence directories may become large; optimise both latest-artifact lookup and footprint calculations to keep the TUI responsive.
- Missing docs or tracker rows must produce actionable guidance instead of panics to avoid blocking operators.
- Potential scope creep toward live-refresh dashboards should be tracked separately once the quick view ships.

## Consensus & Risks (Multi-AI)
- Agreement: Agents aligned on a Rust-first, TUI-only snapshot that reuses existing telemetry, highlights stage freshness, and surfaces evidence footprint warnings without Bash dependencies.
- Disagreement & resolution: Claude requested deeper evidence drilldowns; consensus kept a lightweight warning banner. Gemini proposed HAL summaries (omitted per clarified scope).

## Exit Criteria (Done)
- `/spec-status SPEC-ID` renders the quick-view dashboard with packet health, stage cues, evidence warnings, agent mix, and stale indicators.
- Telemetry parsing handles schema v1/v2 gracefully; unit/integration suites (including footprint thresholds) pass with new fixtures.
- Documentation updates land with supporting evidence artifacts and doc lint success.
- SPEC.md tracker updated post-implementation with validation evidence.
