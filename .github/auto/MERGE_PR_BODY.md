Title: Merge upstream main (ours-first, by-bucket guardrails)

This PR merges openai/codex@main into our fork with an ours-first policy:

- Preserve our TUI/CLI UX, workflows, and docs
- Avoid reintroducing removed crates/paths
- Purge `.github/codex-cli-*` images
- No content changes adopted from this bucket; histories grafted for future selective picks

Validation:
- `./build-fast.sh` — PASS

Artifacts:
- `.github/auto/MERGE_PLAN.md`
- `.github/auto/MERGE_REPORT.md`
