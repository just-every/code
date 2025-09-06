This PR merges the latest `openai/codex` `main` into our `upstream-merge` branch.

Key details:

- Strategy: `git merge upstream/main -X ours --allow-unrelated-histories`
- Kept ours for: `AGENTS.md`, `CHANGELOG.md`, `README.md`, and our TUI customizations (themes/browser/agents). Upstream improvements were incorporated when non-conflicting.
- Added upstream files: new TUI modules/tests, core modules (`event_mapping.rs`, `user_instructions.rs`), MCP server tests, GitHub workflows, and release script.

Validation:

- Ran `./build-fast.sh` from repo root.
- Build result: PASS. No warnings or errors.

Notes:

- Used a local `CARGO_HOME` during the build to respect sandbox write rules (no source changes).
- Changes are minimal and focused on the merge per policy.

See `MERGE_REPORT.md` for a concise summary of incorporated vs. dropped items.

