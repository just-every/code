# Restart Plan: Spec Kit Multi-Agent Pipeline

## Context Recap
- Branch: `feat/spec-auto-telemetry`; borrow-check fixes and evidence gate tests for `/spec-auto` already merged into the worktree.
- Multi-agent slash commands (`/spec-plan`, `/spec-tasks`, `/spec-implement`, `/spec-validate`, `/spec-review`, `/spec-unlock`, `/spec-auto`) source prompts from `docs/spec-kit/prompts.json` via `codex-rs/tui/src/spec_prompts.rs`.
- Guardrail wrappers `/spec-ops-*` describe the follow-on multi-agent stage; Spec Ops telemetry lands under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/`.
- `/spec-auto` self-heals failed guardrail runs by re-queueing implementation/validation up to two retries (`a13178aa-d909-4c14-b94a-3cf11e5f2c4a`) and enforces artifact presence (`27b8cb79-ecb6-4c9f-9866-996df23bdee8`).
- `.github/codex/home/config.toml` and `.bak` now define MCP launchers for `repo_search`, `doc_index`, `shell_lite`, `git_status`, and `spec_registry`; `.github/codex/home/mcp-registry.json` mirrors these recipes for `mcp-registry` experiments (`9f31e38b-edcc-4c3d-abc6-2aab9cad6b77`).

## Latest Progress (2025-09-25)
- Cleaned `SPEC-kit-tasks.md` to mark completed slash-command/prompt delivery as `Done`.
- Hardened `/spec-auto` evidence validation and added `spec_auto_evidence_*` unit coverage.
- Installed and smoke-checked MCP servers via `uvx`/`npx`; `CODEX_HOME=.github/codex/home code mcp list --json` returns the five configured launchers.
- Stored session notes in local memory (`48f109c8-6a37-4841-a9f9-eca2eeaf3459`, `9f31e38b-edcc-4c3d-abc6-2aab9cad6b77`).

## Command & Task Status
- Slash commands `/spec-plan` through `/spec-unlock` are shipping and referenced by SPEC-kit tasks T3–T8 (`SPEC-kit-tasks.md`).
- `/spec-auto` (SPEC task T11) remains **In Progress**: state machine compiles, evidence gate is in place, MCP lookup integration and full telemetry logging still pending.
- Outstanding backlog items linked to the command rollout: T2 (telemetry rename), T9 (MCP enablement—now partially implemented but requires full validation), T10 (local-memory migration), T12 (consensus diff reviewer), T13 (telemetry schema enforcement), T14 (doc refresh), and T15 (nightly sync).

## Pending Work (Next Session Focus)
1. **Relaunch Codex with MCP stack enabled**
   - Set `CODEX_HOME=.github/codex/home` and run `code mcp list --json` after rebuilding; archive CLI output under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/` with a timestamped filename.
   - Use `code mcp get <name> --json` to capture full launcher metadata for each server.
2. **Handshake smoke tests (per server)**
   - `repo_search`: `uvx awslabs.git-repo-research-mcp-server --help` plus a Codex lookup against this repo; log latency and tool list.
   - `doc_index`: `npx -y open-docs-mcp --docsDir /home/thetu/code/docs` dry-run; confirm index rebuild succeeds and note file count.
   - `shell_lite`: launch via Codex and issue `pwd`, `ls`, `whoami`; verify telemetry captures stdout/stderr cleanly.
   - `git_status`: request `status` and `log` via MCP; ensure `GIT_PAGER=cat` prevents pager hangs, record returned summary.
   - `spec_registry`: attempt SSE proxy handshake (`curl -H 'Accept: text/event-stream' https://registry.modelcontextprotocol.io/api/sse`); if 404 persists, log evidence and open follow-up.
3. **Telemetry integration**
   - Update `codex-rs/tui/src/chatwidget.rs:15555-15780` so `/spec-auto` evidence lookups query MCP tools first (filesystem fallback second).
   - Include MCP handshake results in telemetry summaries and local-memory write-backs.
   - Close SPEC-kit task T11 once the state machine exercises MCP lookups end-to-end.
4. **Regression coverage**
   - Re-run targeted unit tests (`cargo test -p codex-tui spec_auto`) and expand coverage for new MCP helpers.
   - Execute an end-to-end `/spec-auto` flow that exercises at least one MCP evidence lookup; collect artefacts under the Spec Ops evidence tree.
5. **Tracker hygiene**
   - Update the relevant row(s) in `SPEC.md` once MCP validation or `/spec-auto` wiring progresses; run `scripts/spec-kit/lint_tasks.py` after edits.
   - Refresh this file and local memory with any scope changes or new dependencies.

## Research & Notes
- Confirm whether MCP servers should be scoped to Codex CLI or shared with other agents; document the outcome.
- Monitor npm/pip registries for server updates; `@modelcontextprotocol/server-shell-lite` remains unavailable, so `super-shell-mcp` is the backed option.
- Track the SSE registry endpoint (currently 404) and coordinate with maintainers when a stable stream becomes available.

## Local Memory References
- Slash command prompts rollout: `8d7681a2-ebe9-43da-bbad-04d79aa01578`
- Telemetry self-healing summary: `a13178aa-d909-4c9f-9866-996df23bdee8`
- Evidence gate & tests: `27b8cb79-ecb6-4c9f-9866-996df23bdee8`
- Spec Ops evidence trail: `455e77d2-e2db-4e83-aa8a-6a7ec7cfc64c`
- Project constitution reminder: `f12d5830-942d-4236-a2e1-6cba91c294a1`
- MCP config snapshot: `9f31e38b-edcc-4c3d-abc6-2aab9cad6b77`
- Session wrap summary: `48f109c8-6a37-4841-a9f9-eca2eeaf3459`

## MCP Validation Playbook (use after relaunch)
- Export `CODEX_HOME` and rerun `code mcp list --json`; store raw output alongside run metadata.
- Launch each server manually to capture version strings (`uvx --version`, `npx --version`, server-specific `--help`).
- From Codex, invoke a representative tool per server; ensure responses appear in telemetry logs and attach artefacts under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/`.
- For any failure (latency, non-zero exit, registry 404), snapshot CLI output and create follow-up tasks in SPEC tracker.

## Next Session Kickoff Checklist
- [ ] Follow the MCP validation playbook and collect artefacts.
- [ ] Wire `/spec-auto` telemetry to the MCP lookup path and rerun unit + integration tests.
- [ ] Update SPEC.md task statuses; run `scripts/spec-kit/lint_tasks.py` afterward.
- [ ] Record new findings in local memory (importance ≥7) and refresh this plan if scope shifts.
- [ ] Review guardrail gaps or additional spec-kit command work before expanding scope.
