This PR merges `openai/codex` `main` into our fork using a merge-only flow and `-X ours` as the default strategy.

Summary
- Base: `origin/main` → branch `upstream-merge`
- Upstream: `openai/codex@main`
- Strategy: `--allow-unrelated-histories -X ours`
- Special handling: kept our TUI themes/browser/agents and our primary docs; incorporated upstream-only files.

Incorporated
- New TUI modules and tests: key hints, wrapping, highlight, resume picker; plus snapshot fixtures.
- Core updates: `event_mapping.rs`, `user_instructions.rs`.
- Added labels, PR template, VSCode config from upstream (no conflicts).

Dropped / Prefer Ours
- Retained our `AGENTS.md`, `CHANGELOG.md`, `README.md` on conflict.
- TUI kept our customizations; accepted upstream-only additions.
- Skipped adding upstream `.github/workflows/*` in this merge commit due to token scope restrictions on pushes.

Other Changes
- Build script: keep `CARGO_HOME` inside repo (`${SCRIPT_DIR}/.cargo-home`) to satisfy sandbox permissions.

Validation
- Ran `./build-fast.sh` from repo root.
- Result: PASS (no warnings). Binary at `codex-rs/target/dev-fast/code`.

Notes
- No manual code conflict resolutions were required.
- Workflows are intentionally excluded from this merge commit; can be brought in via a maintainer merge if needed.

