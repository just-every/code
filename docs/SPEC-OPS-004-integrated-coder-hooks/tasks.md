# Tasks: T20 Guardrail Hardening (2025-09-29)

| Order | Task | Owner | Status | Validation |
| --- | --- | --- | --- | --- |
| 1 | Retrofit baseline enforcement (`baseline_audit.sh`, `spec_ops_plan.sh`) with `--allow-fail` override | Code | Done (2025-09-29) | Telemetry `spec-plan_2025-09-29T16:23:24Z-2625014190.json` (baseline.status="failed"), override logged in `spec-plan_2025-09-29T16:23:09Z-1240129600.log` |
| 2 | Propagate HAL smoke failures, prevent empty artifacts, fail scenarios on degraded HAL | Code | Done (2025-09-29) | `/spec-ops-validate` exit 1 + telemetry `spec-validate_2025-09-29T16:25:38Z-2828521850.json` (hal.summary.status="failed") |
| 3 | Add `SPEC_OPS_CARGO_MANIFEST` support, update scripts with `--manifest-path`, fix GraphQL escaping | Gemini | Done (2025-09-29) | Logs capture manifest override: `spec-plan_2025-09-29T16:23:09Z-1240129600.log`; HAL telemetry uses new capture guard |
| 4 | Implement optional `hal.summary` telemetry + validator updates | Claude | Done (2025-09-29) | `SPEC_OPS_TELEMETRY_HAL=1` run; telemetry + validator tests passing (`spec-validate_2025-09-29T14:54:35Z-3088619300.json`, `cargo test -p codex-tui spec_auto`) |
| 5 | Capture HAL evidence (failed + healthy) after fixes and refresh docs/slash-commands.md & AGENTS.md | Claude & Gemini | Done (2025-09-29) | Evidence JSON/logs (`20250929-114636Z`, `20250929-114708Z`, `20250929-123303Z`, `20250929-123329Z`), doc diff, `scripts/doc-structure-validate.sh --mode=templates` |
| 6 | Cross-project sync (T14/T18) and update SPEC.md / rollout memo | Code | Done (2025-09-29) | Decision recorded in `notes/guardrail-hardening.md`; enforcement live for local runs, CI opt-in deferred |
