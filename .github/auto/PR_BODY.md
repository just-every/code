This PR merges the latest `openai/codex@main` into our fork using a merge-only flow.

Summary
- Strategy: `git merge -X ours --allow-unrelated-histories upstream/main` on a fresh `upstream-merge` branch from `origin/main`.
- Policy keeps: retained our `AGENTS.md`, `CHANGELOG.md`, and `README.md`; preferred our TUI themes/browser/agents on conflicts; left workflows unchanged unless non-conflicting upstream files were added.
- Notable upstream additions: new Rust TUI modules and tests (wrapping, status indicator, VT100 history/live tests, key hints, syntax highlighting, resume picker), plus core files `event_mapping.rs` and `user_instructions.rs`.

Build & Validation
- Attempted to run `./build-fast.sh`, but the sandbox denied Cargo registry writes (permission denied under `.cargo-home/registry`). As a result, a local build could not be completed in this environment.
- No manual code refactors were introduced; changes are limited to the merge and required policy keeps.

Next Steps
- Re-run `./build-fast.sh` in a non-restricted environment to validate and address any warnings (treat warnings as failures per policy).
- If any conflicts arise in TUI visual behavior, prefer our theme/browser/agents customizations and selectively adopt upstream improvements if trivial.

Artifacts
- See `MERGE_REPORT.md` for a concise summary of incorporated vs. kept changes.
