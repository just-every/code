# Restart Plan: Spec Kit Multi-Agent Pipeline

## Status
- `/spec-ops-validate` now drives HAL smoke automatically via `call_tool` and appends evidence to telemetry.
- HAL config/evidence committed in `~/kavedarr`; template repo fully updated.
- `SPEC-kit-tasks.md` has new backlog item **T20 Guardrail script hardening** (staged but not committed yet).
- Docs/prompts/slash commands reference the MCP helper; remaining in-progress tasks: **T14** docs refresh, **T18** HAL integration follow-through.

## Validation Commands
CODEX_HOME=.github/codex/home code mcp list --json  # HAL registered?
cargo run -p codex-mcp-client --bin call_tool -- --tool http-get --args '{"url":"http://127.0.0.1:7878/health"}' -- npx -y hal-mcp  # MCP helper sanity check
python3 scripts/spec_ops_004/validate_schema.py docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-018  # schema guard (T13 follow-up)

## Next Steps
1. **Bring Kavedarr API up locally**
   - `export JWT_PRIVATE_KEY_PATH=/home/thetu/kavedarr/keys/jwt-private.pem`
   - `export JWT_PUBLIC_KEY_PATH=/home/thetu/kavedarr/keys/jwt-public.pem`
   - `source .env` (provides DB + HAL secret) then `cargo run --bin kavedarr`
   - Capture/bootstrap message with `kvd_…` key if it rotates.
2. **Export HAL secret to shell before launching Codex**
   - `export HAL_SECRET_KAVEDARR_API_KEY=$(grep HAL_SECRET_KAVEDARR_API_KEY .env | cut -d"=" -f2 | tr -d "'" )`
   - Restart Codex (`code`) or `/mcp reload` so HAL server loads new env.
3. **Close out HAL task (T18)**
   - Exercise `spec_ops_run_hal_smoke` against a degraded API and define failure criteria.
   - Document findings + required code changes for telemetry enforcement.
4. **Finish docs refresh (T14)**
   - Audit `docs/slash-commands.md`, `AGENTS.md`, and restart notes for stale guardrail references; push cleanup commit.
5. **Kick off guardrail hardening plan (T20)**
   - Outline additional checks (baseline validation, SPEC lock enforcement, schema linting) and drop proposal in `docs/SPEC-OPS-004-integrated-coder-hooks/notes/`.

## Next Session Prompt
```
source ~/.bashrc
cd ~/code
export HAL_SECRET_KAVEDARR_API_KEY=$(grep HAL_SECRET_KAVEDARR_API_KEY ~/kavedarr/.env | cut -d"=" -f2 | tr -d "'")
cargo run -p codex-mcp-client --bin call_tool -- --tool http-get --args '{"url":"http://127.0.0.1:7878/health"}' -- npx -y hal-mcp
vim SPEC-kit-tasks.md  # flesh out T20 guardrail hardening plan
```

## Telemetry & Consensus Troubleshooting

- **Schema failures:** Inspect the latest guardrail JSON under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/<SPEC-ID>/`. Ensure common fields (`command`, `specId`, `sessionId`, `timestamp`, `schemaVersion`, `artifacts`) and stage payload (baseline/tool/lock/scenarios/unlock) match docs/SPEC-KIT-013-telemetry-schema-guard/spec.md. Re-run the guardrail after fixing shell output.
- **Degraded consensus:** Re-run the affected `/spec-*` stage with higher thinking budgets (`/spec-plan --deep-research`, escalate to `gpt-5-pro`). Verify model metadata (`model`, `model_release`, `reasoning_mode`) is present in agent responses (see docs/spec-kit/model-strategy.md).
- **Evidence drift:** Run `/spec-ops-plan` and `/spec-ops-validate` again to refresh artifacts, then re-run `/spec-auto`. Nightly T15 sync should report any lingering mismatches.
