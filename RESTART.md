# Restart Plan: Spec Kit Multi-Agent Pipeline

## Status
- Guardrail scripts now propagate baseline and HAL smoke failures; new env/CLI overrides (`--allow-fail`, `SPEC_OPS_CARGO_MANIFEST`) landed locally.
- Healthy + degraded HAL telemetry captured under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-018/` for template verification (latest `hal.summary` run: `spec-validate_2025-09-29T12:33:03Z-3193628696.json`).
- Specs/tasks updated for T14, T18, and new T20 guardrail-hardening spec/plan/task files committed in this repo.
- Remaining work: finish T14 doc refresh using fresh evidence; wire HAL prompts/docs (T18); merge guardrail fixes and push to remote.

## Validation Commands
CODEX_HOME=.github/codex/home code mcp list --json  # HAL registered?
cargo run -p codex-mcp-client --bin call_tool -- --tool http-get --args '{"url":"http://127.0.0.1:7878/health"}' -- npx -y hal-mcp  # MCP helper sanity check
python3 scripts/spec_ops_004/validate_schema.py docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-018  # schema guard (T13 follow-up)

## Next Steps
1. **Push guardrail + doc updates**
   - Review staged diffs (scripts, Rust validator, docs, evidence) and commit to `feat/spec-auto-telemetry`, then `git push`.
2. **Finish T20 script patches**
   - Land baseline propagation, manifest-awareness, and HAL failure plumbing (tasks 1–3) and rerun `/spec-ops-plan|validate` with healthy + degraded HAL windows under `SPEC_OPS_TELEMETRY_HAL=1`.
3. **Prep rollout memo / CI parity**
   - Coordinate with infra on CI env vars (`SPEC_OPS_CARGO_MANIFEST`, HAL access), update the rollout memo, and schedule enforcement enablement.

## Next Session Prompt
```
source ~/.bashrc
cd ~/code
git status -sb
git add <files>
git commit -m "fix(guardrails): record hal summary telemetry"
git push origin feat/spec-auto-telemetry
```

## Telemetry & Consensus Troubleshooting

- **Schema failures:** Inspect the latest guardrail JSON under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/<SPEC-ID>/`. Ensure common fields (`command`, `specId`, `sessionId`, `timestamp`, `schemaVersion`, `artifacts`) and stage payload (baseline/tool/lock/scenarios/unlock) match docs/SPEC-KIT-013-telemetry-schema-guard/spec.md. Export `SPEC_OPS_TELEMETRY_HAL=1` when capturing HAL smoke so `hal.summary` is present for downstream checks.
- **Degraded consensus:** Re-run the affected `/spec-*` stage with higher thinking budgets (`/spec-plan --deep-research`, escalate to `gpt-5-pro`). Verify model metadata (`model`, `model_release`, `reasoning_mode`) is present in agent responses (see docs/spec-kit/model-strategy.md).
- **Evidence drift:** Run `/spec-ops-plan` and `/spec-ops-validate` again to refresh artifacts, then re-run `/spec-auto`. Nightly T15 sync should report any lingering mismatches. Ensure both healthy and degraded HAL JSON artifacts (e.g., `20250929-114636Z-hal-health.json` vs `20250929-114708Z-hal-health.json`) remain checked in for SPEC-KIT-018.
