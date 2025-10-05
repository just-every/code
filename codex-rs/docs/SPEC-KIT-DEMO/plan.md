# Plan: SPEC-KIT-DEMO
## Inputs
- Spec: docs/SPEC-KIT-DEMO/spec.md (missing as of 2025-10-05; obtain canonical copy or draft with assumptions tracked)
- Constitution: memory/constitution.md (not present in repo on 2025-10-05; request authoritative text before implementation)

## Work Breakdown
1. Rebuild docs/SPEC-KIT-DEMO/{spec.md,tasks.md} from Spec Kit templates, capturing provenance and acceptance traceability in each document.
2. Execute `/spec-plan SPEC-KIT-DEMO --consensus-exec --allow-conflict` once docs exist, archiving telemetry (including HAL HTTP MCP output or degraded logs) under docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-DEMO/.
3. Add SPEC.md tracker row for SPEC-KIT-DEMO and run `python3 scripts/spec-kit/lint_tasks.py` to confirm guardrails.

## Acceptance Mapping
| Requirement (Spec) | Validation Step | Test/Check Artifact |
| --- | --- | --- |
| R1: Documentation scaffold rebuilt | Manual template compliance review | docs/SPEC-KIT-DEMO/spec.md |
| R2: Tracker hygiene | `python3 scripts/spec-kit/lint_tasks.py` | docs/SPEC-OPS-004-integrated-coder-hooks/evidence/lint_tasks.log |
| R3: Consensus telemetry fresh | `/spec-plan SPEC-KIT-DEMO --consensus-exec --allow-conflict` run with HAL capture (or documented fallback) | docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-DEMO/ |

## Risks & Unknowns
- Canonical SPEC-KIT-DEMO requirements source still missing; need upstream doc owner to unblock drafting or clearly label assumptions.
- HAL HTTP MCP availability is unverified; degraded evidence path must be agreed if service remains offline.
- SPEC.md tracker table not yet present; lint may fail until the row and schema are restored.

## Consensus & Risks (Multi-AI)
- Agreement: Gemini (afe033f0-e564-4b3f-824f-74f9567aaab9), Claude (78445b4a-d922-48f0-9027-134cf2f32805), and GPT-5 (5a300047-117e-4d2b-b7e6-294a16fd2671) align on rebuilding documentation, capturing consensus telemetry, and restoring tracker lint as the three critical steps.
- Disagreement & resolution: All agents flag HAL availability as unresolved; proceed with plan but capture degraded telemetry evidence and escalate Infra-CI status before implementation freeze.

## Exit Criteria (Done)
- docs/SPEC-KIT-DEMO/{spec.md,plan.md,tasks.md} present with provenance notes and acceptance traceability.
- Consensus-conflict run produces evidence bundle and HAL status archived under docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-DEMO/.
- SPEC.md tracker row added, lint passes, and results documented in evidence folder.
