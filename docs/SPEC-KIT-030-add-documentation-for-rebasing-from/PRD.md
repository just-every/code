# PRD: SPEC-KIT-030 Add Documentation for Rebasing from Fork Main Branch

## Summary
This spec codifies how just-every/code documents fork rebasing against upstream master while supplying nightly drift validation. It balances evidence-driven guardrails (per the constitution) with tooling-first guidance drawn from `FORK_DEVIATIONS.md`, `docs/slash-commands.md`, and `scripts/spec-kit/nightly_sync_detect.py`.

## Users & Goals
- Spec Kit maintainers – maintain accurate operational runbooks for upstream drift management.
- Release and branch operators – execute repeatable rebases without losing fork-specific instrumentation.
- Nightly automation owners – surface deviations or regressions caused by upstream merges before they block guardrail runs.

## Requirements
1. **R1 – Fork assessment playbook**  
   Document how to review fork-only changes and telemetry artifacts before attempting a rebase (inventory, diff, validation checkpoints).  
   **Acceptance Conditions:**  
   - Documentation references `FORK_DEVIATIONS.md`, identifies `// === FORK-SPECIFIC` sentinels, and outlines commands such as `git log upstream/master ^master`.  
   - `scripts/doc-structure-validate.sh --mode=templates` passes for the new documentation folder.
2. **R2 – Rebase workflow guidance**  
   Provide a numbered rebase procedure that uses documented slash commands (`/spec-ops-plan`, `/spec-ops-auto`, `/spec-evidence-stats`) and records required telemetry envelopes.  
   **Acceptance Conditions:**  
   - Guide covers preparation (branch hygiene, SPEC.md lock state), execution (rebasing feat/* against `master`), and post-run validation using guardrail commands.  
   - Dry-run evidence path (`docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-030/`) is referenced for operators to attach artifacts.
3. **R3 – Nightly drift detection coverage**  
   Describe how nightly automation will call `scripts/spec-kit/nightly_sync_detect.py` against exported local-memory snapshots and guardrail evidence to flag drift from upstream master.  
   **Acceptance Conditions:**  
   - Nightly job location (CI workflow or cron script) is specified, including required inputs (`tmp/memories.jsonl`, evidence root) and exit expectations (non-zero on drift).  
   - Documented workflow captures telemetry (artifact path, JSON report) and references SPEC Ops telemetry requirements.
4. **R4 – Evidence and validation reporting**  
   Ensure operators know how to store drift outcomes, HAL captures (when `SPEC_OPS_TELEMETRY_HAL=1`), and update SPEC.md task notes after each rebase cycle.  
   **Acceptance Conditions:**  
   - Documentation lists evidence storage locations, HAL enablement guidance, and SPEC.md update expectations (status, dated notes).  
   - `python3 scripts/spec-kit/lint_tasks.py` passes after SPEC.md updates associated with this SPEC.

## Non-Goals
- Automating upstream merges or force-pushing rebases without human review.
- Replacing existing guardrail scripts with new tooling.
- Extending HAL endpoint coverage beyond current SPEC-KIT-018 scope.

## Dependencies
- `FORK_DEVIATIONS.md` for canonical fork delta inventory.
- `docs/slash-commands.md` for guardrail slash command semantics.
- `scripts/spec-kit/nightly_sync_detect.py` for drift detection logic.
- `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/` telemetry structure and SPEC.md task governance.

## Rollout Metrics
- Documentation lint (`scripts/doc-structure-validate.sh --mode=templates`) and SPEC task lint (`python3 scripts/spec-kit/lint_tasks.py`) succeed post-publication.
- First nightly drift run produces a JSON report with `drift_detected=false` and archives results under this SPEC ID.
- Operators record at least one dated SPEC.md note referencing the published workflow within two rebases.
- No guardrail stage fails due to missing telemetry fields during pilot rebases.

## Open Questions
- Where should the nightly job run (existing CI workflow vs. new cron harness) to balance visibility and cost?
- Should we snapshot upstream commit hashes within the documentation to simplify regression triage?
- Do we need an allowlist for known noisy evidence files in the nightly drift detector before rollout?
