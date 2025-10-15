# Plan: SPEC-KIT-DEMO Guardrail Baseline (T26)
## Inputs
- Spec: docs/SPEC-KIT-DEMO/spec.md (commit 804d120199f39f80a3e2f954402c91cb690d9eeb)
- Constitution: memory/constitution.md (v1.1, amended 2025-09-28)

## Work Breakdown
1. Reconcile docs/SPEC-KIT-DEMO/{plan.md,tasks.md} and SPEC.md row T26 with the 2025-10-12 guardrail outputs (`spec-plan_2025-10-12T14:26:32Z-1976726666.json` bundle) and re-run `python3 scripts/spec-kit/lint_tasks.py` to confirm tracker integrity. *(Completed 2025-10-12 – see `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-DEMO/lint_tasks_2025-10-12T17-22-21Z.txt`.)*
2. Execute `/spec-plan --consensus-exec SPEC-KIT-DEMO --goal "halt gating validation"` (or the guardrail wrapper) to capture fresh per-agent JSON, synthesis.json, telemetry.jsonl, and the halt screenshot under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-DEMO/`. Launch the TUI with `--sandbox danger-full-access` so Landlock does not block evidence listings.
3. Run HAL HTTP MCP smoke checks (`health`, `list_movies`, `graphql_ping`) with `SPEC_OPS_TELEMETRY_HAL=1`; if `HAL_SECRET_KAVEDARR_API_KEY` is still unavailable, set `SPEC_OPS_HAL_SKIP=1`, log the skip rationale in docs and SPEC.md, and queue the rerun trigger.
4. Update docs/SPEC-KIT-DEMO/{plan.md,tasks.md} with telemetry filenames, halt screenshot ownership, HAL outcome, and follow-up actions; advance tasks to Done once evidence is referenced and open items noted.
5. Run `scripts/spec_ops_004/evidence_stats.sh docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-DEMO` and document the footprint (<25 MB) alongside any pruning steps required.

## Acceptance Mapping
| Requirement (Spec) | Validation Step | Test/Check Artifact |
| --- | --- | --- |
| R1: Docs + tracker aligned | `python3 scripts/spec-kit/lint_tasks.py` and review SPEC.md row T26 notes | SPEC.md T26 diff + lint output |
| R2: Halt gating telemetry captured | `/spec-plan --consensus-exec SPEC-KIT-DEMO --goal "halt gating validation"` | docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-DEMO/spec-plan_*_{telemetry.jsonl,synthesis.json,per-agent.json} + halt screenshot |
| R3: HAL evidence recorded or skip documented | `SPEC_OPS_TELEMETRY_HAL=1 bash scripts/spec_ops_004/commands/spec_ops_validate.sh SPEC-KIT-DEMO` (or skip note) | docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-DEMO/hal_* artifacts or documented skip |
| R4: Docs reference evidence + follow-ups closed | Manual review of docs/SPEC-KIT-DEMO/{plan.md,tasks.md} and SPEC.md notes | Updated docs showing filenames, timestamps, owners, and outstanding actions |

## Risks & Unknowns
- `HAL_SECRET_KAVEDARR_API_KEY` may remain unavailable, delaying Step 3 until credentials arrive.
- Consensus prompts might not trigger a conflict; adversarial prompt variants may be required to capture the halt screenshot.
- Telemetry schema v1 validation can fail and halt the pipeline if evidence paths drift.
- Halt screenshot ownership is still unassigned and must be confirmed before closing Step 4.
- Evidence size could exceed the 25 MB soft limit without pruning, risking CI slowdowns.

## Consensus & Risks (Multi-AI)
- Agreement: Gemini (research), Claude Sonnet (synthesis), and GPT-5 Codex (validator) aligned on refreshing telemetry with the 2025-10-12 guardrail run, documenting HAL outcomes, and recording evidence footprint data before advancing.
- Disagreement & resolution: GPT-Pro and GPT-Codex classic endpoints were unavailable; remaining agents absorbed QA duties, noted the workspace pathing blocker as out-of-scope for this SPEC, and recorded the degraded lineup in telemetry.

## Exit Criteria (Done)
- All acceptance checks pass (lint, consensus run, HAL execution or documented skip, evidence stats recorded).
- Docs updated: SPEC.md T26 notes, docs/SPEC-KIT-DEMO/{plan.md,tasks.md} reference 2025-10-12 evidence and halt screenshot ownership.
- Telemetry bundle (`spec-plan_2025-10-12T14:26:32Z-1976726666.*`) and HAL evidence (or skip rationale) linked in docs and SPEC.md notes.
