# Tasks: T26 SPEC-KIT-DEMO Guardrail Baseline

| Order | Task | Owner | Status | Validation |
| --- | --- | --- | --- | --- |
| 1 | T26-DEMO-1: Sync docs/SPEC-KIT-DEMO/{spec.md,plan.md,tasks.md} and SPEC.md row T26 with 2025-10-12 evidence bundle; rerun `python3 scripts/spec-kit/lint_tasks.py` | Code | Done | `python3 scripts/spec-kit/lint_tasks.py > docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-DEMO/lint_tasks_2025-10-12T17-22-21Z.txt`; SPEC.md row T26 references the same evidence |
| 2 | T26-DEMO-2: Refresh halt-gating consensus bundle via `/spec-plan --consensus-exec SPEC-KIT-DEMO --goal "halt gating validation"` and store halt screenshot | Code | In Progress | docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-DEMO/ holds 2025-10-12 per-agent JSON, synthesis.json, telemetry.jsonl, and screenshot saved under docs/SPEC-OPS-004-integrated-coder-hooks/evidence/screenshots/SPEC-KIT-DEMO/ |
| 3 | T26-DEMO-3: Run HAL HTTP MCP smoke checks with `SPEC_OPS_TELEMETRY_HAL=1` or record skip via `SPEC_OPS_HAL_SKIP=1` | Code | Blocked (HAL secret) | HAL outputs committed in docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-DEMO/hal_* or skip rationale noted in docs/SPEC-KIT-DEMO/{plan.md,tasks.md} and SPEC.md |
| 4 | T26-DEMO-4: Assign owner and capture halt screenshot artefact aligned to 2025-10-12 bundle | Code | Backlog | PNG stored under docs/SPEC-OPS-004-integrated-coder-hooks/evidence/screenshots/SPEC-KIT-DEMO/20251012* and referenced in docs/SPEC-KIT-DEMO/tasks.md |
| 5 | T26-DEMO-5: Update docs/SPEC-KIT-DEMO/{plan.md,tasks.md} with telemetry filenames, HAL outcome, evidence footprint; refresh SPEC.md notes | Code | Backlog | `git diff` shows docs referencing 2025-10-12 telemetry + HAL status; `scripts/spec_ops_004/evidence_stats.sh` output logged |

> Degraded lineup: gpt_pro and gpt_codex endpoints offline; Gemini + Claude + GPT-5 Codex synthesized this stage and absorbed QA responsibilities.

> Sandbox note: When rehearsing the evidence listing steps, launch the TUI with `--sandbox danger-full-access` to avoid Landlock panics (`linux_run_main.rs:28`). Capture a console snippet confirming the roster once the four-model lineup is restored.

## HAL MCP Fallback Strategy (Draft)
- Evidence captured on 2025-10-05 remains the latest successful HAL run:
  - `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-DEMO/20251005T030032Z-hal-health.txt`
  - `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-DEMO/20251005T030041Z-hal-list_movies.txt`
  - `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-DEMO/20251005T030052Z-hal-graphql.txt`
- When `HAL_SECRET_KAVEDARR_API_KEY` is missing, run `SPEC_OPS_HAL_SKIP=1 bash scripts/spec_ops_004/commands/spec_ops_validate.sh SPEC-KIT-DEMO`, capture the skip note in docs/SPEC-KIT-DEMO/{plan.md,tasks.md} and SPEC.md, and queue a rerun once credentials land.
- After credentials arrive, rerun the HAL templates, add `hal_summary` telemetry (with `SPEC_OPS_TELEMETRY_HAL=1`), and update evidence references plus timestamps across docs/SPEC-KIT-DEMO/{plan.md,tasks.md} and SPEC.md T26 notes.
