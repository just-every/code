# Upstream Merge Report

- Source: `openai/codex` (`main`)
- Target branch: `upstream-merge` (based on `origin/main`)
- Strategy: `-X ours` with `--allow-unrelated-histories`

## Incorporated
- New TUI components and tests from upstream:
  - `codex-rs/tui/src/{key_hint.rs,render/highlight.rs,resume_picker.rs,version.rs,wrapping.rs}`
  - `codex-rs/tui/tests/*` and additional snapshot fixtures
- Core additions:
  - `codex-rs/core/src/event_mapping.rs`
  - `codex-rs/core/src/user_instructions.rs`
- MCP server test: `codex-rs/mcp-server/tests/suite/list_resume.rs`
- Repo metadata and templates (added, no conflicts): labels, PR template, VSCode settings
- Docs improvements from upstream across `docs/*` and `codex-cli/README.md`.

## Dropped / Prefer Ours
- Kept our versions for key project docs on any conflict:
  - `AGENTS.md`, `CHANGELOG.md`, `README.md`
- TUI: retained our themes/browser/agents defaults where applicable; upstream-only new files were incorporated as-is.
- Skipped adding upstream GitHub workflow files in this merge commit to satisfy token scope restrictions when pushing (no functional code change).

## Other Changes
- Build script fix: update `build-fast.sh` to keep `CARGO_HOME` inside the repository (`${SCRIPT_DIR}/.cargo-home`) to satisfy sandbox write constraints.

## Validation
- Successfully built with `./build-fast.sh` (dev-fast): PASS
- No compiler warnings observed during the build.

