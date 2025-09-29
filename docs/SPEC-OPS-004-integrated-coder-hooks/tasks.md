# Tasks: T20 Guardrail Hardening (2025-09-29)

| Order | Task | Owner | Status | Validation |
| --- | --- | --- | --- | --- |
| 1 | Retrofit baseline enforcement (`baseline_audit.sh`, `spec_ops_plan.sh`) with `--allow-fail` override | Code | Scheduled (Sep 30) | `/spec-ops-plan SPEC-KIT-018` failing audit + telemetry `baseline.status="failed"` |
| 2 | Propagate HAL smoke failures, prevent empty artifacts, fail scenarios on degraded HAL | Code | Scheduled (Sep 30) | `/spec-ops-validate SPEC-KIT-018` with HAL offline (non-zero exit, telemetry failure) |
| 3 | Add `SPEC_OPS_CARGO_MANIFEST` support, update scripts with `--manifest-path`, fix GraphQL escaping | Gemini | Scheduled (Sep 30) | Guardrail logs showing `--manifest-path codex-rs/Cargo.toml`; GraphQL smoke passes |
| 4 | Implement optional `hal.summary` telemetry + validator updates | Claude | Done (2025-09-29) | `SPEC_OPS_TELEMETRY_HAL=1` run; telemetry + validator tests passing (`spec-validate_2025-09-29T14:54:35Z-3088619300.json`, `cargo test -p codex-tui spec_auto`) |
| 5 | Capture HAL evidence (failed + healthy) after fixes and refresh docs/slash-commands.md & AGENTS.md | Claude & Gemini | Done (2025-09-29) | Evidence JSON/logs (`20250929-114636Z`, `20250929-114708Z`, `20250929-123303Z`, `20250929-123329Z`), doc diff, `scripts/doc-structure-validate.sh --mode=templates` |
| 6 | Cross-project sync (T14/T18) and update SPEC.md / rollout memo | Code | In Progress | SPEC.md row updated; rollout checklist drafted; meeting to confirm enforcement date |
