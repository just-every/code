This PR merges the latest upstream changes from `openai/codex@main` into our `upstream-merge` branch using the default `-X ours` strategy.

Summary

- Incorporated:
  - Rust TUI additions: `key_hint`, `resume_picker`, `wrapping`, highlighting, version module, and expanded test suites/fixtures
  - Core additions: `event_mapping.rs`, `user_instructions.rs`
  - MCP server test: `list_resume.rs`
  - Release helper script: `codex-rs/scripts/create_github_release`
- Kept ours:
  - `AGENTS.md`, `CHANGELOG.md`, `README.md`
  - Existing TUI themes/browser/agents; only trivial upstream modules/tests added
  - Retained our CI workflows (upstream workflow additions not carried over)

Validation

- Ran `./build-fast.sh` at repo root.
- Result: build successful, no warnings or errors.

Notes

- Merge performed with `--allow-unrelated-histories` due to upstream lineage.
- `.gitignore` excludes sandbox build caches and automation stdout to avoid accidental commits.

