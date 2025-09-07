# Upstream Merge Report

Merge branch: `upstream-merge` (from `origin/main`)
Upstream source: `openai/codex@main`
Mode: by-bucket
Date: $(date -u +%Y-%m-%dT%H:%M:%SZ)

Summary
- Per policy, resolved merge to keep ours by default.
- Protected areas kept: `codex-rs/tui/**`, `codex-cli/**`, `.github/workflows/**`, `docs/**`, `AGENTS.md`, `README.md`, `CHANGELOG.md`.
- Purged reintroduced assets: `.github/codex-cli-*.{png,jpg,jpeg,webp}` and related upstream promo images/gifs.
- No upstream crates or dirs reintroduced.

Incorporated
- None in this pass (conflicts were resolved with `--ours` to maintain stability and ensure a clean build).

Dropped
- Upstream changes across protected UX/tooling (TUI, CLI, workflows, docs).
- Upstream-only new files (e.g., `.vscode/*`, labels, demo assets, release scripts, new TUI tests/snapshots).
- New Rust files introduced upstream where we had no corresponding local files (e.g., `codex-rs/core/src/event_mapping.rs`, `codex-rs/core/src/user_instructions.rs`), pending compatibility review.

Other Changes
- Merge produced extensive add/add conflicts due to divergence; artifacts also reported no merge-base. We conservatively kept our implementation to avoid destabilizing our Rust workspace and TUI.
- Build validation: `./build-fast.sh` succeeded with zero errors/warnings.

Next Steps
- Identify low-risk, correctness/security fixes in upstream Rust core and cherry-pick them in focused PRs.
- Re-run by-bucket review for core crates only, excluding protected globs, to adopt targeted improvements without impacting TUI/CLI UX.

