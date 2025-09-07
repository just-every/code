# Upstream Merge Plan (by-bucket)

Mode: by-bucket
Upstream: openai/codex@main → Branch: upstream-merge (from origin/main)

Guard Rails
- Keep ours by default across the repo; do not reintroduce locally removed crates/dirs.
- Strongly prefer ours for: `codex-rs/tui/**`, `codex-cli/**`, `.github/workflows/**`, `docs/**`, `AGENTS.md`, `README.md`, `CHANGELOG.md`.
- Purge any `.github/codex-cli-*.(png|jpg|jpeg|webp)` files if reintroduced upstream.

Buckets and Strategy
- Protected UX/Tooling (TUI, CLI, Workflows, Docs): keep ours entirely. If upstream adds new files in these globs, drop them.
- Rust Core/Crates (non-TUI): default to ours on conflicts; selectively adopt upstream only where risk-free and compatible. Given severe divergence and “no merge-base” in artifacts, this pass resolves conflicts to ours to keep the workspace buildable. Candidate upstream improvements will be reviewed and cherry-picked in follow-ups.
- Repo meta (root scripts/config): keep ours, unless a change is clearly isolated and beneficial without impacting our flows.

Procedure
1) Create `upstream-merge` from `origin/main`.
2) Attempt merge `upstream/main --no-commit`; resolve conflicts with `--ours` by default, then enforce protected globs and purge list.
3) Ensure no protected-area additions remain; remove any reintroduced purge assets.
4) Commit with a clear merge message including rationale and status.
5) Validate `./build-fast.sh` (warnings treated as failures). Apply minimal, surgical fixes only if needed.
6) Produce `MERGE_REPORT.md` summarizing Incorporated / Dropped / Other.
