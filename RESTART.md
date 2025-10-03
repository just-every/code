# Restart Plan: Spec Kit Multi-Agent Pipeline

## Status
- Guardrail enforcement (T20) is merged. Slash commands now cover the automation: `/spec-ops-plan`, `/spec-ops-tasks`, `/spec-ops-implement`, `/spec-ops-validate`, `/spec-ops-audit`, `/spec-ops-unlock`, plus `/spec-ops-auto` (guardrail sequence) and `/spec-evidence-stats` (telemetry footprint).
- HAL MCP integration (T18) is complete: templates reference `HAL_SECRET_KAVEDARR_API_KEY`, and healthy/degraded telemetry lives under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-018/` (e.g., `spec-validate_2025-09-29T16:25:38Z-2828521850.json`, `spec-validate_2025-09-29T16:34:21Z-229132461.json`).
- Documentation refresh (T14) is still open; `docs/getting-started.md` is the canonical troubleshooting and HAL workflow.
- Current branch: `feat/spec-auto-telemetry` (ahead of origin).

## Validation Commands
- `/spec-evidence-stats` — confirm repo footprint and consensus artefact counts.
- `/spec-ops-auto SPEC-KIT-018 --from plan` — guardrail smoke (creates telemetry JSON/logs under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-018/`).
- `cargo run -p codex-mcp-client --bin call_tool -- --tool http-get --args '{"url":"http://127.0.0.1:7878/health"}' -- npx -y hal-mcp` — HAL MCP helper sanity check (only needed if the higher-level slash command fails).

## Next Steps
1. **Docs alignment (T14 follow-up)** — Audit `docs/slash-commands.md`, `AGENTS.md`, `docs/getting-started.md`, and cross-links to `docs/spec-kit/model-strategy.md`; ensure troubleshooting references prompt-version evidence and the new slash commands.
2. **Smoke the new wrappers** — In the TUI run `/spec-evidence-stats` and `/spec-ops-auto SPEC-KIT-018 --from plan`; confirm telemetry/artifact handling is correct.
3. **Push branch when ready** — `git status -sb`, confirm only intentional diffs, then `git push origin feat/spec-auto-telemetry`.

## Next Session Prompt
```
/spec-evidence-stats
/spec-ops-auto SPEC-KIT-018 --from plan
/spec-plan SPEC-KIT-018 Align HAL prompts
```

## Telemetry & Consensus Troubleshooting

- **Schema failures:** Inspect the latest guardrail JSON under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/<SPEC-ID>/`. Ensure common fields (`command`, `specId`, `sessionId`, `timestamp`, `schemaVersion`, `artifacts`) and stage payload (baseline/tool/lock/scenarios/unlock) match docs/SPEC-KIT-013-telemetry-schema-guard/spec.md. Export `SPEC_OPS_TELEMETRY_HAL=1` when capturing HAL smoke so `hal.summary` is present for downstream checks.
- **Degraded consensus:** Re-run the affected `/spec-*` stage with higher thinking budgets (`/spec-plan --deep-research`, escalate to `gpt-5` with `--thinking`). Verify model metadata (`model`, `model_release`, `reasoning_mode`) is present in agent responses (see docs/spec-kit/model-strategy.md).
- **Evidence drift:** Run `/spec-ops-plan` and `/spec-ops-validate` again to refresh artifacts, then re-run `/spec-auto`. Nightly T15 sync should report any lingering mismatches. Ensure both healthy and degraded HAL JSON artifacts (e.g., `20250929-114636Z-hal-health.json` vs `20250929-114708Z-hal-health.json`) remain checked in for SPEC-KIT-018.
