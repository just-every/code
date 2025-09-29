# Tasks: T14 Documentation Refresh (2025-09-29)

| Order | Task | Owner | Status | Validation |
| --- | --- | --- | --- | --- |
| 1 | Preflight with T18/T20 owners; capture fresh HAL healthy/degraded telemetry before editing | Claude | Blocked (waiting on T20 Step 1) | Meeting notes + updated evidence timestamps in docs/SPEC-OPS-004-integrated-coder-hooks/evidence/ |
| 2 | Refresh `docs/slash-commands.md` with telemetry schema v1 fields, guardrail flags, and model metadata guidance | Claude | Pending | Manual diff review + link check against docs/SPEC-KIT-013-telemetry-schema-guard/spec.md |
| 3 | Update `AGENTS.md` guardrail section with telemetry envelope + evidence instructions | Claude | Pending | Manual review + `scripts/spec-kit/lint_tasks.py` |
| 4 | Expand onboarding/troubleshooting (docs/getting-started.md, RESTART.md) with HAL validation flow | Gemini | Pending | `scripts/doc-structure-validate.sh --mode=templates` (dry-run) |
| 5 | Update SPEC tracker row (T14) with final evidence links and rerun doc lint | Code | Pending | `python3 scripts/spec-kit/lint_tasks.py`, SPEC.md diff |
