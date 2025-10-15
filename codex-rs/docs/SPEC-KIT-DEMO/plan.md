# Plan: SPEC-KIT-DEMO
## Inputs
- Spec: docs/SPEC-KIT-DEMO/spec.md (to be drafted via Step 1; assumptions documented per memory 60e0b909-2f7c-4107-b8b6-cc2b60cd3418)
- Constitution: memory/constitution.md (authoritative copy requested; capture once owners provide)

## Work Breakdown
1. Rebuild docs/SPEC-KIT-DEMO/spec.md and docs/SPEC-KIT-DEMO/tasks.md from Spec Kit templates with provenance notes.
2. Execute `/spec-plan SPEC-KIT-DEMO --consensus-exec --allow-conflict` with HAL HTTP MCP (or degraded evidence) and archive outputs under docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-DEMO/.
3. Restore SPEC.md tracker row for SPEC-KIT-DEMO and run `python3 scripts/spec-kit/lint_tasks.py`, storing the lint log under docs/SPEC-OPS-004-integrated-coder-hooks/evidence/.
4. Refresh docs/SPEC-KIT-DEMO/plan.md exit criteria and summary references to the new evidence bundle.

## Acceptance Mapping
| Requirement (Spec) | Validation Step | Test/Check Artifact |
| --- | --- | --- |
| R1: Documentation scaffold rebuilt | Manual template compliance review | docs/SPEC-KIT-DEMO/spec.md |
| R2: Tracker hygiene | `python3 scripts/spec-kit/lint_tasks.py` | docs/SPEC-OPS-004-integrated-coder-hooks/evidence/lint_tasks.log |
| R3: Consensus telemetry fresh | `/spec-plan SPEC-KIT-DEMO --consensus-exec --allow-conflict` run with HAL capture (or documented fallback) | docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-DEMO/ |

## Risks & Unknowns
- Canonical spec owner not yet identified; escalate before implementation to confirm assumptions.
- HAL HTTP MCP availability uncertain; degraded telemetry path (manual curl) must remain ready.
- SPEC.md schema drift may cause lint failures; regenerate table structure if scripts flag inconsistencies.

## Consensus & Risks (Multi-AI)
- Agreement: Gemini 60e0b909-2f7c-4107-b8b6-cc2b60cd3418, Claude 1ee9ffa1-303f-4251-9bdb-4229fa445b9a, and GPT-5 26c555e4-e3de-4bfa-b34e-a43e0976dce5 align on rebuilding missing docs, refreshing HAL telemetry, and restoring tracker hygiene as the minimal viable sequence.
- Disagreement & resolution: All agents flag HAL availability as unresolved; plan proceeds with degraded capture fallback (b8cb4529-9e5e-4659-93cc-28a9672b1a76) and infra escalation trigger if service stays offline.

## Exit Criteria (Done)
- docs/SPEC-KIT-DEMO/{spec.md,plan.md,tasks.md} present with assumption notes and evidence links.
- Consensus run produces HAL capture (or degraded log) archived under docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-DEMO/.
- SPEC.md tracker row restored, lint passes, and evidence/log stored.
