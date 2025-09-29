# Tasks: T18 HAL HTTP MCP Integration (2025-09-29)

| Order | Task | Owner | Status | Validation |
| --- | --- | --- | --- | --- |
| 1 | Verify guardrail patches (baseline + HAL failure propagation) on branch `feat/t20-guardrail-hardening` | Code | Done (2025-09-29) | `/spec-ops-plan SPEC-KIT-018` forced failure + healthy rerun (`spec-plan_2025-09-29T14:54:20Z-20962766.json`, `spec-validate_2025-09-29T11:47:08Z-325419396.json`, `spec-validate_2025-09-29T14:54:35Z-3088619300.json`) |
| 2 | Finalize HAL config/profile templates with manifest-aware guidance | Gemini | Done (2025-09-29) | Manual review of `docs/hal/hal_config.toml.example` & `docs/hal/hal_profile.json` |
| 3 | Capture HAL degraded evidence (`/spec-ops-validate SPEC-KIT-018` with HAL offline) and archive under docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-018/ | Gemini | Done (2025-09-29) | Command exit status !=0, telemetry `hal.summary.status="failed"` (`20250929-114708Z-hal-*.json`) |
| 4 | Capture HAL healthy evidence and archive alongside degraded run | Gemini | Done (2025-09-29) | Command exit status 0, telemetry `hal.summary.status="passed"` (`20250929-114636Z-hal-*.json`) |
| 5 | Update docs/prompts and SPEC tracker with new evidence references | Claude | Done (2025-09-29) | `scripts/doc-structure-validate.sh --mode=templates`, `python3 scripts/spec-kit/lint_tasks.py`, SPEC.md row T18 updated |
