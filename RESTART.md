# Restart Plan: Spec Kit Multi-Agent Pipeline

## Status
Checked MCP stack: `repo_search`, `doc_index`, `shell_lite`, and `git_status` all respond via `codex-mcp-client`. SPEC-kit T2, T9, T11, and T12 are complete; remaining backlog covers T10 local-memory migration, T13 telemetry schema guard, T14 docs refresh, and T15 nightly sync.

## Validation Commands
CODEX_HOME=.github/codex/home code mcp list --json
(cd codex-rs && target/debug/codex-mcp-client uvx awslabs.git-repo-research-mcp-server@latest)
(cd codex-rs && target/debug/codex-mcp-client npx -y open-docs-mcp --docsDir /home/thetu/code/docs)
(cd codex-rs && target/debug/codex-mcp-client npx -y super-shell-mcp)
(cd codex-rs && target/debug/codex-mcp-client uvx mcp-server-git --repository /home/thetu/code)

## Next Steps
1. **T10 Local-memory migration**
   - Add fixture-based tests for `scripts/spec-kit/migrate_local_memory.py` and wire the bulk Byterover export helper (single retrieval on Oct 2).
   - Extend TUI slash-command hydration to use the new local-memory helper (consensus already updated) and add coverage for the fallback hook.
   - Prep Oct 2 runbook: dry-run → apply → rerun nightly drift detector; cache Byterover export for replay.
2. **T13 Telemetry schema guard** – integrate validator into remaining guardrail paths once T10 data flows are stable; expand scenario coverage.
3. **T14 Docs refresh** – fold migration workflow + nightly sync docs into onboarding/AGENTS.
4. **T15 Nightly sync** – once T10 lands, re-run detector and attach evidence to SPEC tracker.
5. MCP expansions (T16–T19) stay queued until core migration + schema work finishes.

## Next Session Prompt
- Add unit tests for `scripts/spec-kit/migrate_local_memory.py` using fixture exports (dry-run/apply) and commit results.
- Wire the local-memory helper into `/spec-plan`/`/spec-tasks` hydration and add tests around empty vs populated context.
- Draft the Byterover bulk export script (`scripts/spec-kit/fetch_byterover_bulk.py`) that performs a single batched retrieval and caches JSON for the Oct 2 migration.

## Telemetry & Consensus Troubleshooting

- **Schema failures:** Inspect the latest guardrail JSON under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/<SPEC-ID>/`. Ensure common fields (`command`, `specId`, `sessionId`, `timestamp`, `schemaVersion`, `artifacts`) and stage payload (baseline/tool/lock/scenarios/unlock) match docs/SPEC-KIT-013-telemetry-schema-guard/spec.md. Re-run the guardrail after fixing shell output.
- **Degraded consensus:** Re-run the affected `/spec-*` stage with higher thinking budgets (`/spec-plan --deep-research`, escalate to `gpt-5-pro`). Verify model metadata (`model`, `model_release`, `reasoning_mode`) is present in agent responses (see docs/spec-kit/model-strategy.md).
- **Evidence drift:** Run `/spec-ops-plan` and `/spec-ops-validate` again to refresh artifacts, then re-run `/spec-auto`. Nightly T15 sync should report any lingering mismatches.
