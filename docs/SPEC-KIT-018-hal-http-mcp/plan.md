# Plan: T18 HAL HTTP MCP Integration
## Inputs
- Spec: docs/SPEC-KIT-018-hal-http-mcp/spec.md (3f3c34f0)
- Constitution: memory/constitution.md (8bcab66e)

## Work Breakdown
1. **Guardrail prerequisites (blocker sync).** Pair with T20 owners to land guardrail fixes: ensure `spec_ops_run_hal_smoke` propagates non-zero exits, GraphQL payload escaping is corrected, and `SPEC_OPS_CARGO_MANIFEST`/`--manifest-path` is honored. Re-run `/spec-ops-plan SPEC-KIT-018` and `/spec-ops-validate SPEC-KIT-018` against a known-bad HAL to verify telemetry now reports failure and records `hal.summary`.
2. **HAL configuration assets.** Author final templates `docs/SPEC-KIT-018-hal-http-mcp/hal_config.toml.example` and `docs/SPEC-KIT-018-hal-http-mcp/hal_profile.json` capturing health, movie list, indexer test, and GraphQL ping endpoints. Document secret usage (`HAL_SECRET_KAVEDARR_API_KEY`) and instructions for copying these assets into the product repo (~/kavedarr/docs/hal/...).
3. **Evidence capture workflow.** With guardrail fixes in place, execute `/spec-ops-validate SPEC-KIT-018` twice: once with HAL offline (expect failure) and once with HAL healthy. Store artifacts under docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-018/ and annotate each telemetry payload with scenario notes.
4. **Documentation & prompt refresh.** Update docs/slash-commands.md, AGENTS.md, and docs/getting-started.md to describe HAL smoke integration, evidence directories, manifest overrides, and consensus model metadata. Ensure `/spec-*` guidance references HAL prerequisites and the need for degraded vs healthy evidence snapshots.
5. **Tracker & validation.** Update SPEC.md row T18 with evidence paths and status, run `scripts/spec-kit/lint_tasks.py`, perform a doc validation dry-run (`scripts/doc-structure-validate.sh --mode=templates --dry-run` prior to full run), and capture review notes for handoff.

## Acceptance Mapping
| Requirement (Spec) | Validation Step | Test/Check Artifact |
| --- | --- | --- |
| HAL MCP entry registered and working | `cargo run --manifest-path codex-rs/Cargo.toml -p codex-mcp-client --bin call_tool -- --tool http-get --args '{"url":"http://127.0.0.1:7878/health"}' -- npx -y hal-mcp` succeeds with healthy HAL | docs/hal/hal_config.toml.example, command output |
| HAL evidence stored under SPEC-KIT-018 | Inspect docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-018/* for paired failed/passed telemetry JSON | Evidence JSON files |
| `/spec-*` flows document HAL usage | Review docs/slash-commands.md & AGENTS.md diffs for HAL guidance and telemetry schema reminders | Updated docs |
| SPEC tracker updated with notes | Modify SPEC.md row T18 with status + evidence links and run `scripts/spec-kit/lint_tasks.py` | SPEC.md diff, lint output |

## Risks & Unknowns
- Guardrail hardening (T20) must complete before evidence is trustworthy; any delay blocks Step 3.
- Local Kavedarr API availability can destabilize validation; consider lightweight mock for CI if outages persist.
- API key rotation relies on operator process; missing runbook updates could expose secrets.
- Differences between template repo and product repo layouts require clear copy instructions to avoid configuration drift.

## Consensus & Risks (Multi-AI)
- Agreement: Claude (docs architect), Gemini (planner), and Code (risk) all insist on sequencing guardrail fixes before new evidence and on providing degraded + healthy HAL runs with updated documentation.
- Disagreement & resolution: Gemini initially proposed pausing T18 entirely until T20 lands; consensus reached to proceed but treat Step 1 as an explicit blocker and track the dependency before moving forward.

## Exit Criteria (Done)
- Guardrail fixes verified via failed HAL run producing `hal.summary.status="failed"` telemetry.
- HAL templates committed and referenced in operator docs with clear copy/secret instructions.
- Healthy and degraded HAL evidence captured and linked in SPEC.md.
- Docs/slash-commands.md, AGENTS.md, and onboarding guidance updated; lint/validation scripts pass.
