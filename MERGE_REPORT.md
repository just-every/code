# Upstream Merge Report

Attempt: upstream main -> our `upstream-merge` (from origin/main)
Date: $(date -u +"%Y-%m-%dT%H:%M:%SZ")
Mode: by-bucket

Summary
- Merge stopped: Git reports unrelated histories between `origin/main` and `upstream/main`.
- Per policy, we do not use `--allow-unrelated-histories`. No upstream files were merged.

Artifacts considered
- .github/auto/COMMITS.json: Reviewed to scope areas touched (tui=140, core=60, other=95).
- .github/auto/CHANGE_HISTOGRAM.txt: Confirms heavy TUI churn upstream; we preserve our TUI.
- .github/auto/DELTA_FILES.txt, DIFFSTAT.txt: Cataloged upstream deltas; none applied due to unrelated histories.
- DELETED_ON_DEFAULT and REINTRODUCED_PATHS: No reintroduced blocks required.

Decisions
- TUI (`codex-rs/tui/**`): Keep ours. Upstream changes not merged this pass.
- CLI (`codex-cli/**`), workflows, docs: Keep ours (prefer ours globs).
- Core/others: Would selectively take correctness/security fixes; deferred because merge is blocked by unrelated histories.
- Purge policy: Ensure `.github/codex-cli-*` images remain absent; none added since no merge occurred.

Next steps (proposed)
- If maintainers approve, follow up with targeted cherry-picks or file-level sync for specific fixes (especially core correctness/security) rather than forcing an unrelated-history merge.
- Alternatively, coordinate with upstream to establish a shared ancestor or rebase our fork branch in a controlled, reviewed effort.

Build status
- Current branch builds will be validated via `./build-fast.sh`.
