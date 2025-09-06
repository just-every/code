Upstream Merge Report

Incorporated:
- Upstream `openai/codex@main` merged with `-X ours` default.
- New Rust TUI tests and fixtures (vt100 histories, status indicator, streaming).
- New TUI modules: `key_hint.rs`, `wrapping.rs`, `render/highlight.rs`, `resume_picker.rs`, `version.rs`.
- Core additions: `core/src/event_mapping.rs`, `core/src/user_instructions.rs`.
- MCP server test: `mcp-server/tests/suite/list_resume.rs`.
- CI/docs assets and labels added where non-conflicting.

Dropped:
- Our versions retained on conflicts due to `-X ours` policy (TUI theming/behavior, AGENTS.md, CHANGELOG.md, README.md).
- Excluded upstream workflow files from this branch to satisfy token permission limits.

Other changes:
- Fix build script path bug: `build-fast.sh` now sets `REPO_ROOT` to the repository root instead of its parent to keep `CARGO_HOME` inside the workspace. This resolved a sandbox permission error.
- Validated build with `./build-fast.sh` (dev-fast). Result: PASS, no warnings.

