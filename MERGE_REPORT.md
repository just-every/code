Upstream merge report (openai/codex@main → upstream-merge)

Incorporated:
- Upstream Rust TUI additions: `version.rs`, `wrapping.rs`, `key_hint.rs`, new snapshot/tests and fixtures
- Core improvements: `core/src/event_mapping.rs`, `core/src/user_instructions.rs`
- MCP server test: `tests/suite/list_resume.rs`
- Repo assets and templates: `.github/*` images, labels, PR template
- VS Code configs and release helper: `.vscode/*`, `codex-rs/scripts/create_github_release`

Dropped/overridden:
- Kept our AGENTS.md, README.md, and CHANGELOG.md (policy)
- Preferred our workflows on conflict; no manual edits needed
- TUI: kept our themes/browser/agents; upstream additions merged when non‑conflicting

Other changes:
- Merged with `-X ours --allow-unrelated-histories` to prefer our side on conflicts
- Built via `./build-fast.sh` using sparse registry and local CARGO_HOME
- Result: build succeeded cleanly with no warnings (dev-fast profile)

