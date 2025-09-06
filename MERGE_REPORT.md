Upstream merge report

Incorporated:
- Merge upstream `openai/codex@main` into `upstream-merge` with `-X ours`.
- Added upstream Rust TUI improvements and tests (wrapping, status indicator, VT100, key hints, highlighting, resume picker).
- Pulled new core modules: `codex-rs/core/src/event_mapping.rs`, `codex-rs/core/src/user_instructions.rs`.
- Synced CI and docs files that did not conflict.

Dropped/Kept Ours:
- Kept our AGENTS.md, CHANGELOG.md, README.md as-is.
- Preferred our versions for TUI themes/browser/agents code where conflicts arose.
- Left .github/workflows as in our repo unless merge added non-conflicting files.

Other Changes:
- No functional refactors; only merge-related additions.
- Build validation blocked in this sandbox: cargo registry write was denied.

Validation Status:
- ./build-fast.sh could not complete due to sandbox permission error writing to `.cargo-home/registry`.
- No code-level fixes were necessary/applied beyond policy keeps listed above.

