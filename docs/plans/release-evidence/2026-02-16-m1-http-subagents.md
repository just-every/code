# Milestone 1 Evidence: HTTP-Native Subagents + Auto-Review P1 Closure

Date: 2026-02-16
Scope: `code-rs/core` + tests + docs

## Summary

Milestone 1 keeps HTTP-native subagent support for read-only agents while preserving subprocess semantics for write-mode agents.

## Auto-Review P1 Audit Outcome

Finding audited from `/home/hermia/.code/working/Hermia-Coder/branches/auto-review`:

- Reported risk: write-mode HTTP-configured agents could bypass write-mode subprocess semantics.
- Evidence of regression (failing test-first):
  - Command: `cargo test -p code-core write_mode_agents_with_http_endpoint_still_use_subprocess_execution -- --nocapture`
  - Pre-fix result: **failed** with `left: "hello from http"` and `right: "subprocess-write-ok"`.
  - This proved write-mode execution was taking HTTP dispatch instead of subprocess.
- Auto-review worktree validation:
  - The worktree had an uncommitted diff (no safe commit to cherry-pick directly).
  - Validated fix was manually applied equivalently in main workspace.

Applied fix:

- `code-rs/core/src/agent_tool.rs`
  - HTTP path is now gated to read-only execution only:
    - from: `if has_http_endpoint(config.as_ref())`
    - to: `if read_only && has_http_endpoint(config.as_ref())`
  - Added regression test:
    - `write_mode_agents_with_http_endpoint_still_use_subprocess_execution`

## Risk-Focused Coverage (Executed)

All commands below were run locally from `code-rs/` on 2026-02-16.

| Area | Command | Result |
|---|---|---|
| Config parsing | `cargo test -p code-core deserialize_agent_config_http_fields -- --nocapture` | Pass |
| Config parsing compatibility | `cargo test -p code-core deserialize_agent_config_without_http_fields -- --nocapture` | Pass |
| Slash-agent enablement | `cargo test -p code-core test_http_agents_are_runnable_without_local_cli -- --nocapture` | Pass |
| Read-only HTTP dispatch | `cargo test -p code-core http_agents_dispatch_via_endpoint_without_subprocess_binary -- --nocapture` | Pass |
| Write-mode subprocess regression | `cargo test -p code-core write_mode_agents_with_http_endpoint_still_use_subprocess_execution -- --nocapture` | Pass (after fix) |
| Subprocess non-HTTP regression | `cargo test -p code-core subprocess_agents_still_execute_without_http_endpoint -- --nocapture` | Pass |

## Ship Sweep Gates (Executed)

All commands below were run locally from repo root.

| Gate | Command | Result | Evidence |
|---|---|---|---|
| Build gate | `./build-fast.sh` | Pass | Binary hash `f8e5cf244517e86f0790514df4ed6f4577910c73b5d54e3b8854b804291dc1de` |
| Pre-release gate | `./pre-release.sh` | Pass | `nextest` run ID `d3a38480-1f55-4698-ac7a-1aede91170ff` (1364 passed, 4 skipped) |

## Behavioral Check Boundaries

| Check | Command evidence | Boundary |
|---|---|---|
| `/plan` `/code` `/solve` full completion | See Milestone 2 evidence (`/tmp/m2-plan.jsonl`, `/tmp/m2-code.jsonl`, `/tmp/m2-solve.jsonl`) | Executed locally with released Linux binary; still re-check during live publish window recommended |
| Streaming behavior | `cargo test -p code-core http_agents_dispatch_via_endpoint_without_subprocess_binary -- --nocapture` and Milestone 2 `code-tui` smoke | Local coverage only; live endpoint/network behavior remains deploy-stage concern |
| Tool-use behavior | Milestone 2: `cargo test -p code-core --test tool_hooks tool_hooks_fire_for_shell_exec -- --nocapture` | Local hook/tool execution verified; production telemetry and hosted integrations remain CI/deploy-stage |
