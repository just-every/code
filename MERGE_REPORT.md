Upstream merge summary (openai/codex@main → upstream-merge)

Incorporated

- Rust TUI improvements: added `codex-rs/tui/{key_hint.rs,render/highlight.rs,resume_picker.rs,version.rs,wrapping.rs}` and new TUI test suites and fixtures.
- Core additions: added `codex-rs/core/src/{event_mapping.rs,user_instructions.rs}`.
- MCP server tests: added `codex-rs/mcp-server/tests/suite/list_resume.rs`.
- Release helper: added `codex-rs/scripts/create_github_release`.

Dropped / Kept Ours

- Kept our `AGENTS.md`, `CHANGELOG.md`, and `README.md` per policy.
- TUI: kept our existing themes/browser/agents logic; incorporated only trivial upstream additions (new modules and tests).
- Workflows: retained our existing CI workflows; upstream workflow additions not carried over to match our CI policy.

Other Changes

- Merge used `-X ours` with `--allow-unrelated-histories` to join histories.
- Validated with `./build-fast.sh`: build successful, no warnings or errors.
- Re-added `.gitignore` entries for sandbox build caches if needed in future.

