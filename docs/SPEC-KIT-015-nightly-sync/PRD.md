# PRD: Nightly Sync Drift Detector (T15)

## Summary
- **Objective.** Detect when local-memory entries and guardrail telemetry evidence drift out of sync so `/spec-auto` audits remain trustworthy.
- **Problem.** Local-memory is now the primary context store, but operators may forget to mirror telemetry evidence into memory or remove stale entries. We lack an automated check to highlight divergence.
- **Outcome.** A nightly job (or manual command) compares local-memory facts against evidence logs (`docs/SPEC-OPS-004-integrated-coder-hooks/evidence/`) and reports mismatches with actionable remediation steps.

## Users & Jobs
- Spec Kit operators want a simple status report indicating which SPEC IDs have stale or missing memory entries relative to evidence artifacts.
- Maintainers need an audit trail clarifying when the last sync occurred and what drift was detected.

## Goals
1. Define a deterministic comparison between local-memory entries (JSONL) and evidence artifacts (telemetry JSON, logs).
2. Provide a CLI script/tool that outputs drift findings (missing, extra, or outdated entries) and optional JSON for automation.
3. Document runbook steps so operators can schedule the check nightly and remediate drift.

## Non-Goals
- Rewriting local-memory storage format.
- Deleting or rewriting evidence files automatically; tool reports issues but does not mutate data.
- Integrating with external schedulers (CI/CD) beyond documentation.

## Requirements
| ID | Description | Acceptance |
| --- | --- | --- |
| R1 | Tool scans local-memory domains relevant to Spec Kit (spec-tracker, docs-ops, telemetry) and compares timestamps/IDs against evidence artifacts. | Mismatches are listed with clear labels (e.g., `missing_memory`, `missing_evidence`, `stale_memory`). |
| R2 | Supports dry-run (default) and `--apply` options to generate suggested fixes (e.g., template for `local-memory remember`). | Dry-run output includes recommended commands; apply mode writes a JSON checklist. |
| R3 | Nightly run should produce a summary report or exit code >0 when drift detected. | Script exit code indicates success (0) or drift (>0). |
| R4 | Documentation explains scheduling (cron/GitHub Action) and remediation flow. | Updates to docs/spec-kit or RESTART.md with instructions. |

## Dependencies & Risks
- Accurate local-memory export assumed (needs `local-memory export` command or direct file access).
- Evidence directory can grow large; tool should stream/aggregate without loading everything into memory.
- Potential false positives if evidence files are rotated; need clear path to mark expected differences.

## Rollout & Success Metrics
- Implement as a Rust or Python tool under `scripts/spec-kit/`. Run manually first, then integrate into nightly automation.
- Success measured by nightly drift report running clean once memory/evidence are aligned.
- Document deployment in SPEC tracker notes.

