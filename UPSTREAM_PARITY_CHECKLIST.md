# Upstream Parity Quick Reference

**Status (November 7, 2025):** `code-rs/main` carries upstream commits through `openai/codex@2030b2808` (Nov 7 2025). `code-ansi-escape`, `code-backend-client`, `code-cloud-tasks-client`, `code-execpolicy`, `code-git-apply`, and `code-linux-sandbox` all continue to re-export the upstream `codex-*` crates. Thin wrappers for `code-mcp-client`, `code-mcp-types`, and `code-responses-api-proxy` remain in place and should stay minimal.

## Cadence
- **First Monday:** run `scripts/upstream-merge/diff-crates.sh --all` and review `.github/auto/upstream-diffs/SUMMARY.md` for unexpected churn.
- **Second Monday (if needed):** run `scripts/upstream-merge/highlight-critical-changes.sh --all`, capture decisions via `log-merge.sh`, and update wrapper docs if APIs shift.
- **Ad hoc:** trigger the cadence immediately when upstream ships MCP/runtime changes we rely on.

## Spot Checks
- Ensure wrapper crates only contain configuration glue (buffer sizes, binary names) and continue to compile without warnings.
- When adopting new upstream releases, document any fork-only hooks inside the crate README or `docs/maintenance/upstream-diff.md`.
- Security-sensitive crates (`code-execpolicy`, `code-linux-sandbox`) should get manual smoke checks after each bump.

For the full workflow, see `docs/maintenance/upstream-diff.md`.
