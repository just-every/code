# Upstream Merge Report

Incorporated:
- Upstream `main` from `openai/codex` merged with `-X ours`.
- New Rust TUI modules and tests (e.g., `tui/src/render/highlight.rs`, `tui/src/wrapping.rs`, `tui/src/key_hint.rs`, `tui/src/resume_picker.rs`, VT100 + status tests and fixtures).
- Core additions: `core/src/event_mapping.rs`, `core/src/user_instructions.rs`.
- MCP server test additions: `mcp-server/tests/suite/list_resume.rs`.
- CI assets and docs from upstream (workflows, images, CLI README).

Dropped:
- Kept our local TUI themes/browser/agents on conflicts (strict preference for ours).
- Kept our `AGENTS.md`, `CHANGELOG.md`, and `README.md` unchanged.

Other changes:
- No code changes required post-merge.
- Build validated with repo-local cargo caches to satisfy sandbox.
