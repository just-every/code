This PR merges `openai/codex` `main` into our `upstream-merge` branch using the recursive strategy with `-X ours` to prefer our changes on conflicts.

Highlights
- Incorporates upstream Rust TUI improvements (highlighting, wrapping, resume picker) and expanded VT100/status tests.
- Adds core `event_mapping` and `user_instructions` modules from upstream.
- Preserves our TUI themes/browser/agents and our AGENTS.md/CHANGELOG.md/README.md per policy.
- Brings in upstream CI assets and docs without altering our workflows beyond merge results.

Validation
- Ran `./build-fast.sh` from repo root; build completed successfully with no warnings.
- No additional changes were necessary.

If anything should be reverted or adopted differently (especially TUI pieces), let me know and I can adjust.
