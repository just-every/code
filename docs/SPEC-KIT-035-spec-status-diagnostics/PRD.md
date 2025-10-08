# PRD: Spec Status Diagnostics (SPEC-KIT-035)

## Problem Statement
- `/spec-status` currently proxies to `scripts/spec_ops_004/commands/spec_ops_status.sh`, producing a minimal stage table that lacks consensus, HAL, policy, or evidence footprint insight.
- Operators must manually inspect telemetry directories (`docs/SPEC-OPS-004-integrated-coder-hooks/evidence/**`) to diagnose stale runs, missing synthesis artifacts, or HAL regressions, slowing recovery when guardrails fail.
- Evidence growth beyond the documented 25 MB threshold is invisible until downstream workflows degrade, risking repository bloat and incomplete validation.
- Without a reliable status snapshot, automation (dashboards, alerting) cannot consume health signals programmatically, forcing ad-hoc shell parsing.

## Target Users & Use Cases
- **Spec Ops operators** need a single dashboard to verify guardrail outcomes, consensus verdicts, HAL health, and blockers before advancing `/spec-auto`.
- **Guardrail maintainers** require fast visibility into telemetry gaps (missing JSON, skipped policy layers, stale timestamps) to triage failures.
- **Automation/observability engineers** want JSON output that downstream monitors can ingest to flag regressions and evidence footprint drift.

## Goals
1. Deliver a `/spec-status <SPEC-ID>` dashboard rendered in the Codex TUI (and CLI) that aggregates guardrail telemetry, consensus artifacts, agent metrics, policy/HAL summaries, and evidence size warnings.
2. Differentiate `passed`, `failed`, `skipped`, and `stale` states for baseline, policy, HAL, and consensus so operators can respond appropriately.
3. Expose evidence size warnings at ≥20 MB and critical alerts ≥25 MB, pointing to oversized directories.
4. Provide a machine-readable output (`--format json`) with schema metadata so automation can reuse the diagnostics.

## Non-Goals
- Implementing interactive drill-down panes or real-time auto-refresh (deferred per plan.md).
- Changing guardrail telemetry schema versions or relocating evidence directories.
- Automatically remediating failures; scope is detection and operator guidance.

## Scope & Assumptions
- Dashboard remains markdown-style to fit the 1024×768 Codex viewport and 80×24 fallback, using concise tables and callouts.
- Aggregation must degrade gracefully: missing HAL/policy telemetry is reported as `skipped` unless explicit failure payloads exist.
- Evidence directories may be large; implementation must avoid full rescans on every invocation (cache latest entries per stage where possible).
- JSON output extends existing tooling rather than introducing new persistence stores; local-memory continues to hold narrative updates.

## Functional Requirements & Acceptance Criteria

| ID | Requirement | Acceptance Criteria / Validation |
| --- | --- | --- |
| R1 | Aggregate guardrail telemetry per stage (plan→unlock) and display baseline status, timestamps, failure details. | `cargo test -p codex-tui spec_status::parse_guardrail` with fixture telemetry (pass/fail) and `/spec-status SPEC-KIT-DEMO` showcasing baseline failure messaging. |
| R2 | Surface consensus synthesis status, conflicts, and agent counts per stage. | Fixture `spec_status_consensus_conflict.json`; `/spec-status` shows `⚠ conflicts (N)` and lists conflict topics; unit test `spec_status::parse_consensus`. |
| R3 | Enumerate recent agent runs (model, status, timestamp) and highlight failed agents with remediation pointers. | Integration test using synthetic agent result files; manual TUI run ensuring failed agent entries include log paths. |
| R4 | Compute evidence footprint (commands + consensus) with warnings at ≥20 MB and critical alert at ≥25 MB. | Fixture directory sized via `spec_status_fixtures.sh`; `/spec-status` prints `⚠ 22 MB` warning and `❌` when exceeding 25 MB; unit test `spec_status::footprint_thresholds`. |
| R5 | Provide JSON export aligned with schema v1 + dashboard extensions (`schemaVersion`, `stages[]`, `evidenceFootprint`). | `scripts/spec_ops_004/commands/spec_ops_status.sh --format json | jq '.schemaVersion'` returns `1.1`; contract test verifies JSON keys. |
| R6 | Offer actionable blockers/next steps (e.g., rerun guardrail, resolve conflicts) with guidance sourced from telemetry states. | Manual validation on failing fixture ensures blocker list suggests `/spec-ops-validate` or `/spec-consensus`; unit test mapping state→remediation. |
| R7 | Maintain CLI fallback parity (text + JSON) while pointing users to the richer TUI experience. | Updated `spec_ops_status.sh` prints reference to `/spec-status` and exits non-zero on missing telemetry; shellcheck passes. |
| R8 | Bundle fixtures and documentation demonstrating healthy, stale, and failing scenarios. | `scripts/spec_ops_004/spec_status_fixtures.sh --out ...` produces fixtures consumed by tests; docs updated with troubleshooting steps; lint (`python3 scripts/spec-kit/lint_tasks.py`) passes post-update. |

