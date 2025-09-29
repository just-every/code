# Restart Plan: Spec Kit Multi-Agent Pipeline

## Status
- Guardrail enforcement (T20) is merged: `/spec-ops-plan` fails on missing baseline output, manifests are resolved via `SPEC_OPS_CARGO_MANIFEST`, and HAL artifacts are recorded only when present.
- HAL MCP integration (T18) is complete: templates reference `HAL_SECRET_KAVEDARR_API_KEY`, and healthy/degraded telemetry lives under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-018/` (e.g., `spec-validate_2025-09-29T16:25:38Z-2828521850.json`, `spec-validate_2025-09-29T16:34:21Z-229132461.json`).
- Documentation refresh (T14) is in flight; follow `docs/getting-started.md` for the canonical troubleshooting and HAL workflow.
- Current branch: `feat/spec-auto-telemetry` (ahead of origin) — push when ready.

## Validation Commands
CODEX_HOME=.github/codex/home code mcp list --json  # HAL registered?
cargo run -p codex-mcp-client --bin call_tool -- --tool http-get --args '{"url":"http://127.0.0.1:7878/health"}' -- npx -y hal-mcp  # MCP helper sanity check
python3 scripts/spec_ops_004/validate_schema.py docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-018  # schema guard (T13 follow-up)

## Next Steps
1. **Finalize T14 doc edits**
   - Audit `docs/slash-commands.md`, `AGENTS.md`, `docs/getting-started.md`, and cross-links to `docs/spec-kit/model-strategy.md`; ensure troubleshooting references the latest evidence set.
2. **Push branch**
   - `git status -sb`, confirm no stray files (ignore `PDDL.md` and local evidence), then `git push origin feat/spec-auto-telemetry`.
3. **Run guardrail smoke before `/spec-auto`**
   - `/spec-ops-plan SPEC-KIT-018 --baseline-mode full`
   - `SPEC_OPS_TELEMETRY_HAL=1 /spec-ops-validate SPEC-KIT-018`
   - `/spec-auto SPEC-KIT-018` (halts if consensus or telemetry drift).

## Next Session Prompt
```
source ~/.bashrc
cd ~/code
git status -sb
git push origin feat/spec-auto-telemetry
```

## Telemetry & Consensus Troubleshooting

- **Schema failures:** Inspect the latest guardrail JSON under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/<SPEC-ID>/`. Ensure common fields (`command`, `specId`, `sessionId`, `timestamp`, `schemaVersion`, `artifacts`) and stage payload (baseline/tool/lock/scenarios/unlock) match docs/SPEC-KIT-013-telemetry-schema-guard/spec.md. Export `SPEC_OPS_TELEMETRY_HAL=1` when capturing HAL smoke so `hal.summary` is present for downstream checks.
- **Degraded consensus:** Re-run the affected `/spec-*` stage with higher thinking budgets (`/spec-plan --deep-research`, escalate to `gpt-5-pro`). Verify model metadata (`model`, `model_release`, `reasoning_mode`) is present in agent responses (see docs/spec-kit/model-strategy.md).
- **Evidence drift:** Run `/spec-ops-plan` and `/spec-ops-validate` again to refresh artifacts, then re-run `/spec-auto`. Nightly T15 sync should report any lingering mismatches. Ensure both healthy and degraded HAL JSON artifacts (e.g., `20250929-114636Z-hal-health.json` vs `20250929-114708Z-hal-health.json`) remain checked in for SPEC-KIT-018.
