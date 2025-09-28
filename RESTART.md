# Restart Plan: Spec Kit Multi-Agent Pipeline

## Status
- New HAL integration templates committed under `docs/SPEC-KIT-018-hal-http-mcp/` (template repo).
- Project downstream wiring started in `/home/thetu/kavedarr/docs/hal/` (config/profile/readme) but not yet committed.
- HAL MCP entry appended to `/home/thetu/.code/config.toml` (needs Codex restart) but HAL secret not exported.
- API key for HAL located in `/home/thetu/kavedarr/.env` (`HAL_SECRET_KAVEDARR_API_KEY`).
- Kavedarr service currently fails to start due to missing JWT env vars in shell (need export).
- Spec tracker updated: T18 row now points to integration status; T10 docs still pending evidence.
- Working tree dirty: template docs, spec prompts, slash commands, SPEC.md, SPEC-kit-tasks.md, new docs/hal/ folder.

## Validation Commands
CODEX_HOME=.github/codex/home code mcp list --json  # verify HAL appears after restart
cd /home/thetu/kavedarr && source .env && cargo run --bin kavedarr  # bootstrap API key & run service
HAL_PROFILE=/home/thetu/kavedarr/docs/hal/hal_profile.json code mcp call hal health

## Next Steps
1. **Bring Kavedarr API up locally**
   - `export JWT_PRIVATE_KEY_PATH=/home/thetu/kavedarr/keys/jwt-private.pem`
   - `export JWT_PUBLIC_KEY_PATH=/home/thetu/kavedarr/keys/jwt-public.pem`
   - `source .env` (provides DB + HAL secret) then `cargo run --bin kavedarr`
   - Capture/bootstrap message with `kvd_…` key if it rotates.
2. **Export HAL secret to shell before launching Codex**
   - `export HAL_SECRET_KAVEDARR_API_KEY=$(grep HAL_SECRET_KAVEDARR_API_KEY .env | cut -d"=" -f2 | tr -d "'" )`
   - Restart Codex (`code`) or `/mcp reload` so HAL server loads new env.
3. **Run HAL smoke profile**
   - `HAL_PROFILE=/home/thetu/kavedarr/docs/hal/hal_profile.json code mcp call hal health`
   - repeat for `list_movies`, `indexer_test`, `graphql_ping`
   - Save responses under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-018/`.
4. **Commit template + project wiring**
   - Stage & commit `docs/SPEC-KIT-018-hal-http-mcp`, `docs/hal`, `codex-rs/tui/src/spec_prompts.rs`, `docs/slash-commands.md`, `SPEC.md`, `SPEC-kit-tasks.md` (after evidence captured).
5. **Resume remaining SPEC tasks**
   - T10/T13/T14 follow-up after HAL evidence.

## Next Session Prompt
- Source `.env` and export JWT + HAL secrets.
- Start `cargo run --bin kavedarr`, confirm API key bootstrap (log `kvd_…`).
- Restart Codex so HAL appears in `/mcp list`.
- Run `code mcp call hal {health,list_movies,indexer_test,graphql_ping}` with `HAL_PROFILE` set; archive JSON under SPEC-KIT-018 evidence.
- Stage/commit HAL docs + SPEC tracker updates once evidence stored.

## Telemetry & Consensus Troubleshooting

- **Schema failures:** Inspect the latest guardrail JSON under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/<SPEC-ID>/`. Ensure common fields (`command`, `specId`, `sessionId`, `timestamp`, `schemaVersion`, `artifacts`) and stage payload (baseline/tool/lock/scenarios/unlock) match docs/SPEC-KIT-013-telemetry-schema-guard/spec.md. Re-run the guardrail after fixing shell output.
- **Degraded consensus:** Re-run the affected `/spec-*` stage with higher thinking budgets (`/spec-plan --deep-research`, escalate to `gpt-5-pro`). Verify model metadata (`model`, `model_release`, `reasoning_mode`) is present in agent responses (see docs/spec-kit/model-strategy.md).
- **Evidence drift:** Run `/spec-ops-plan` and `/spec-ops-validate` again to refresh artifacts, then re-run `/spec-auto`. Nightly T15 sync should report any lingering mismatches.
