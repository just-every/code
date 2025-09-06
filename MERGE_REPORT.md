Upstream Merge Report

Incorporated:
- Merge upstream `openai/codex@main` into `upstream-merge` using `-X ours`.
- Pulled latest Rust workspace updates across `codex-rs/*` crates.
- Synced non-conflicting changes in CLI scripts and docs where applicable.

Dropped:
- Kept our TUI customizations (themes/browser/agents) and existing behavior.
- Preserved our `AGENTS.md`, `CHANGELOG.md`, and `README.md` contents on conflict.
- Preferred our existing GitHub workflows where conflicts would arise.

Other changes:
- Adjust `build-fast.sh` to keep caches inside the workspace sandbox by
  setting `REPO_ROOT` to the script directory. This ensures `CARGO_HOME`
  resolves to `./.cargo-home` within the repo and avoids write permissions
  outside the workspace.

Validation:
- Ran `./build-fast.sh` from repo root: build succeeded with no warnings.

