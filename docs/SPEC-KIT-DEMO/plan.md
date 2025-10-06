# Plan: SPEC-KIT-DEMO Halt Gating Validation (T26)
## Inputs
- Spec: docs/SPEC-KIT-DEMO/spec.md (commit 1a495650ef6538f71a8bdc38afd1823b2e2c21d0)
- Constitution: memory/constitution.md (v1.1, amended 2025-09-28)

## Work Breakdown
1. Reconcile docs/SPEC-KIT-DEMO/{spec.md, tasks.md} and SPEC.md row T26 so acceptance criteria, evidence filenames, and tracker notes reflect the upcoming halt run.
2. Execute `/spec-plan --consensus-exec SPEC-KIT-DEMO --goal "halt gating validation"` (or equivalent guardrail wrapper) to capture a fresh conflict bundle with per-agent JSON, synthesis.json, and telemetry.jsonl.
3. Run HAL HTTP MCP templates (health/list_movies/graphql) via `cargo run -p codex-mcp-client --bin call_tool -- --tool … -- npx -y hal-mcp` when credentials exist, or document the skip in plan/tasks with rationale.
4. Update docs/SPEC-KIT-DEMO/plan.md and tasks.md with telemetry filenames, halt screenshot status, and HAL outcomes so tasks.md Step 2 can move to Done and residual actions are logged.

## Acceptance Mapping
| Requirement (Spec) | Validation Step | Test/Check Artifact |
| --- | --- | --- |
| R1: Docs + tracker aligned | `python3 scripts/spec-kit/lint_tasks.py` and review SPEC.md T26 notes | SPEC.md diff + lint output |
| R2: Halt gating validated | `/spec-plan --consensus-exec SPEC-KIT-DEMO --goal "halt gating validation"` | docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-DEMO/spec-plan_*_telemetry.jsonl |
| R3: HAL evidence recorded | HAL HTTP MCP templates (or documented skip) | docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-DEMO/hal_* |

## Risks & Unknowns
- HAL_SECRET_KAVEDARR_API_KEY might be unavailable; if so, record the skip and schedule the rerun once provided.
- Consensus prompts may not trigger a conflict; keep adversarial prompt variants on hand and rerun until halt gating is demonstrated.
- Telemetry filenames could drift if evidence directories differ; pre-verify paths before updating SPEC.md notes.
- Halt screenshot ownership is still open; confirm who captures it during the evidence run.

## Consensus & Risks (Multi-AI)
- Agreement: Gemini bc83ced5-abac-4222-a8f3-83c89158d5dd, Claude 50816001-f146-48de-a45e-bcd6f9e198bc, and GPT-5 305a6766-81ac-4609-9574-a628e04a6726 align on sequencing: docs/tracker sync → halt run → HAL validation → doc updates.
- Disagreement & resolution: Minor uncertainty remains about HAL credential availability and halt screenshot assignment; track decisions in tasks.md until resolved.

## Exit Criteria (Done)
- `python3 scripts/spec-kit/lint_tasks.py` passes and SPEC.md T26 notes cite the new telemetry bundle filename.
- docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-DEMO/ contains a ≥2025-10-05 conflict bundle (per-agent JSON, synthesis.json, telemetry.jsonl) from the halt run.
- HAL templates executed (or skip documented) with outputs linked from plan/tasks and SPEC.md notes.
- docs/SPEC-KIT-DEMO/tasks.md updated with halt screenshot status and any remaining follow-up actions.
