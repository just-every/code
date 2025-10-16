# Unlock Notes: SPEC-KIT-045 Mini

Record the unlock/hold rationale after a rehearsal run of `/guardrail.unlock SPEC-KIT-045-mini`.

Template:
- Date/Time: <UTC timestamp>
- HAL mode: mock|live (if live, reference credentials handling runbook)
- Reason for hold or unlock decision:
- Follow-ups required (if any):
- Evidence references:
  - `docs/SPEC-KIT-045-mini/telemetry/sample-validate.json` (schema example)
  - Any TUI transcript or paths captured during the session

Notes:
- Keep this file concise; the mini bundle must remain under 100 KB.
- Prefer referencing existing sample artifacts rather than copying new large outputs.

## 2025-10-13T03:59:37Z — spec-implement rehearsal

- Date/Time: 2025-10-13T03:59:37Z (UTC)
- HAL mode: mock (SPEC_OPS_ALLOW_DIRTY=1, SPEC_OPS_POLICY_*_CMD=true for rehearsal)
- Reason for hold or unlock decision: Documentation-only run; unlock remains on hold until policy overrides are removed and live evidence is captured.
- Follow-ups required (if any): Rerun plan/validate/unlock without policy stubs; capture four-agent roster JSON and mock HAL diff from live guardrail execution.
- Evidence references:
  - docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-045-mini/spec-implement_2025-10-13T03:59:37Z-2393215615.json
  - docs/SPEC-KIT-045-mini/checksums.sha256 (regenerated 2025-10-13)

## 2025-10-14T16:39:43Z — spec-implement rehearsal

- Date/Time: 2025-10-14T16:39:43Z (UTC)
- HAL mode: mock (SPEC_OPS_ALLOW_DIRTY=1, SPEC_OPS_POLICY_*_CMD=true)
- Reason for hold or unlock decision: Evidence bundle refreshed to 2025-10-14 telemetry but policy overrides remain; unlock continues to hold until clean reruns confirm policy compliance and (optional) live HAL coverage.
- Follow-ups required: Run plan/tasks/validate without policy stubs, capture live HAL summary when credentials available, compare clean telemetry to rehearsal artefacts.
- Evidence references:
  - docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-045-mini/spec-implement_2025-10-14T16:39:43Z-997018070.json
  - docs/SPEC-KIT-045-mini/telemetry/policy-overrides_2025-10-14T16:19:16Z.txt
  - docs/SPEC-KIT-045-mini/telemetry/mock-hal_2025-10-14T16:19:16Z.diff
  - docs/SPEC-KIT-045-mini/telemetry/fixture-size_2025-10-14T16:19:16Z.txt
  - docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-045-mini/spec-validate_2025-10-14T16:58:53Z-971619013.json
  - docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-045-mini/20251014-165853Z-hal-mock.json
  - docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-045-mini/spec-unlock_2025-10-14T17:28:18Z-3087513806.json
