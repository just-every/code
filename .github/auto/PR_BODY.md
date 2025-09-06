This PR merges the latest `openai/codex` `main` into our codebase.

- Upstream: `openai/codex@026909622` (2025-09-06)
- Branch: `upstream-merge` (recreated from `origin/main`)
- Strategy: `git merge upstream/main -X ours --allow-unrelated-histories`

Key notes
- Kept our top-level docs: `AGENTS.md`, `CHANGELOG.md`, `README.md` per policy.
- TUI: we favored our themes/browser/agents on conflicts; no manual overrides were needed.
- Non-conflicting upstream improvements across `codex-rs` crates, TUI, and docs were incorporated.

Validation
- Ran `./build-fast.sh` from repo root.
- Result: success, no warnings or errors.
- Binary: `./codex-rs/target/dev-fast/code` (dev-fast).

Artifacts
- Merge report: `MERGE_REPORT.md` documents incorporated/dropped/other changes.

Follow-ups
- None required. If we spot any UX deltas in TUI during daily usage, we can cherry-pick selectively.
