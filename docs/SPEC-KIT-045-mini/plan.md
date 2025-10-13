# Plan: SPEC-KIT-045-mini
## Inputs
- Spec: docs/SPEC-KIT-045-mini/spec.md (sha256 6bcce50a1b5bf14ab8834ef26301fc597538902a0f5eff9db3768022dea79cc3 captured 2025-10-13)
- Constitution: memory/constitution.md (v1.1, sha256 08cc5374d2fedec0b1fb6429656e7fd930948d76de582facb88fd1435b82b515)

## Work Breakdown
1. Reload foundation docs (constitution, product-requirements.md, PLANNING.md, SPEC.md T49) and prompts.json (spec-plan v20251002-plan-a) to confirm scope and acceptance criteria.
2. Run `SPEC_OPS_ALLOW_DIRTY=1 SPEC_OPS_POLICY_PREFILTER_CMD=true SPEC_OPS_POLICY_FINAL_CMD=true bash scripts/spec_ops_004/commands/spec_ops_plan.sh SPEC-KIT-045-mini` to capture telemetry `spec-plan_2025-10-13T03:39:12Z-92930885.json` and baseline digest, noting policy layers were stubbed.
3. Update this plan.md with consensus agent outputs, referencing the 03:39:12Z telemetry run, roster evidence expectations, and policy degradation notes.
4. Queue docs/SPEC-KIT-045-mini/tasks.md refresh so tasks cover roster confirmation, mock HAL `jq -S` diff versus `telemetry/sample-validate.json`, unlock rationale capture, checksum regeneration, and fixture size logging.
5. Stage implement/validate prerequisites: verify sample telemetry schema locally (`jq -S '.' telemetry/sample-validate.json`), prepare comparison script, and ensure unlock-notes.md lists policy override follow-up to run without stubs.
6. Record consensus + risks in docs/SPEC-KIT-045-mini/unlock-notes.md and ensure checksums.sha256 reflects updated evidence references before moving to /tasks.

## Acceptance Mapping
| Requirement (Spec) | Validation Step | Test/Check Artifact |
| --- | --- | --- |
| R1: Four-agent roster logged | Confirm four agent JSON outputs (gemini/claude/gpt_pro/gpt_codex) stored for 2025-10-13T03:39:12Z run | docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-045-mini/spec-plan_2025-10-13T03:39:12Z-92930885*.json |
| R2: Baseline telemetry schema v1 intact | Run jq assertions for baseline.mode=no-run, baseline.status=passed, hooks.session.start present | docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-045-mini/spec-plan_2025-10-13T03:39:12Z-92930885.json |
| R3: Mock HAL rehearsal ready | Diff live validate telemetry against docs/SPEC-KIT-045-mini/telemetry/sample-validate.json and log hal.summary comparison | docs/SPEC-KIT-045-mini/telemetry/sample-validate.json + validation diff log |
| R4: Docs reference evidence and policy override | Ensure plan.md, tasks.md, unlock-notes.md cite 03:39 telemetry path and flag SPEC_OPS_POLICY_* stub usage | docs/SPEC-KIT-045-mini/{plan.md,tasks.md,unlock-notes.md} |

## Risks & Unknowns
- Policy prefilter/final guards were stubbed (`SPEC_OPS_POLICY_*_CMD=true`); a follow-up run without overrides is required before sign-off.
- Fixture docs remain untracked in main repo; agents in sanitized environments will not see them unless staged—track before sharing pipelines.
- Mock HAL mode only; live HAL behaviour may surface schema deltas or credential issues once available.

## Consensus & Risks (Multi-AI)
- Agreement: Gemini, Claude, GPT-Pro, and GPT-Codex outputs aligned on anchoring the plan to telemetry 2025-10-13T03:39:12Z, enforcing four-agent roster evidence, running schema checks, and documenting policy overrides.
- Disagreement & resolution: Agents running in clean sandboxes reported missing fixture docs; local repository confirmed docs exist but are untracked. Resolution: proceed with updates while noting need to stage fixture assets before remote automation.

## Exit Criteria (Done)
- Plan.md updated with latest telemetry references, acceptance mapping, and policy degradation notes.
- docs/SPEC-KIT-045-mini/{tasks.md,unlock-notes.md,checksums.sha256} queued for refresh using new plan guidance.
- Evidence directory docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-045-mini/2025-10-13T03-39-12Z-92930885/ documented for downstream stages and consensus synthesis captured.
