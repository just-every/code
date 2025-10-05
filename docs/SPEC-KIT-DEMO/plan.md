# Plan: SPEC-KIT-DEMO Guardrail Baseline (T26)
## Inputs
- Spec: docs/SPEC-KIT-DEMO/spec.md (retrieved 2025-10-05, commit HEAD)
- Constitution: memory/constitution.md (v1.1, amended 2025-09-28)

## Work Breakdown
1. Align docs/SPEC-KIT-DEMO/{spec.md,plan.md,tasks.md} and SPEC.md row T26 before reruns; capture lint output for evidence.
2. Execute `/spec-plan --consensus-exec SPEC-KIT-DEMO --goal "validating command exec"` with conflict prompts and SPEC_KIT_TELEMETRY_ENABLED=1 to produce halt telemetry.
3. Run HAL HTTP MCP templates (health/list/graphql) if credentials are available; record SPEC_OPS_TELEMETRY_HAL decision and archive outputs.
4. Refresh docs/SPEC-KIT-DEMO/tasks.md with the new evidence status, rerun checklist, and documented adversarial prompt pair for follow-up stages.

## Acceptance Mapping
| Requirement (Spec) | Validation Step | Test/Check Artifact |
| --- | --- | --- |
| R1: Docs + tracker ready | `python3 scripts/spec-kit/lint_tasks.py`; verify SPEC.md row T26 timestamp and notes | SPEC.md diff + lint output |
| R2: Conflict halt validated | `/spec-plan --consensus-exec SPEC-KIT-DEMO --goal "validating command exec"` (expect non-zero exit) | docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-DEMO/spec-plan_*_telemetry.jsonl |
| R3: Evidence archived & documented | Archive HAL runs (if available) and update spec/plan/tasks with evidence + prompt pair | docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-DEMO/hal_* + docs diffs |

## Risks & Unknowns
- Consensus prompts may converge; prepare adversarial variants from local-memory and rerun if halt fails.
- HAL credentials might be absent; document the skip and plan follow-up if `HAL_SECRET_KAVEDARR_API_KEY` is missing.
- Evidence path mismatches can drop telemetry; pre-create the consensus directory and record CLI logs.

## Consensus & Risks (Multi-AI)
- Agreement: Gemini e5a0fbe5-0780-4ec6-8569-169effd001d8, Claude 5345211d-5a22-4277-9945-32e64c6cd638, and GPT-5 fa36d440-ac6f-459b-8638-6f9120462384 align on sequencing: doc alignment → conflict run → HAL evidence → task refresh.
- Disagreement & resolution: Gemini flagged HAL configuration uncertainty; GPT-5 lists it under missing items. Treat as an open action to confirm credentials before execution.

## Exit Criteria (Done)
- `python3 scripts/spec-kit/lint_tasks.py` passes with SPEC.md row T26 referencing the new evidence timestamp and adversarial prompt note.
- docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-DEMO/ holds ≥2025-10-05 bundle (per-agent JSON, synthesis.json, telemetry.jsonl) from a conflicting run.
- HAL telemetry decision captured (run or documented skip) with evidence links mirrored across spec/plan/tasks.
- docs/SPEC-KIT-DEMO/tasks.md updated with the consensus prompt reference, HAL follow-up, and rerun checklist statuses.
