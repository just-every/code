Summary

- Merge upstream `openai/codex` branch `main` into `upstream-merge` using `-X ours` as default.
- Preserved our TUI theming/browser/agents and top‑level docs (`AGENTS.md`, `CHANGELOG.md`, `README.md`).
- Accepted upstream non‑conflicting improvements and new files across crates and tests.

Details

- Strategy: `git merge upstream/main --allow-unrelated-histories -X ours`.
- TUI: kept our implementations where conflicts existed; incorporated upstream additions like `key_hint.rs`, `wrapping.rs`, and new tests when non‑conflicting.
- CI/Workflows: added upstream workflow files where there were no conflicts; on conflict, kept ours per policy.
- Rust crates: merged upstream updates across `codex-core`, `codex-tui`, `codex-exec`, `codex-login`, and related crates.

Validation

- Build: ran `./build-fast.sh` from repo root.
- Sandbox note: used environment overrides for the build to keep writes inside the workspace:
  - `CARGO_HOME="$PWD/.cargo-home"`
  - `CARGO_TARGET_DIR="$PWD/codex-rs/target"`
- Result: build successful in `dev-fast` profile, no warnings observed.

Next Steps

- Review the new upstream TUI tests and fixtures that were added; they are included but we did not enable any additional CI beyond the merge.
- If desired, follow‑up PRs can selectively adopt more upstream TUI improvements.

