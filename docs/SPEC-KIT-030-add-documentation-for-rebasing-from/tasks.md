# Tasks: SPEC-KIT-030 Add Documentation for Rebasing from Fork Main Branch

| Order | Task | Owner | Status | Validation |
| --- | --- | --- | --- | --- |
| 1 | Inventory fork deltas and telemetry touchpoints before documenting | Spec Kit maintainers | Backlog | `git log upstream/master ^master`, review `FORK_DEVIATIONS.md`, capture evidence paths |
| 2 | Author rebase assessment + execution guide referencing guardrail commands | Spec Kit documentation owner | Backlog | `scripts/doc-structure-validate.sh --mode=templates` |
| 3 | Define nightly drift detection workflow (inputs, schedule, alerting) | Nightly automation owner | Backlog | `python3 scripts/spec-kit/nightly_sync_detect.py --spec SPEC-KIT-030 --pretty` dry-run |
| 4 | Document telemetry/HAL expectations and SPEC.md update checklist | Spec Kit maintainers | Backlog | `python3 scripts/spec-kit/lint_tasks.py` (post SPEC row update) |
| 5 | Publish adoption evidence and update SPEC.md Tasks table row | Spec Kit maintainers | Backlog | Attach dated SPEC.md note; archive evidence under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-030/` |

## Notes
- Nightly checks can start as a scheduled GitHub Actions workflow referencing `scripts/spec-kit/nightly_sync_detect.py`; capture JSON output under this SPEC ID for telemetry.
- According to Byterover memory layer, documentation guardrails demand template alignment, so every doc change should run `scripts/doc-structure-validate.sh --mode=templates` before merge.
- Document how to enable `SPEC_OPS_TELEMETRY_HAL=1` during validation runs when assessing fork drift to retain HAL artifacts.
- Update SPEC.md’s Tasks table once work begins; ensure `python3 scripts/spec-kit/lint_tasks.py` passes after each status change.
