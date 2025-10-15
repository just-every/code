# Local Memory Notes

## 2025-10-12 Policy Prefilter: SPEC-KIT-045-mini/spec-plan
- According to Byterover memory layer, earlier prefilter runs flagged that the mini plan’s acceptance mapping and work breakdown drifted from the rehearsal spec; this remains unresolved.
- Plan row for "Hold path documented" (docs/SPEC-KIT-045-mini/plan.md line 19) still points to `docs/spec-kit/systematic-testing-guide.md`, which does not exist in this repository.
- Spec acceptance criterion 3 requires fixture docs to reference evidence filenames and rerun instructions, but the plan focuses on `/spec-ops-unlock` and external docs instead of updating the local README/tasks.
- From Byterover memory tools and today’s re-check (2025-10-13), the plan now contains the consensus section, yet the acceptance mapping row continues to reference the missing `docs/spec-kit/systematic-testing-guide.md`; policy prefilter remains blocked until docs inside `docs/SPEC-KIT-045-mini/` call out `telemetry/sample-validate.json` and rerun instructions directly.
