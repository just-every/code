This PR merges the latest upstream changes from openai/codex@main into our fork, on branch `upstream-merge`.

Summary
- Strategy: `git merge -X ours --allow-unrelated-histories upstream/main`
- Focus: Keep our TUI themes/browser/agents and our docs (AGENTS.md, CHANGELOG.md, README.md) while incorporating upstream improvements where non‑conflicting.
- Added from upstream: new TUI modules/tests, core event mapping and user instructions, repo assets, VS Code settings, and release helper.

Validation
- Built with `./build-fast.sh` (dev-fast profile)
- Result: ✅ Build successful; no warnings or errors

Details
- See MERGE_REPORT.md for a concise list of incorporated items and policies applied.

Follow‑ups
- None required. CI and workflows remain as in our fork where conflicts would arise.
