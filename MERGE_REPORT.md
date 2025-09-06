# Upstream Merge Report

- Source: `openai/codex` @ `upstream/main`
- Target branch: `upstream-merge` (from `origin/main`)
- Strategy: `-X ours` with `--allow-unrelated-histories`

## Incorporated

- New Rust TUI modules and tests from upstream:
  - `codex-rs/tui/src/key_hint.rs`, `render/highlight.rs`, `resume_picker.rs`, `wrapping.rs`, `version.rs`.
  - Additional TUI streaming and VT100 test suites and fixtures.
- Core additions:
  - `codex-rs/core/src/event_mapping.rs`, `user_instructions.rs`.
- MCP Server tests: `codex-rs/mcp-server/tests/suite/list_resume.rs`.
- Scripts: `codex-rs/scripts/create_github_release`.
- GitHub materials (images, labels, templates). Note: workflows were not retained in this branch; see Dropped.
- CLI docs: `codex-cli/README.md`.

## Dropped / Prefer-ours

- `AGENTS.md`, `CHANGELOG.md`, `README.md`: kept our versions when conflicts arose.
- TUI customizations (themes/browser/agents): retained our implementation; upstream changes incorporated only when non-conflicting.
- Upstream GitHub workflows: omitted from this branch due to GitHub token policy (PAT without `workflow` scope rejects pushes that add or modify workflows). Maintainers can re-add on merge.

## Other Changes

- Per sandbox constraints, used a local `CARGO_HOME` during build to keep writes inside the repo.
- Ran `./build-fast.sh` successfully; no warnings or errors reported.
- No additional refactors; changes are limited to the merge and required metadata.

## Build Status

- Result: PASS — `./build-fast.sh` completed successfully.
