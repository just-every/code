# Upstream Merge Plan

Strategy: by-bucket

- Scope: Merge upstream main into fork using guarded, area-based reconciliation.
- Default policy: Keep ours across the board to preserve the TUI/CLI/tooling UX.
- Protected (prefer ours): codex-rs/tui/**, codex-cli/**, .github/workflows/**, docs/**, AGENTS.md, README.md, CHANGELOG.md
- Purge list: .github/codex-cli-*.{png,jpg,jpeg,webp}
- Reintroduced paths: Do not reintroduce any directories removed in our default branch; if upstream adds them, they will be removed.
- Review mode: Single upstream bucket (1 commit ahead). Integrate only correctness/security fixes that do not affect protected areas; otherwise keep ours.

Steps:
1) Create merge branch from origin/main
2) Merge upstream/main with --no-commit
3) Resolve to ours; remove protected/purge additions
4) Ensure previously-deleted paths stay deleted
5) Commit, build with ./build-fast.sh, fix minimal issues
6) Push and open PR
