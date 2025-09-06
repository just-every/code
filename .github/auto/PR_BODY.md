This PR merges the latest upstream changes from `openai/codex@main` into our fork using the default `-X ours` strategy.

Highlights
- Preserved our TUI customizations (themes, browser control, agents, and related files).
- Kept our `AGENTS.md`, `CHANGELOG.md`, and `README.md` content where overlaps existed.
- Preferred our GitHub workflows on conflicts; otherwise adopted upstream updates.

Small Fixes
- Updated `build-fast.sh` to scope caches inside the workspace sandbox by setting `REPO_ROOT` to the script directory. This ensures `CARGO_HOME` resolves to `./.cargo-home` and avoids permission issues during CI builds.

Validation
- Built locally via `./build-fast.sh` from repo root.
- Result: success with no warnings. Binary at `codex-rs/target/dev-fast/code`.

Merge Details
- Branch: `upstream-merge` rebased from `origin/main` and merged `upstream/main`.
- Strategy: `--allow-unrelated-histories -X ours` (kept our side on conflicts).

