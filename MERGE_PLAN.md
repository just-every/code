# Upstream Merge Plan (by-bucket)

Context
- UPSTREAM: openai/codex@main
- LOCAL DEFAULT: origin/main
- MERGE MODE: by-bucket
- Policy
  - prefer_ours_globs: [
    "codex-rs/tui/**",
    "codex-cli/**",
    ".github/workflows/**",
    "docs/**",
    "AGENTS.md",
    "README.md",
    "CHANGELOG.md"
  ]
  - prefer_theirs_globs: []
  - purge_globs: [
    ".github/codex-cli-*.png",
    ".github/codex-cli-*.jpg",
    ".github/codex-cli-*.jpeg",
    ".github/codex-cli-*.webp"
  ]

Finding
- No merge-base between origin/main and upstream/main (unrelated histories). Per policy, do not force a link with `--allow-unrelated-histories`.

Strategy
- Proceed by-bucket using artifacts in `.github/auto/` to assess upstream changes.
- Do not reintroduce paths removed locally; keep our TUI and CLI flows.
- Only consider manual cherry-picks/patch-sync for correctness, security, or compatibility in Rust core crates that do not impact protected areas.
- Record all decisions and deferrals in MERGE_REPORT.md.

Buckets (from CHANGE_HISTOGRAM)
- tui (protected): keep ours; ignore upstream changes unless trivial non-UX fixes.
- core/cli/common/protocol/login/exec: candidate for targeted adoption if needed.
- docs/workflows (protected): keep ours.

Next Steps
1) Create `upstream-merge` from `origin/main`.
2) Attempt `git merge --no-commit upstream/main`.
   - If unrelated histories (expected), abort, do not force merge.
3) Document outcome and any manual adoption candidates in MERGE_REPORT.md.
4) Run `./build-fast.sh` to validate status quo.
5) Push branch and open a PR summarizing the situation and proposed follow-ups.

