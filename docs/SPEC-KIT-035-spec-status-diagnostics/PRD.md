# PRD: Spec Status Diagnostics (SPEC-KIT-035)

## Problem Statement
- `/spec-status` currently proxies to `scripts/spec_ops_004/commands/spec_ops_status.sh`, yielding a bare-bones table that omits consensus health, agent coverage, or freshness cues.
- Operators still dig through `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/**` to confirm whether telemetry is recent or missing, slowing incident response.
- Evidence growth beyond the 20/25 MB guardrails is invisible until downstream workflows degrade, so cleanup work arrives late.
- The Bash-based workflow is brittle inside the TUI and unavailable to users who rely solely on the Rust client.
- Without a concise TUI snapshot, teams lack a shared understanding of SPEC progress across plan→unlock stages.

## Target Users & Use Cases
- **Spec Ops operators** need a single dashboard to verify guardrail outcomes, consensus verdicts, freshness, and blockers before advancing `/spec-auto`.
- **Guardrail maintainers** require fast visibility into telemetry gaps (missing JSON, stale timestamps) to triage failures.
- **Automation/observability engineers** want predictable cues surfaced in the TUI so they can mirror alerts manually until automation hooks are defined.

## Goals
1. Deliver a `/spec-status <SPEC-ID>` dashboard rendered directly in the Codex TUI that summarizes guardrail baseline outcomes, consensus verdicts, agent participation, and freshness cues.
2. Differentiate `passed`, `failed`, and `stale` states per stage so operators know whether action is required.
3. Surface packet scaffolding issues (missing PRD/spec/plan/tasks or SPEC.md tracker entry) with actionable guidance.
4. Surface evidence footprint warnings (≥20 MB/≥25 MB) inside the TUI, highlighting the directories most responsible.

## Non-Goals
- Implementing interactive drill-down panes or real-time auto-refresh (defer for a follow-up iteration).
- Replacing guardrail telemetry schema versions or relocating evidence directories.
- Building CLI or JSON export parity; this work focuses solely on the TUI experience.

## Scope & Assumptions
- Dashboard remains markdown-style to fit the 1024×768 Codex viewport and 80×24 fallback, using concise tables and callouts.
- Aggregation must degrade gracefully: missing guardrail or consensus telemetry should surface as "pending" instead of crashing.
- Evidence directories may be large; implementation should avoid full rescans by selecting the latest artifact per stage.

## Functional Requirements & Acceptance Criteria

| ID | Requirement | Acceptance Criteria / Validation |
| --- | --- | --- |
| R1 | TUI dashboard lists packet scaffolding status (PRD/spec/plan/tasks) and SPEC.md tracker note. | Fixture missing docs results in "⚠ missing" callouts; unit test `spec_status::packet_health`. |
| R2 | Per-stage rows show baseline result, consensus verdict/conflicts, and updated timestamps. | Integration test `spec_status_integration.rs` renders ✅/⚠ with timestamps from fixtures. |
| R3 | Dashboard highlights stale telemetry when latest artifact exceeds configurable age threshold. | Stale fixture triggers "⚠ stale" badge; unit test verifies threshold logic. |
| R4 | Evidence footprint banners warn at ≥20 MB (⚠) and ≥25 MB (❌) with top offending directories. | Oversized fixture triggers warning; integration test snapshots confirm banner content. |
| R5 | Agent participation summary lists models contributing to latest consensus. | Integration test confirms agent list derived from consensus artifacts; manual screenshot captured. |
| R6 | Documentation updated so operators understand the cues and next steps. | Diffs to `docs/slash-commands.md` and operator note reviewed; doc lint passes. |

## Telemetry & Evidence Handling
- Aggregator reads guardrail payloads from `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/<SPEC-ID>/`, selecting the latest baseline artifact per stage.
- Consensus artifacts come from `.../evidence/consensus/<SPEC-ID>/` (Gemini, Claude, GPT outputs plus synthesis) to capture verdict/conflict data and agent participation.
- Staleness thresholds compare artifact timestamps against a configurable window (default 24 h); stale results surface as warnings in the TUI.
- Evidence footprint is computed by summing the relevant directories in Rust; warnings include the heaviest subdirectories so operators know where to prune.
- No CLI or JSON export is required; the Rust module returns structured data directly to the TUI renderer.

## Dependencies
- `codex-rs/tui` for rendering, slash-command plumbing, and unit testing harness.
- Guardrail helpers in `scripts/spec_ops_004/common.sh` for consistent telemetry locations and baseline metadata.
- Telemetry schema references (`docs/SPEC-KIT-013-telemetry-schema-guard/spec.md`, `docs/spec-kit/telemetry-schema-v2.md`).
- Local-memory context for documenting remediation and operational playbooks.

## Risks & Mitigations
- **Telemetry drift:** Schema changes could break parsing → introduce version detection, refresh fixtures, and add CI coverage for new fields.
- **Performance:** Large evidence directories may slow status generation → short-circuit once latest artifacts are discovered and memoize results per invocation.
- **Concurrent writes:** Guardrail runs may update telemetry mid-read → treat partially read files as `pending` and surface a retry hint.
- **TUI layout overflow:** Dense data can exceed viewport → prioritise concise tables, truncating long paths while offering tooltips in follow-up work.

## Validation Plan
1. `cargo test -p codex-tui spec_status::*` covering telemetry parsing, stage cue scoring, packet health checks, footprint thresholds, and stale logic.
2. Run `/spec-status SPEC-KIT-DEMO` (healthy) and fixture-driven failing/stale/oversized scenarios to confirm rendered cues and guidance.
3. Update docs, then run `python3 scripts/spec-kit/lint_tasks.py` and `scripts/doc-structure-validate.sh --mode=templates --dry-run` before merge.
4. Capture representative TUI screenshots for healthy, stale, and oversized evidence states, storing them under SPEC-KIT-035 evidence.

## Success Metrics
- `/spec-status` renders within 2 seconds on typical telemetry sizes (<5k files).
- Each stage row displays a concrete cue (`✅ passed`, `⚠ failed/stale`, `⏳ pending`) when telemetry exists.
- Evidence warnings prompt cleanup before directories exceed 25 MB for more than one guardrail cycle (tracked via Spec Ops retrospectives).
- Operators clear stale or missing telemetry in ≤1 follow-up command informed by the dashboard.
- Packet scaffolding issues (missing docs or tracker entries) are resolved within one guardrail cycle after the dashboard calls them out.

## Documentation & Rollout
- Update `docs/slash-commands.md` and `docs/spec-kit/spec-status-diagnostics.md` with dashboard interpretation guidance.
- Provide fixture instructions under `docs/spec-kit/` so contributors can reproduce healthy/stale scenarios locally.
- Announce availability in release notes and store screenshots/fixtures under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-035/`.

## Open Questions
1. What default staleness threshold balances timely alerts with noisy warnings (24 h vs 12 h vs configurable per SPEC)?
2. Should evidence thresholds stay fixed at 20/25 MB or become configurable per SPEC/area?
3. Should we surface delta comparisons when consecutive runs flip between pass/fail, or reserve that for a later iteration?
4. Do we need export hooks for external dashboards, or is the TUI view sufficient for operators?
