# Tasks: SPEC-KIT-045-mini Rehearsal

Anchors
- Plan guardrail (2025-10-14T15:58:30Z): `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-045-mini/spec-plan_2025-10-14T15:58:30Z-79323873.{json,log}`
- Tasks guardrail (2025-10-14T16:19:16Z): `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-045-mini/spec-tasks_2025-10-14T16:19:16Z-1583422877.{json,log}`
- Prompt version: `docs/spec-kit/prompts.json` → `spec-tasks` `20251002-tasks-a`

| Order | Task | Owner | Status | Validation |
| --- | --- | --- | --- | --- |
| 1 | Evidence catalog refresh | Code | Done | Verified telemetry anchors with `ls` and recorded summary in `docs/SPEC-KIT-045-mini/telemetry/anchors_2025-10-14T16-19-16Z.md`. |
| 2 | Four-agent roster export | Code | Done | `jq` extraction captured roster -> `docs/SPEC-KIT-045-mini/telemetry/roster_2025-10-14T15:58:30Z.json`. |
| 3 | Schema v1 assertions | Code | Done | `jq` checks stored in `plan-schema-check_2025-10-14T15:58:30Z.txt` and `tasks-schema-check_2025-10-14T16:19:16Z.txt`. |
| 4 | HAL mock rehearsal prep | Code | Done | Mock diff documented at `docs/SPEC-KIT-045-mini/telemetry/mock-hal_2025-10-14T16:19:16Z.diff`. |
| 5 | Policy override documentation | Code | Done | Override log + follow-ups in `docs/SPEC-KIT-045-mini/telemetry/policy-overrides_2025-10-14T16:19:16Z.txt` and unlock-notes entry appended. |
| 6 | Fixture hygiene (<100 KB) | Code | Done | `du -chs docs/SPEC-KIT-045-mini` logged to `fixture-size_2025-10-14T16:19:16Z.txt`; `checksums.sha256` regenerated. |
| 7 | SPEC.md tracker sync (T49) | Code | Done | SPEC.md row T49 updated; lint evidence stored at `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-045-mini/lint_tasks_2025-10-14T16-19-16Z.txt`. |
| 8 | Consensus archive | Code | Done | Stage synthesis saved to `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-045-mini/spec-implement_synthesis.json`. |

Notes
- Maintain fixture + evidence footprint under 100 KB (see latest size log above).
- Rerun guardrails without policy overrides for clean unlock; update policy notes when completed.
- SPEC.md update must preserve single `In Progress` row and cite lint/log artefacts created above.
