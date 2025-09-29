# Tasks: T20 Guardrail Hardening (2025-09-29)

| Order | Task | Owner | Status | Validation |
| --- | --- | --- | --- | --- |
| 1 | Retrofit baseline enforcement (`baseline_audit.sh`, `spec_ops_plan.sh`) with `--allow-fail` override | Code | Scheduled (Sep 30) | `/spec-ops-plan SPEC-KIT-018` failing audit + telemetry `baseline.status="failed"` |
| 2 | Propagate HAL smoke failures, prevent empty artifacts, fail scenarios on degraded HAL | Code | Scheduled (Sep 30) | `/spec-ops-validate SPEC-KIT-018` with HAL offline (non-zero exit, telemetry failure) |
| 3 | Add `SPEC_OPS_CARGO_MANIFEST` support, update scripts with `--manifest-path`, fix GraphQL escaping | Gemini | Scheduled (Sep 30) | Guardrail logs showing `--manifest-path codex-rs/Cargo.toml`; GraphQL smoke passes |
| 4 | Implement optional `hal.summary` telemetry + validator updates | Claude | Scheduled (Oct 1) | `SPEC_OPS_TELEMETRY_HAL=1` run + `python3 scripts/spec-kit/lint_tasks.py` |
| 5 | Capture HAL evidence (failed + healthy) after fixes and refresh docs/slash-commands.md & AGENTS.md | Claude & Gemini | Scheduled (Oct 2) | Evidence JSON/logs, doc diff, `scripts/doc-structure-validate.sh --mode=templates` |
| 6 | Cross-project sync (T14/T18) and update SPEC.md / rollout memo | Code | Scheduled (Oct 3) | Meeting notes, SPEC.md updates, confirmation of enforcement date |