## Telemetry & Evidence Handling
- Aggregator reads guardrail payloads from `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/<SPEC-ID>/` using helpers in `scripts/spec_ops_004/common.sh`.
- Consensus artifacts sourced from `.../evidence/consensus/<SPEC-ID>/` (Gemini, Claude, GPT outputs + synthesis).
- Aggregator caches latest file per stage to avoid scanning entire directories repeatedly; stale telemetry (>7 days old) flagged in blockers.
- Evidence footprint computed via `du` or Rust metadata APIs; warnings highlight specific directories exceeding thresholds.
- JSON export includes `schemaVersion`, `generatedAt`, `stages[]`, `agents[]`, and `evidence` metadata for downstream consumers.

## Dependencies
- `codex-rs/tui` for rendering, slash-command plumbing, and unit testing harness.
- Guardrail helpers in `scripts/spec_ops_004/common.sh` for consistent telemetry locations and policy/HAL status capture.
- Telemetry schema references (`docs/SPEC-KIT-013-telemetry-schema-guard/spec.md`, `docs/spec-kit/telemetry-schema-v2.md`).
- Local-memory context for documenting remediation and operational playbooks.

## Risks & Mitigations
- **Telemetry drift:** Schema changes could break parsing → introduce version detection, fixtures covering v1/v1+HAL, and CI tests.
- **Performance:** Large evidence directories may slow status generation → implement lazy scanning and warn about overly large directories to prompt cleanup.
- **Concurrent writes:** Guardrail runs may update telemetry mid-read → handle missing/partial files gracefully and mark snapshot as `in-progress` if reads fail.
- **TUI layout overflow:** Dense data can exceed viewport → prioritise concise tables and wrap long paths; verify 80×24 compatibility.

## Validation Plan
1. `cargo test -p codex-tui spec_status::*` covering telemetry parsing, evidence thresholds, remediation mapping, and JSON serialization.
2. Run `/spec-status SPEC-KIT-DEMO` with healthy and failing fixture bundles to confirm visual output and blockers.
3. Execute `scripts/spec_ops_004/commands/spec_ops_status.sh --spec SPEC-KIT-DEMO --format json` ensuring CLI fallback parity and schema adherence.
4. Update docs and run `python3 scripts/spec-kit/lint_tasks.py` plus `scripts/doc-structure-validate.sh --mode=templates --dry-run` before merge.
5. Capture before/after evidence snapshots via `scripts/spec_ops_004/evidence_stats.sh` to demonstrate footprint alerting.

## Success Metrics
- `/spec-status` renders within 2 seconds on typical telemetry sizes (<5k files).
- Every stage row shows a concrete status (`passed`, `failed`, `skipped`, `stale`) when telemetry exists.
- Operators resolve stale or missing telemetry in ≤1 follow-up command based on surfaced blockers (tracked via Spec Ops retrospectives).
- Evidence warnings trigger clean-up actions before repos exceed the 25 MB per-SPEC guideline.

## Documentation & Rollout
- Update `docs/slash-commands.md`, `docs/spec-ops-tasks.md`, and CLAUDE.md troubleshooting sections with new dashboard guidance.
- Provide fixture instructions under `docs/spec-kit/` so contributors can reproduce healthy/failing scenarios locally.
- Announce availability in release notes and record telemetry/fixtures under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-035/`.

## Open Questions
1. Should crossing the 25 MB critical evidence threshold force `/spec-status` (or CLI fallback) to exit non-zero to block pipelines?
2. What retention window should the aggregator respect when multiple telemetry snapshots exist (latest only vs N recent runs)?
3. Do we expose an explicit flag to suppress HAL checks in air-gapped environments, or rely solely on the existing `SPEC_OPS_HAL_SKIP` convention?
