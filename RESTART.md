# Restart Plan: Spec Kit Multi-Agent Pipeline

## Status
- Guardrail enforcement (T20) remains stable. Slash commands cover the automation: `/spec-ops-plan`, `/spec-ops-tasks`, `/spec-ops-implement`, `/spec-ops-validate`, `/spec-ops-audit`, `/spec-ops-unlock`, plus `/spec-ops-auto` (guardrail sequence) and `/spec-evidence-stats` (telemetry footprint).
- HAL MCP integration (T18) is fully landed; healthy/degraded telemetry samples live under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-018/` (e.g., `spec-validate_2025-09-29T16:25:38Z-2828521850.json`, `spec-validate_2025-09-29T16:34:21Z-229132461.json`).
- Multi-agent consensus automation (T21/T23) is active: consensus runner prompts exist and TUI hook work has begun (guarded by `SPEC_KIT_TELEMETRY_ENABLED`).
- Current branch: `feat/spec-auto-telemetry` (ahead of origin).

## Validation Commands
- `/spec-evidence-stats` — confirm repo footprint and consensus artefact counts.
- `/spec-ops-auto SPEC-KIT-018 --from plan` — guardrail smoke (creates telemetry JSON/logs under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-018/`).
- `cargo run -p codex-mcp-client --bin call_tool -- --tool http-get --args '{"url":"http://127.0.0.1:7878/health"}' -- npx -y hal-mcp` — HAL MCP helper sanity check (only needed if the higher-level slash command fails).

## Next Steps
1. **Finish T23 (Spec-kit telemetry hook)** — Enable `SPEC_KIT_TELEMETRY_ENABLED=1`, capture agent outputs after `/spec-plan --consensus`, and persist per-agent JSON + telemetry line file + synthesis into `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/<SPEC-ID>/`.
2. **Implement T24 (Consensus synthesis & halt)** — Add synthesis helper in ChatWidget, mark `/spec-auto` as halted when consensus degrades/conflicts, and surface evidence pointer in history.
3. **Create tests for T25** — Add TUI integration coverage for happy/conflict/missing-agent flows, then run an E2E SPEC to confirm evidence creation.
4. **Docs alignment (T14 follow-up)** — Re-sync `docs/slash-commands.md`, `AGENTS.md`, `docs/getting-started.md`, and `docs/spec-kit/model-strategy.md` once telemetry hook is stable.

## Next Session Prompt
```
/set-env SPEC_KIT_TELEMETRY_ENABLED=1
/spec-plan --consensus SPEC-CONSENSUS-E2E "Draft telemetry hook MVP"
/review docs/spec-kit/telemetry-schema-v2.md
```

## Telemetry & Consensus Troubleshooting

- **Schema failures:** Inspect the latest guardrail JSON under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/<SPEC-ID>/`. Ensure common fields (`command`, `specId`, `sessionId`, `timestamp`, `schemaVersion`, `artifacts`) and stage payload (baseline/tool/lock/scenarios/unlock) match docs/SPEC-KIT-013-telemetry-schema-guard/spec.md. Export `SPEC_OPS_TELEMETRY_HAL=1` when capturing HAL smoke so `hal.summary` is present for downstream checks.
- **Degraded consensus:** Re-run the affected `/spec-*` stage with higher thinking budgets (`/spec-plan --deep-research`, escalate to `gpt-5` with `--thinking`). Verify model metadata (`model`, `model_release`, `reasoning_mode`) is present in agent responses (see docs/spec-kit/model-strategy.md).
- **Evidence drift:** Run `/spec-ops-plan` and `/spec-ops-validate` again to refresh artifacts, then re-run `/spec-auto`. Nightly T15 sync should report any lingering mismatches. Ensure both healthy and degraded HAL JSON artifacts (e.g., `20250929-114636Z-hal-health.json` vs `20250929-114708Z-hal-health.json`) remain checked in for SPEC-KIT-018.
