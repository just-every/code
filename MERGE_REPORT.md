# Upstream Merge Report

Context
- Upstream: openai/codex (main)
- Branch: upstream-merge (from origin/main)
- Strategy: merge --no-commit with allow-unrelated-histories, then policy‑guided reconciliation

Policy Summary
- Prefer ours: codex-rs/tui/**, codex-cli/**, .github/workflows/**, AGENTS.md, README.md, CHANGELOG.md
- Prefer theirs: codex-rs/core/**, codex-rs/common/**, codex-rs/protocol/**, codex-rs/exec/**, codex-rs/file-search/**
- Purge: .github/codex-cli-*.{png,jpg,jpeg,webp}
- Default: ours, unless trivial doc/typo improvements without conflicts

Incorporated
- None of the Rust core/protocol changes were retained due to broad API deltas conflicting with our TUI/CLI.
- Build pipeline improvements retained locally by updating `build-fast.sh` to use the correct exec bin name (`code-exec`).

Dropped
- Upstream GitHub workflows and editor/automation assets:
  - .github/workflows/*
  - .github/auto/*
  - .vscode/*
  - Images under purge globs: .github/codex-cli-*.png (and related extensions)
- Upstream‑only TUI test and helper files added under codex-rs/tui/** (policy: prefer ours; new files removed).

Other Changes
- Merge reconciliation switched codex-rs/{core,protocol,common,exec,file-search} back to our versions to restore compatibility with our TUI and CLI.
- `codex-rs/cli` now gates the MCP server behind an optional `mcp` feature (off by default) to avoid compiling incompatible upstream MCP changes.
- Ensured `CARGO_HOME` remains workspace‑local during build; no script changes needed (invocation sets it).

Build Status
- ./build-fast.sh: PASS (dev-fast). No warnings emitted in final state.

Notes
- Upstream introduced protocol and event model changes (ordering metadata, event shapes, config enums). Fully adopting those would require a large coordinated refactor across TUI/exec/CLI. Given our policy to keep TUI/CLI intact and the constraint to be surgical, we kept our implementations for these crates in this merge.

