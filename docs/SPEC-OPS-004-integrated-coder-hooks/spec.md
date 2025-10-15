# Spec: Guardrail Hardening (T20)

## Context
- Guardrail automation scripts (`scripts/spec_ops_004/*`) are swallowing baseline failures, masking HAL smoke errors, and assuming the Rust workspace root.
- Evidence captured on 2025-09-28 (`docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-018/`) shows healthy vs degraded HAL runs still reporting success.
- T18 HAL integration and T14 documentation refresh both depend on hardened guardrails.

## Objectives
1. Enforce baseline audit outcomes so `/spec-ops-plan` fails when audits detect issues.
2. Ensure `/spec-ops-validate` and `/spec-ops-audit` propagate HAL smoke failures with non-zero exits and accurate telemetry.
3. Make guardrail scripts resilient to repository layout changes (manifest-awareness) and fix malformed GraphQL payloads.
4. Extend telemetry with optional HAL summary data without breaking schema v1 consumers.
5. Document new flags/env vars and rollout plan for teams consuming guardrail automation.

## Scope
- Bash script updates under `scripts/spec_ops_004/` (common helpers + command wrappers).
- Telemetry schema adjustments gated by environment flags.
- Evidence capture for both failed and successful HAL smoke runs.
- Documentation and rollout guidance (slash commands, AGENTS.md, runbooks, SPEC.md).

## Non-Goals
- Building a permanent HAL mock service (may be considered later).
- Altering guardrail prompts outside of telemetry fields.
- Replacing existing validation hooks beyond the identified fixes.

## Task Breakdown (2025-09-28)
### Task Slices
- **Guardrail engineer (Code)** – Retrofit `baseline_audit.sh`/`spec_ops_plan.sh` to propagate failures, add `--allow-fail`, and emit `baseline.status="failed"`.
- **Guardrail engineer (Code)** – Rework `spec_ops_run_hal_smoke` + callers to track per-endpoint success, prevent empty artifacts, and fail scenarios on degraded HAL checks.
- **Build engineer (Gemini)** – Introduce `SPEC_OPS_CARGO_MANIFEST` defaults, update all guardrail scripts with `--manifest-path`, fix GraphQL JSON escaping, and add regression coverage.
- **Telemetry analyst (Claude)** – Implement optional `hal.summary` payload gated via `SPEC_OPS_TELEMETRY_HAL`, extend validators (`scripts/spec-kit/lint_tasks.py`) while preserving schema v1 compatibility.
- **Rollout lead (Code)** – Refresh docs/slash-commands.md, AGENTS.md, and guardrail runbooks with new flags/fail behaviors, coordinate announcements to T18/T14 owners, and capture new HAL evidence (failed + healthy).

**Dependencies**
- Access to HAL service (healthy + induced failure) to validate telemetry changes.
- Agreement with T18 owners on strict failure handling and override expectations.
- CI capacity to run new regression tests.

**Validation**
- `/spec-ops-plan SPEC-KIT-018` with forced baseline failure vs `--allow-fail` override.
- `/spec-ops-validate SPEC-KIT-018` with HAL offline to confirm `hal.summary.status="failed"` and non-zero exit.
- Targeted regression test covering GraphQL payload success and manifest-path usage.
- `SPEC_OPS_TELEMETRY_HAL=1` run followed by `python3 scripts/spec-kit/lint_tasks.py`.
- `scripts/doc-structure-validate.sh --mode=templates --dry-run` before documentation handoff.

**Docs**
- `scripts/spec_ops_004/` README/runbooks, `docs/slash-commands.md`, `AGENTS.md`, SPEC.md task row, change announcements for dependent teams.

**Risks & Assumptions**
- Stricter exits may break existing automation; mitigation via documented overrides + staged rollout.
- Telemetry consumers may lag in adopting `hal.summary`; optional flag allows gradual enablement.
- CI without HAL access may require skip flag; assume `SPEC_OPS_HAL_SKIP` remains available.

**Consensus**
- Agreement (Claude/Gemini/Code): Baseline and HAL failures must halt pipelines; manifest awareness and telemetry extensions are mandatory.
- Divergence: Operations voice requested warning-only rollout; consensus is to enforce failures by default while publicizing overrides and providing grace period guidance.
- Degraded participation: Only GPT-5 Codex provided direct synthesis; follow-up cross-agent review recommended before execution.

## Open Questions
- Should `SPEC_OPS_TELEMETRY_HAL` default to enabled post-rollout or remain opt-in pending consumer readiness?
- Is a lightweight HAL mock required for CI environments lacking the real service?
