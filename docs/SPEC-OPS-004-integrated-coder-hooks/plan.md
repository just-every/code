# Plan: T20 Guardrail Hardening (SPEC-OPS-004)
## Inputs
- Spec: docs/SPEC-OPS-004-integrated-coder-hooks/notes/guardrail-hardening.md (7122ab3f)
- Constitution: memory/constitution.md (8bcab66e)

## Work Breakdown
1. **Baseline enforcement retrofit.** Update `scripts/spec_ops_004/baseline_audit.sh` and `spec_ops_plan.sh` so baseline failures exit non-zero, populate `baseline.status="failed"`, and honor escape hatches (`SPEC_OPS_ALLOW_DIRTY=1`, new `--allow-fail`). Capture regression tests around skip/full modes.
2. **HAL smoke failure propagation.** Refactor `spec_ops_run_hal_smoke`/`spec_ops_capture_hal` to track per-endpoint status, prevent empty artifacts, and return non-zero when any check falls back. Patch `spec_ops_validate.sh`/`spec_ops_audit.sh` to mark scenarios failed when HAL fails.
3. **Workspace & GraphQL fixes.** Introduce `SPEC_OPS_CARGO_MANIFEST` defaulting to `codex-rs/Cargo.toml` and add `--manifest-path` to every `cargo run`. Correct GraphQL payload escaping and add regression coverage (shell or Rust integration) for healthy/degraded responses.
4. **Telemetry extensions & tests.** Gate an optional `hal.summary` object (`status`, `failed_checks`, `artifacts`) behind `SPEC_OPS_TELEMETRY_HAL=1`, update telemetry validators (`scripts/spec-kit/lint_tasks.py`) and add unit/integration tests ensuring schema v1 compatibility and new fields render when enabled.
5. **Docs, rollout, and coordination.** Update docs/slash-commands.md, AGENTS.md, and guardrail runbooks with new flags/env vars, publish migration notes for T18/T14 owners, stage CI rollout plan (warning window vs strict enforcement), and capture evidence (failed & passed HAL runs) under docs/SPEC-OPS-004-integrated-coder-hooks/evidence/.

## Acceptance Mapping
| Requirement (Spec) | Validation Step | Test/Check Artifact |
| --- | --- | --- |
| R1: Baseline audit failure blocks plan | Force a failing audit and run `/spec-ops-plan SPEC-KIT-018`; expect non-zero exit and telemetry `baseline.status="failed"`; rerun with `--allow-fail` to confirm override | Telemetry JSON + CLI exit status |
| R2: HAL smoke failures surface | Stop HAL service and run `/spec-ops-validate SPEC-KIT-018)`; expect command failure, log entry, and telemetry with `hal.summary.status="failed"` | spec-validate log + evidence JSON |
| R3: Cargo manifest honored | From repo root run `/spec-ops-validate SPEC-KIT-018)`; inspect logs for `cargo run --manifest-path codex-rs/Cargo.toml` and ensure success | Guardrail log snippet |
| R4: GraphQL payload valid | Run HAL smoke with healthy API; inspect `*-hal-graphql_ping.json` for valid response and zero parsing errors | HAL artifact JSON |
| R5: Telemetry extension behind flag | Set `SPEC_OPS_TELEMETRY_HAL=1` and rerun `/spec-ops-validate`; lint via `scripts/spec-kit/lint_tasks.py` to ensure new fields pass schema | Lint output + telemetry JSON |
| R6: Documentation & CI guidance updated | Review docs/slash-commands.md & AGENTS.md diffs; validate doc structure; capture migration notes for dependent tasks | Doc diffs + `scripts/doc-structure-validate.sh --mode=templates` |

## Risks & Unknowns
- HAL availability in CI/local can block validation; document `SPEC_OPS_HAL_SKIP=1` fallback and plan for a mock server follow-up.
- Stricter exits may break downstream automation; stage rollout with communication to T18/T14 owners and allow temporary overrides.
- Telemetry schema changes impact analytics and `/spec-auto`; ensure consumer teams review before enabling `SPEC_OPS_TELEMETRY_HAL` by default.
- Shell changes risk regressions across multiple scripts; require integration tests and dry-run evidence before adoption.

## Consensus & Risks (Multi-AI)
- Agreement: Claude (backend), Gemini (QA/telemetry), and Code (DevOps/security) unanimously prioritize non-zero baseline exits, HAL failure propagation, manifest awareness, GraphQL escaping, and telemetry extensions with opt-in flag.
- Disagreement & resolution: DevOps agent favored phased warning-only rollout; consensus shifted to enforcing by default while documenting overrides and CI grace period to balance reliability with workflow continuity.

## Exit Criteria (Done)
- Baseline and HAL guardrail commands fail appropriately on regressions and pass when healthy.
- `SPEC_OPS_CARGO_MANIFEST`/GraphQL fixes merged with regression tests.
- Telemetry includes `hal.summary` when enabled and validators accept the schema.
- Documentation updated, evidence (failed + healthy HAL runs) refreshed, `scripts/spec-kit/lint_tasks.py` and doc validation pass, and handoff notes delivered to dependent tasks.
