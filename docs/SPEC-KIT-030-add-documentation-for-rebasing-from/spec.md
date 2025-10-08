# Spec: Fork Rebase Documentation & Nightly Drift Validation (T46)

## Context
- Fork branch `feat/spec-auto-telemetry` regularly diverges from upstream `master`, and existing guidance in `FORK_DEVIATIONS.md` does not give operators a repeatable rebase runbook.
- Nightly drift detection via `scripts/spec-kit/nightly_sync_detect.py` is available but undocumented; guardrail owners lack clarity on required inputs, evidence paths, and escalation triggers.
- Constitution requirements call for evidence-driven documentation that keeps SPEC.md and telemetry artefacts in sync with guardrail automation.

## Objectives
1. Publish an evidence-backed fork assessment and rebase guide that preserves fork-specific instrumentation and references guardrail slash commands.
2. Define and document a nightly drift workflow that detects divergence from upstream master, captures telemetry, and surfaces actionable alerts.
3. Establish expectations for HAL capture, evidence storage, and SPEC.md task governance whenever rebases or nightly checks run.

## Approach
- Catalogue fork-specific code regions (`// === FORK-SPECIFIC`) and evidence locations referenced in `FORK_DEVIATIONS.md`, then fold them into a reusable pre-rebase checklist.
- Produce a numbered rebase flow that covers pre-flight validation, guarded rebase execution, and post-run guardrail verification using `/spec-ops-*` commands.
- Document a nightly automation harness that invokes `python3 scripts/spec-kit/nightly_sync_detect.py`, exports JSON reports, and files artefacts under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-030/`.
- According to Byterover memory layer, documentation guardrails demand template alignment, so the spec mandates running `scripts/doc-structure-validate.sh --mode=templates` and `python3 scripts/spec-kit/lint_tasks.py` as part of the rollout.

## Telemetry & Evidence
- Nightly drift reports and dry-run rebases must archive telemetry JSON, logs, and HAL summaries (when `SPEC_OPS_TELEMETRY_HAL=1`) beneath this SPEC ID.
- SPEC.md updates require dated notes referencing stored evidence; tracker lint must pass after every status change.
- Guardrail validations (`/spec-ops-plan`, `/spec-ops-auto`, `/spec-evidence-stats`) should be recorded with session IDs per SPEC-OPS telemetry schema v1.

## Risks & Mitigations
- **Documentation rot.** Mitigate via quarterly review of `FORK_DEVIATIONS.md` and nightly drift outputs; add reminders in tasks.md.
- **False positives in nightly drift.** Introduce allowlists for known noisy artefacts and document override procedure.
- **HAL unavailability.** Provide fallback instructions (skip with note) while maintaining telemetry expectations.

## Open Questions
- Preferred execution venue for nightly drift (existing CI vs. dedicated cron) and notification channel for failures.
- Whether to snapshot upstream commit hashes in documentation to improve triage accuracy.
- Appetite for automated PR creation when drift is detected vs. manual reconciliation only.
