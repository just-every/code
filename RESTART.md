# Restart Plan: Spec Kit Multi-Agent Pipeline

## Status
- Guardrail scripts now propagate baseline and HAL smoke failures; new env/CLI overrides (`--allow-fail`, `SPEC_OPS_CARGO_MANIFEST`) landed locally.
- Healthy + degraded HAL telemetry captured under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-018/` for template verification.
- Specs/tasks updated for T14, T18, and new T20 guardrail-hardening spec/plan/task files committed in this repo.
- Remaining work: finish T14 doc refresh using fresh evidence; wire HAL prompts/docs (T18); merge guardrail fixes and push to remote.

## Validation Commands
CODEX_HOME=.github/codex/home code mcp list --json  # HAL registered?
cargo run -p codex-mcp-client --bin call_tool -- --tool http-get --args '{"url":"http://127.0.0.1:7878/health"}' -- npx -y hal-mcp  # MCP helper sanity check
python3 scripts/spec_ops_004/validate_schema.py docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-018  # schema guard (T13 follow-up)

## Next Steps
1. **Commit & push guardrail changes**
   - Review staged diffs (scripts + specs/tasks + evidence) and commit to `feat/spec-auto-telemetry`, then `git push`.
2. **T14 doc refresh**
   - Update `docs/slash-commands.md`, `AGENTS.md`, `docs/getting-started.md`, `RESTART.md` with new guardrail/HAL guidance.
   - Run `scripts/doc-structure-validate.sh --mode=templates` (dry-run then full) and `python3 scripts/spec-kit/lint_tasks.py`.
3. **T18 HAL documentation wrap-up**
   - Inline new evidence paths + instructions into prompts/runbooks.
   - Update SPEC.md row with timestamps; mark healthy/degraded evidence ready.
4. **T20 rollout follow-through**
   - Execute tasks listed in `docs/SPEC-OPS-004-integrated-coder-hooks/tasks.md` (baseline enforcement, telemetry flag enable, rollout memo).
   - Coordinate CI parity (root@runner SSH key) before enabling strict failure in pipelines.

## Next Session Prompt
```
source ~/.bashrc
cd ~/code
git status -sb
git commit -am "fix(guardrails): enforce baseline + hal telemetry"  # adjust message as needed
git push
scripts/doc-structure-validate.sh --mode=templates --dry-run
python3 scripts/spec-kit/lint_tasks.py
```

## Telemetry & Consensus Troubleshooting

- **Schema failures:** Inspect the latest guardrail JSON under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/<SPEC-ID>/`. Ensure common fields (`command`, `specId`, `sessionId`, `timestamp`, `schemaVersion`, `artifacts`) and stage payload (baseline/tool/lock/scenarios/unlock) match docs/SPEC-KIT-013-telemetry-schema-guard/spec.md. Re-run the guardrail after fixing shell output.
- **Degraded consensus:** Re-run the affected `/spec-*` stage with higher thinking budgets (`/spec-plan --deep-research`, escalate to `gpt-5-pro`). Verify model metadata (`model`, `model_release`, `reasoning_mode`) is present in agent responses (see docs/spec-kit/model-strategy.md).
- **Evidence drift:** Run `/spec-ops-plan` and `/spec-ops-validate` again to refresh artifacts, then re-run `/spec-auto`. Nightly T15 sync should report any lingering mismatches.
