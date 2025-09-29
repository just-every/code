# Tasks: T18 HAL HTTP MCP Integration (2025-09-29)

| Order | Task | Owner | Status | Validation |
| --- | --- | --- | --- | --- |
| 1 | Verify guardrail patches (baseline + HAL failure propagation) on branch `feat/t20-guardrail-hardening` | Code | Blocked (requires T20 Step 1 complete) | `/spec-ops-plan SPEC-KIT-018` forced failure, telemetry check |
| 2 | Finalize HAL config/profile templates with manifest-aware guidance | Gemini | Pending | Manual review of `docs/hal/hal_config.toml.example` & `docs/hal/hal_profile.json` |
| 3 | Capture HAL degraded evidence (`/spec-ops-validate SPEC-KIT-018` with HAL offline) and archive under docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-018/ | Gemini | Pending | Command exit status !=0, telemetry `hal.summary.status="failed"` |
| 4 | Capture HAL healthy evidence and archive alongside degraded run | Gemini | Pending | Command exit status 0, telemetry `hal.summary.status="passed"` |
| 5 | Update docs/prompts and SPEC tracker with new evidence references | Claude | Pending | `scripts/doc-structure-validate.sh --mode=templates`, `python3 scripts/spec-kit/lint_tasks.py` |
