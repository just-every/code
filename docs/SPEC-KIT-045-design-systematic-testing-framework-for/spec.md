# Spec: Systematic Testing Framework for Spec-Auto Orchestrator (SPEC-KIT-045)

## Context
Spec-Auto currently requires long manual runs to validate guardrail stages, consensus agents, and telemetry bundles. Recent orchestrator upgrades demand a faster, fixture-driven regression suite that can be run locally, in CI, and before `/spec-auto` automation kicks off.

## Objectives
- Deliver stage-isolated harnesses that exercise plan, tasks, implement, validate, audit, and unlock without full pipeline execution.
- Keep fixtures lightweight (<100 KB) and regenerable so developers can iterate rapidly.
- Validate agent spawning, telemetry schema v1 payloads, and evidence directory structure with deterministic results.

## Acceptance Criteria
- Stage runners and Rust integration tests confirm guardrail exits, consensus artefacts, and agent metadata for each stage.
- Error injections (guardrail failure, missing telemetry, agent dropout) halt runs with actionable summaries and evidence diffs.
- Documentation describes how to regenerate fixtures, run the suite locally/CI, and capture artefacts for SPEC tracker updates.
