# Guardrail Hardening Plan (T20)

Date: 2025-09-28
Owner: Code

## Scope
- Tighten SPEC-OPS-004 guardrail scripts around baseline checks, HAL smoke handling, and telemetry gating.
- Resolve regressions introduced after the codex-rs workspace split (cargo manifest lookup, GraphQL payload escaping).

## Observed Gaps
- Baseline audit exit codes are discarded: `spec_ops_plan.sh` swallows failures from `baseline_audit.sh` and stamps telemetry as passed based only on the requested mode.
- HAL smoke runs succeed even on total failure: `spec_ops_run_hal_smoke` logs errors but callers keep `SCENARIO_STATUS="passed"`, so `/spec-ops-validate` continues and emits green telemetry.
- Evidence artifacts go missing or blank when HAL capture fails because the scripts append log entries without checking file creation.
- Guardrail scripts expect `cargo run -p codex-mcp-client` from the repository root, which now fails after the Rust workspace moved to `codex-rs/` (see log `spec-validate_2025-09-28T22:37:43Z-483932008.log`).
- GraphQL capture JSON is malformed (`"body":"{"query":"..."}"`) so the fourth HAL request always errors before reaching the API.

## Proposed Enhancements
- Baseline enforcement
  - Plumb the exit status from `baseline_audit.sh` back into `spec_ops_plan.sh` and mark telemetry `baseline.status` as `failed` when checks fail or produce empty output.
  - Emit a non-zero exit when baseline status is `failed` unless `SPEC_OPS_ALLOW_DIRTY=1` or an explicit `--allow-fail` flag is supplied.
- HAL smoke reliability
  - Add a `hal_status` accumulator in `spec_ops_run_hal_smoke` that tracks per-endpoint success, returning non-zero when any capture fails or falls back to a synthetic body.
  - Surface `hal_status` and failure notes in the telemetry payload for Validate/Audit so `/spec-auto` can gate on it.
  - Only append HAL artifact paths when the capture succeeds and the destination file exists.
- Workspace awareness
  - Introduce `SPEC_OPS_CARGO_MANIFEST` (defaulting to `codex-rs/Cargo.toml`) and pass `--manifest-path` to every `cargo run`, making guardrails resilient to repo layout changes.
- GraphQL payload fix
  - JSON-escape the GraphQL body when formatting `graphql_args` so the MCP helper receives valid JSON and records the actual API error body.
- Telemetry schema extensions
  - Add optional `hal.summary` fields (`status`, `failed_checks`, `artifacts`) while preserving schema v1 compatibility by wrapping them under a new object guarded by flag `SPEC_OPS_TELEMETRY_HAL=1`.

## Task Breakdown
1. Patch `spec_ops_run_hal_smoke` and callers to propagate failure counts, update telemetry builders, and exit non-zero when HAL fails under strict mode.
2. Modify `spec_ops_plan.sh` to respect baseline exit codes and document the new CLI flags in `docs/slash-commands.md`.
3. Introduce manifest-path configuration and update all guardrail scripts (plan/tasks/implement/validate/audit/unlock) to use it.
4. Fix GraphQL JSON escaping and add a regression test (shell or rust integration) covering healthy vs. unhealthy HAL responses.
5. Extend telemetry structs or serializers (if needed) so `/spec-auto` halts when it receives `hal_status=failed`, then update the Spec Kit telemetry validator.

## Validation & Evidence
- Re-run `/spec-ops-plan`, `/spec-ops-validate`, and `/spec-auto` against SPEC-KIT-018 with HAL healthy and unhealthy to confirm telemetry transitions (`passed` vs `failed`).
- Capture new evidence under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-018/` showing:
  - Healthy baseline captures (`20250929-114636Z-hal-*`)
  - Forced failure window (`20250929-114708Z-hal-*`)
  - Telemetry with `hal.summary` enabled in both degraded and healthy modes (`spec-validate_2025-09-29T12:33:03Z-3193628696.json`, `spec-audit_2025-09-29T12:33:29Z-218285443.json`, `spec-validate_2025-09-29T14:54:35Z-3088619300.json`) referencing the new artifact sets (`20250929-123303Z-hal-*`, `20250929-123329Z-hal-*`, `20250929-145435Z-hal-*`).
- Update `scripts/spec-kit/lint_tasks.py` outputs to ensure the new telemetry fields still lint cleanly.

## Rollout Checklist (2025-09-29)
- **Telemetry flag staging:** Document `SPEC_OPS_TELEMETRY_HAL=1` across slash-command, AGENTS, and onboarding guides. Keep the flag opt-in until CI parity is confirmed, then flip the default in guardrail scripts.
- **CI parity:** Ensure CI runners export `SPEC_OPS_CARGO_MANIFEST=codex-rs/Cargo.toml` and have HAL (or a mock) reachable. Schedule a dry run of the guardrail harness with the flag enabled before enforcing failure exits.
- **Rollout memo:** Share this note plus SPEC.md updates with T14/T18 owners. Highlight the paired evidence sets (`20250929-114636Z-hal-*`, `20250929-114708Z-hal-*`) and remind teams to keep both healthy/degraded artifacts under version control.

## Open Questions
- Should HAL failures block `/spec-ops-validate` when the product repo intentionally runs without downloaders/indexers? Need confirmation from the HAL integration owners before making strict mode the default.
- Do we need a lightweight mock HAL server for CI to avoid timeouts when the real stack is offline?
