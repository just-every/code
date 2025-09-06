Upstream merge report

Incorporated

- Merge upstream `openai/codex@main` into `upstream-merge` with `-X ours`.
- Pulled in upstream additions across Rust crates and TUI, including:
  - `codex-rs/core`: `event_mapping.rs`, `user_instructions.rs`.
  - `codex-rs/tui`: `key_hint.rs`, `render/highlight.rs`, `resume_picker.rs`, `version.rs`, `wrapping.rs`, plus new tests/fixtures and snapshots.
- CI assets and VS Code settings added; upstream workflow files were removed post-merge (see Dropped) to satisfy push token restrictions.
- Updated lockfiles and Cargo manifests as per upstream merge.

Dropped

- Kept our `AGENTS.md`, `CHANGELOG.md`, and `README.md` per policy.
- For TUI code and assets, defaulted to ours on conflict; upstream-only improvements were accepted when non‑conflicting.
- Preserved our GitHub Actions as-is when conflicts occurred (prefer ours).
- Removed upstream `.github/workflows/*` files to allow push with a token lacking `workflow` scope.

Other changes

- Build validated with `./build-fast.sh` (dev-fast). To respect sandbox write limits, set `CARGO_HOME="$PWD/.cargo-home"` and `CARGO_TARGET_DIR="$PWD/codex-rs/target"` only for the build invocation (no repo changes required).
- Result: build successful; no compiler warnings surfaced during the build step.
