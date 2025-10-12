# Spec: Systematic Testing Framework for Spec-Auto Orchestrator (SPEC-KIT-045)

## Context
Spec-Auto currently requires long manual runs to validate guardrail stages, consensus agents, and telemetry bundles. Recent orchestrator upgrades demand a faster, fixture-driven regression suite that can be run locally, in CI, and before `/spec-auto` automation kicks off.

## Objectives
- Deliver stage-isolated harnesses that exercise plan, tasks, implement, validate, audit, and unlock without full pipeline execution.
- Keep fixtures lightweight (<100 KB) and regenerable so developers can iterate rapidly.
- Validate agent spawning, telemetry schema v1 payloads, and evidence directory structure with deterministic results.

## Acceptance Criteria
- Stage rehearsal playbooks executed via TUI (`/spec-ops-*` and `/spec-auto --from <stage>`) confirm guardrail exits, consensus artefacts, and agent metadata per stage.
- Error injections (guardrail failure, missing telemetry, agent dropout) triggered through documented environment flags halt runs with actionable summaries and evidence diffs.
- Documentation describes how to maintain fixtures, execute the fast rehearsal workflow, and capture artefacts for SPEC tracker updates without a 90-minute run.

## Task Breakdown Snapshot (2025-10-11)
- **Agent spawn remediation**: audit and fix Gemini/Claude/GPT Pro/GPT Codex launching behaviour; log model metadata per stage.
- **Fixture kit**: maintain sub-100 KB SPEC-KIT-045-mini fixtures plus telemetry snapshots with regeneration script.
- **Stage harnesses**: implement Rust tests that drive `spec_ops_<stage>.sh` individually, asserting acceptance criteria, resumable flows, and evidence writes.
- **File & telemetry validation**: harness-level manifest diffing and schema checks for guardrail outputs, handling timestamp variance.
- **Error suite**: harness scenarios for guardrail exits, missing telemetry, agent dropouts, and HAL outages.
- **Fast-run documentation**: publish guide, troubleshooting playbook, CI wiring, and tracker evidence updates.

## Open Risks / Questions
- GPT Pro / GPT Codex availability may block live validation; need fallback plan if outages persist.
- Telemetry schema changes can invalidate fixtures—must regenerate alongside validator updates.
- HAL dependency strategy (mock vs live smoke) requires decision before finalize error suite.
