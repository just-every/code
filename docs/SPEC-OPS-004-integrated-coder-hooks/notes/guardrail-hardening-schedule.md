# Guardrail Hardening Execution Schedule (T20)

Date prepared: 2025-09-29
Owner: Code

| Window (UTC) | Activity | Leads | Dependencies | Deliverables |
| --- | --- | --- | --- | --- |
| Sep 29 (Mon) 16:00–18:00 | Kick-off huddle (T20 step 1) to walk through baseline/ HAL failure fixes, confirm manifest flag defaults, and assign implementers | Code (guardrails), Gemini (build), Claude (telemetry) | HAL service confirmed healthy; spec/plan reviewed | Meeting notes + Jira tickets for retrofit tasks |
| Sep 30 (Tue) 15:00–20:00 | Implement baseline enforcement + HAL failure propagation patches; add `SPEC_OPS_CARGO_MANIFEST` + GraphQL fix | Code (guardrails), Gemini (build) | Kick-off decisions | Draft PR with guardrail script changes + regression harness |
| Oct 1 (Wed) 14:00–18:00 | Telemetry extension (`hal.summary`) behind flag + validator updates | Claude (telemetry), Code (guardrails) | Guardrail patch PR open | PR with schema updates + validation evidence |
| Oct 2 (Thu) 16:00–19:00 | HAL degraded/healthy evidence capture, rerun `/spec-ops-plan` + `/spec-ops-validate`; prep rollout comms | Gemini (build), Claude (telemetry), Code (ops) | Guardrail + telemetry PRs merged | Evidence JSON/logs, rollout memo, updated docs/slash-commands.md + AGENTS.md |
| Oct 3 (Fri) 15:00–17:00 | Cross-project sync with T18/T14 owners, verify blockers cleared, hand over integration checkpoints | Code, Claude, Gemini + downstream owners | Evidence + docs staged | Action items list, SPEC.md updates, go/no-go decision |

**Next Actions**
- Send calendar invites for the Sep 29 kick-off (include HAL service access instructions and PR templates).
- Spin up dedicated branch `feat/t20-guardrail-hardening` for implementation tasks.
- Prepare checklist for each step (baseline failure scenario, HAL offline scenario, doc refresh) to be signed off during the Oct 3 sync.
