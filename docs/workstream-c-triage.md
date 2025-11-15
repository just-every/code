# Workstream C Crate Tracker (2025-11-15)

This file captures the actionable backlog for the Workstream C merge effort so we can
show progress every time we regenerate upstream diffs.

## Critical Triage Backlog

| Crate | Delta Size | Source | Immediate Action |
| --- | --- | --- | --- |
| core | 105,574 LOC | `.github/auto/upstream-diffs/core.diff` | Token usage + config loader ports landed in `code-rs` (see `docs/workstream-c-core-port.md`); next up is the `Agent*` lifecycle/event plumbing called out in `core-critical.md`. |
| app-server | 10,787 LOC | `.github/auto/upstream-diffs/app-server.diff` | Determine if upstream fixes (model defaults, login flow) can be adopted wholesale or need fork-only patches. |
| app-server-protocol | 3,276 LOC | `.github/auto/upstream-diffs/app-server-protocol.diff` | Align protocol structs/events with upstream to reduce serialization drift. |
| cli | 4,355 LOC | `.github/auto/upstream-diffs/cli.diff` | Audit new gpt-5.1 defaults and prompt contract changes; cherry-pick if compatible with fork-only UX.
| exec | 4,636 LOC | `.github/auto/upstream-diffs/exec.diff` | Compare execpolicy/command lifecycle hooks to ensure strict ordering + Auto Drive escape contracts stay intact. |

### Core Highlights

- New token usage tracking primitives (`TokenTotals`, `StoredUsageSummary`, etc.) and
  compaction helpers must be incorporated to stay in lock-step with upstream billing logic.
- `AgentManager`/`AgentStatus` plumbing now issues richer `EventMsg::*` variants and
  stamps them with `TurnContext`; our fork needs the same coverage before next merge.
- Config loader surface (macOS + profile/types modules) diverged; plan a targeted port so
  we can drop local shims after validating on macOS.

All of the above are spelled out in `.github/auto/upstream-diffs/critical-changes/core-critical.md`.

## Re-export Candidates (Identical Crates)

The latest diff summary shows these crates are byte-for-byte identical between
`code-rs` and `codex-rs`. Track them for re-export or deletion once dependent crates stop
modifying them locally:

```
git-apply
git-tooling
linux-sandbox
login
mcp-client
mcp-server
mcp-types
ollama
otel
protocol
protocol-ts
process-hardening
rmcp-client
responses-api-proxy
tui
```

Next actions:
1. Double-check no fork-only features depend on local paths.
2. Re-point workspace members to the mirrored `codex-rs` crates or remove the duplicates.
3. Update this tracker after each deletion/re-export to keep PLAN.md accurate.

## Automation Backlog

- Guard: `./build-fast.sh` now runs `scripts/check-codex-path-deps.sh` before touching
  either workspace, so every GitHub Action that shells out to `build-fast.sh` inherits the
  protection. (Completed 2025-11-15.)
- Reporting: keep `scripts/upstream-merge/diff-crates.sh --summary` and
  `highlight-critical-changes` outputs fresh after every upstream intake; add a cron-based
  workflow if manual regeneration starts to lag.
- Core port log: in-flight details for token usage, config loader, and agent lifecycle
  integrations now live in `docs/workstream-c-core-port.md` so we can track decisions
  and open questions incrementally.
