# Upstream Merge Report

Summary
- Attempted `git merge --no-commit upstream/main` into `upstream-merge`.
- Result: Unrelated histories. Per policy, did not force with `--allow-unrelated-histories`.

Policy Application
- prefer_ours_globs: kept local stance for protected areas (tui, cli, workflows, docs, AGENTS.md/README/CHANGELOG).
- purge_globs: would remove upstream reintroduced marketing images under `.github/` if merged; none applied due to merge abort.

Upstream Review (by bucket)
- tui: heavy changes upstream (histogram ~140). Deferred; our TUI approach is unique and protected.
- core: moderate changes (~60). Candidate for selective, manual cherry-picks in future PRs if they improve correctness/security without TUI coupling.
- cli/tests/docs/other: deferred due to unrelated histories and protection policy.

Next Actions
- Keep this branch as an integration point documenting constraints.
- If maintainers agree, we can:
  1) Open a PR to discuss history divergence and options.
  2) Prepare targeted cherry-pick PRs for specific core fixes identified upstream.

Build Status
- `./build-fast.sh`: successful; no errors or warnings.

Artifacts
- `MERGE_PLAN.md`: by-bucket strategy recorded; no forced merge.
- `.merge_status`: UNRELATED_HISTORIES
