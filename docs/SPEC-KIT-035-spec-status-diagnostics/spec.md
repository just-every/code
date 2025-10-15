# Spec: SPEC-KIT-035 spec-status diagnostics
## Inputs
- PRD: docs/SPEC-KIT-035-spec-status-diagnostics/PRD.md (2025-10-07)
- Plan: docs/SPEC-KIT-035-spec-status-diagnostics/plan.md (2025-10-07)
- Constitution: memory/constitution.md (version 1.1, hash 4e159c7eccd2cba0114315e385584abc0106834c)

## Work Breakdown
1. **T47.1 – Rust telemetry reader & data model.** Implement `codex-rs/tui/src/spec_status.rs` that scans SPEC packets and evidence roots to build a `SpecStatusSnapshot`. Capture: core doc presence (PRD/spec/plan/tasks), SPEC.md tracker state, latest guardrail baseline per stage (schema v1/v2), consensus verdict/conflict count, agent coverage (which models participated), and timestamps.
2. **T47.2 – Quick-view scoring & freshness rules.** Derive lightweight cues for each stage (✅/⚠/⏳) driven by baseline status + consensus result, and flag stale telemetry when the most recent artifact exceeds configurable age thresholds (default 24 h). No HAL or policy parsing required.
3. **T47.3 – Evidence footprint sentinel.** Summarize combined evidence size (commands + consensus) with warning banners at ≥20 MB and critical alerts at ≥25 MB. Highlight the heaviest directories so operators know where to clean up—all computed in Rust.
4. **T47.4 – TUI dashboard experience.** Wire `/spec-status <SPEC-ID>` through `slash_command.rs`, `chatwidget.rs`, and rendering helpers so the TUI shows a concise markdown summary (packet health, stage table, tracker note, evidence warnings, recent agent mix, stale indicators). The flow must be TUI-only—no Bash/CLI fallbacks.
5. **T47.5 – Tests & fixtures.** Provide fixtures under `codex-rs/tui/tests/fixtures/spec_status/` covering healthy, stale, missing-consensus, missing-doc, and oversized-evidence scenarios. Add focused unit tests for telemetry parsing + freshness + footprint thresholds, and integration tests ensuring the rendered markdown includes expected cues.
6. **T47.6 – Documentation updates.** Update `docs/slash-commands.md` and draft a concise operator note in `docs/spec-kit/spec-status-diagnostics.md` explaining how to interpret the TUI quick view and data sources. Record example screenshots in evidence once implementation lands.

## Task Synthesis (2025-10-08)
**Consensus (Claude, Gemini, Code agents)**
- All agents converged on a phased sequence: build the Rust telemetry aggregator, compute evidence footprints, surface the `/spec-status` dashboard, keep CLI fallback parity, invest in fixtures/tests, and close with documentation plus release validation. According to Byterover memory layer, evidence thresholds must remain warning-only while still surfaced prominently.
- Claude (agent 585dc343-…​) and Code (agent 18f517a8-…​) explicitly included CLI JSON parity; Gemini (agent 455125bb-…​) focused on the TUI but otherwise aligned on dashboard content.

**Compare & Contrast Notes**
- *Sequencing*: Gemini recommended scaffolding the TUI with placeholder data before wiring live telemetry, whereas Claude and Code prefer landing the aggregators first. We will follow the aggregator-first approach, adding a temporary stub only if visual QA blocks progress.
- *Evidence detail*: Claude and Code insisted on explicit threshold tests (20 MB/25 MB) and remediation hints; Gemini captured alerts conceptually. The merged plan adopts the stricter sentinel checks.
- *HAL/policy guidance*: Claude and Code expect remediation prompts for HAL/policy states even when skipped; Gemini emphasised guardrail/agent cues. We will surface HAL/policy messages as warnings with clear follow-up commands while keeping HAL optional.

