# Tasks: SPEC-KIT-045-mini Rehearsal

| Order | Task | Owner | Status | Validation |
| --- | --- | --- | --- | --- |
| 1 | Capture four-agent roster evidence from plan guardrail (03:39:12Z run) | Code | Pending | Confirm `spec-plan_2025-10-13T03:39:12Z-92930885.json` includes gemini/claude/gpt_pro/gpt_codex with model metadata; export roster summary to docs/SPEC-KIT-045-mini/telemetry/roster.json |
| 2 | Assert schema v1 fields for plan + tasks telemetry | Code | Pending | `jq` checks for `command`, `specId`, `sessionId`, `timestamp`, `schemaVersion`, stage fields (baseline.*, tool.status) on plan/tasks telemetry JSON |
| 3 | Stage mock HAL diff workflow | Code | Pending | `SPEC_OPS_ALLOW_DIRTY=1 SPEC_OPS_TELEMETRY_HAL=1 bash scripts/spec_ops_004/commands/spec_ops_validate.sh SPEC-KIT-045-mini` then `jq -S` diff vs `telemetry/sample-validate.json`; archive diff log |
| 4 | Document policy override mitigation | Code | Pending | Update unlock-notes.md with SPEC_OPS_POLICY_* override usage and produce checklist to rerun plan/validate without stubs |
| 5 | Refresh checksums & size report for fixture evidence | Code | Pending | Regenerate `docs/SPEC-KIT-045-mini/checksums.sha256` and append size output (`du -chs docs/SPEC-KIT-045-mini`) with timestamp |
| 6 | Sync docs & tracker after validations | Code | Pending | Rewrite tasks.md with results, ensure plan.md/spec.md reference evidence path, run `python3 scripts/spec-kit/lint_tasks.py`, update SPEC.md T49 notes |

> Keep telemetry + docs under 100 KB; note that guardrail runs so far used `SPEC_OPS_ALLOW_DIRTY=1` and `SPEC_OPS_POLICY_*_CMD=true`.
> This rehearsal run generated placeholder documentation for tasks 4–5 (including `telemetry/mock-hal.diff`); statuses remain Pending until a clean run without policy stubs verifies live evidence.
