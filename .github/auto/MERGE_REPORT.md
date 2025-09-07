# Upstream Merge Report

Upstream: openai/codex@main
Local base: origin/main
Merge branch: upstream-merge
Merge mode: by-bucket

## Summary
- Upstream commits ahead: 1 (02690962 Move token usage/context information to session level (#3221))
- Strategy: Ours-first. Preserve our TUI, CLI, workflows, and docs. Do not reintroduce removed crates/dirs.
- Histories were unrelated; performed a graft merge with `--allow-unrelated-histories` and resolved all files to ours by default.

## Incorporated
- None. The single upstream bucket primarily introduces/changes broad repository structure, TUI assets, workflows, and Rust crates we already maintain with a materially different approach. Incorporating these wholesale would override our UX and tooling.

## Dropped
- All upstream additions under protected areas: `codex-rs/tui/**`, `codex-cli/**`, `.github/workflows/**`, `docs/**`, and repo-level docs (`AGENTS.md`, `README.md`, `CHANGELOG.md`).
- Purged any `.github/codex-cli-*.{png,jpg,jpeg,webp}` images if present.
- Prevented reintroduction of any paths listed in `.github/auto/DELETED_ON_DEFAULT.txt` (none changed).

## Other Notes
- This merge brings upstream history into the graph without content changes to our codebase, keeping us positioned for selective cherry-picks in future buckets.
- Build validation via `./build-fast.sh` succeeded with zero warnings or errors.

## Validation
- Build: PASS (`./build-fast.sh`)