**Merged Task Slices for T47**
1. **Telemetry Aggregator & Staleness Guard** – Build `spec_status.rs` parsers (schema v1/v2), classify stage health, and enforce configurable freshness checks. Dependencies: telemetry schema docs, `scripts/spec_ops_004/common.sh`. Validation: `cargo test -p codex-tui spec_status::tests::parse_*`. Evidence: parser logs, fixture outputs.
2. **Evidence Footprint Sentinel** – Compute combined evidence size, raise ⚠/❌ banners, and list top offenders without altering exit codes. Dependencies: filesystem metadata helpers, `/spec-evidence-stats`. Validation: `cargo test -p codex-tui spec_status::tests::footprint_thresholds`. Evidence: screenshots + oversized fixture manifest.
3. **TUI Dashboard Rendering** – Wire `/spec-status` command, render packet health, stage table, blockers, agent mix, and footprint alerts with 80×24 fallback. Dependencies: Slash command plumbing, telemetry aggregator. Validation: `cargo test -p codex-tui spec_status_integration`; manual `/spec-status SPEC-KIT-DEMO`. Evidence: healthy/conflict/stale dashboards.
4. **CLI Fallback JSON Parity** – Update `scripts/spec_ops_004/commands/spec_ops_status.sh` to emit schemaVersion 1.1 JSON, include degraded-mode messaging, and keep human-readable tables. Validation: `spec_ops_status.sh --spec SPEC-KIT-DEMO --format json | jq '.schemaVersion'`. Evidence: stored JSON sample.
5. **HAL & Policy Messaging** – Display HAL/policy skipped vs failed states with remediation hints (`SPEC_OPS_TELEMETRY_HAL=1`, `/spec-ops-validate`). Validation: targeted unit tests + HAL-enabled manual run. Evidence: dashboard capture showing HAL failure vs skipped.
6. **Fixture Library & Regression Coverage** – Populate fixtures for healthy, stale, missing-consensus, missing-doc, oversized-evidence scenarios; add unit + integration tests. Validation: `cargo test -p codex-tui spec_status_integration -- --nocapture`. Evidence: fixture manifest, test logs.
7. **Documentation & Operator Playbook** – Update slash-command docs, add operator guide, embed screenshots, and document footprint cleanup workflow. Validation: `scripts/doc-structure-validate.sh --mode=templates`; `python3 scripts/spec-kit/lint_tasks.py`. Evidence: doc lint logs, published screenshots.
8. **Release Validation & Evidence Ledger** – Run fmt/clippy/test suite, `/spec-status` demos, CLI JSON checks, footprint audit, and capture outputs under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-035/`; update SPEC.md with dated evidence.

**Unresolved Risks**
- Telemetry schema drift beyond v2 will require fixture updates; schedule periodic schema audits.
- Evidence scanning must remain performant on large trees—monitor runtime and consider caching if dashboard latency exceeds 2 s.
- Evaluate whether evidence thresholds should become configurable per SPEC before release validation (raised by Code agent).

## Policy Decisions (2025-10-08)
- Dashboard surfaces only the latest telemetry snapshot per stage; historical retention and advanced diffing remain future scope.
- Staleness threshold defaults to 24 h but must be configurable via environment or config file without code changes.
- Evidence footprint sentinel raises warnings at ≥20 MB and critical alerts at ≥25 MB but does not change exit codes.
- Packet scaffolding issues are treated as warnings in the dashboard (not hard errors) to avoid blocking interactive use.

## Acceptance Mapping
| Requirement (PRD) | Validation Step | Test / Evidence |
| --- | --- | --- |
| R1: Core packet & tracker health surfaced | `cargo test -p codex-tui spec_status::tests::packet_health` with fixtures missing docs | Unit test log + TUI screenshot |
| R2: Stage status and consensus cues visible | `cargo test -p codex-tui spec_status::tests::stage_snapshot` + `/spec-status SPEC-KIT-DEMO` capture showing ✅/⚠ per stage | Unit tests + screenshot |
| R3: Freshness detection warns when telemetry stale | Fixture exceeding staleness threshold exercised via `spec_status_integration.rs`; rendered output shows ⚠ stale badge | Integration test log + screenshot |
| R4: Evidence footprint warnings appear at ≥20 MB (⚠) and ≥25 MB (❌) | Oversized fixture measured via `spec_status_integration.rs`; TUI renders warning banner listing largest directories | Integration test log + screenshot |
| R5: Agent coverage and participation listed | `spec_status_integration.rs` verifies rendered agent mix pulled from consensus artifacts | Integration test log |
| R6: Documentation updated for operators | `docs/slash-commands.md` + `docs/spec-kit/spec-status-diagnostics.md` diffs reviewed; doc lint run | Doc lint log + diff |

## Risks & Unknowns
- **Telemetry drift:** Schema changes beyond v2 could break parsing; mitigate with version detection and fixture updates.
- **Data freshness limits:** If guardrails are paused for extended periods the dashboard will show stale warnings; document how to clear them via reruns.
- **Evidence scanning latency:** Walking large evidence trees may impact responsiveness—optimize for latest artifact discovery and memoize where viable.
- **Spec packet assumptions:** Missing plan/tasks docs should be handled gracefully with actionable guidance instead of panics.

## Consensus & Risks (Multi-AI)
- **Agreement:** Agents aligned on a TUI-first snapshot that reuses existing guardrail and consensus telemetry, highlights stage freshness, and surfaces evidence footprint warnings without shell dependencies or HAL data.
- **Disagreement & resolution:**
  - Claude advocated for deeper evidence drilldowns; consensus kept a lightweight warning banner with top offenders listed.
  - Gemini pushed for HAL summaries; group removed HAL entirely per operator guidance.
- **Degradations:** All participating models available; no degraded runs noted.

## Exit Criteria (Done)
- `/spec-status SPEC-ID` renders the quick-view TUI dashboard with packet health, stage cues, evidence footprint warnings, agent coverage, and stale indicators.
- Unit and integration tests for telemetry parsing, freshness rules, footprint thresholds, and rendering pass; SPEC task lint and doc lint succeed after doc updates.
- Example screenshots and logs stored under SPEC-KIT-035 evidence; SPEC.md tracker updated when implementation completes.
