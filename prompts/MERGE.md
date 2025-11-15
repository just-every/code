# Merge Playbook (Workstreams B → C)

## Goals
1. Keep `code-rs/` as the only place with fork-specific Rust code.
2. Keep `codex-rs/` as a **pure mirror** of `openai/codex:main`.
3. Integrate upstream changes regularly while preserving our UX (themes, browser, multi-agent flows) and CI surface.
4. Validate the fork with `./build-fast.sh` (warning-free) and the guard script (`scripts/check-codex-path-deps.sh`).

---

## Preparation
1. **Ensure remotes**
   ```bash
   git remote -v
   # add upstream if missing
   git remote add upstream https://github.com/openai/codex.git
   ```
2. **Sync local repos**
   ```bash
   git fetch origin main
   git fetch upstream main
   # update the mirror checkout next to this repo
   git -C ../codex fetch origin
   git -C ../codex checkout main
   git -C ../codex pull --ff-only origin main
   ```
3. **Refresh `codex-rs/` mirror**
   ```bash
   rsync -a --delete ../codex/codex-rs/ codex-rs/
   git status --short codex-rs  # should show no drift vs upstream/main
   ```
4. **Verify isolation guard**
   ```bash
   scripts/check-codex-path-deps.sh
   ```
   This script must stay green before/after every merge; it guarantees no crate in `code-rs/` references `../codex-rs`.

---

## Merge Workflow
1. **Stage-friendly merge attempt**
   ```bash
   git checkout main
   git merge upstream/main -X ours --no-commit
   ```
   *Use `--no-commit` so you can `git merge --abort` if the diff is unmanageable.*

2. **Scope control**
   - If the diff explodes (hundreds of files in `code-rs/`), abort and plan phased merges (docs/workflows first, then leaf crates, then core/TUI).
   - Always keep `codex-rs/` untouched; if you need to re-sync mid-merge, redo the rsync step.

3. **Codex/TUI integration**
   - Review upstream TUI/core improvements; selectively port them into `code-rs/` manually after the merge. Keep our themes, browser HUD, multi-agent UX, etc., but don’t ignore upstream bug fixes.
   - For GitHub workflows: compare with `git show upstream/main:.github/workflows/<file>.yml`, cherry-pick any improvements, then restore our workflow set (`git checkout --ours .github/workflows && git clean -fd -- .github/workflows`).
   - Keep local AGENTS/README/CHANGELOG versions unless we intentionally refresh them.

4. **Commit & record the upstream pointer**
   ```bash
   git commit  # include “Merge upstream/main …” context
   ```
   If you end up aborting the staged merge and porting manually, still record a
   merge commit referencing `upstream/main` (e.g., `git merge -s ours upstream/main`)
   so GitHub recognizes that our history contains the upstream tip.
   Ensure `git merge-base --is-ancestor $(git rev-parse upstream/main) HEAD`
   succeeds and `git rev-list --left-right --count upstream/main...HEAD`
   reports `0` behind before you continue work.

---

## Post-Merge Checklist
1. **Regenerate diff reports**
   ```bash
   scripts/upstream-merge/diff-crates.sh --summary
   scripts/upstream-merge/diff-crates.sh --all
   scripts/upstream-merge/highlight-critical-changes.sh --all
   ```
   Review the markdown in `.github/auto/upstream-diffs/critical-changes/` to see which crates need targeted follow-up.

2. **Guardrail + Build**
   ```bash
   scripts/check-codex-path-deps.sh
   ./build-fast.sh
   ```
   Fix *all* errors/warnings. If the build explodes because upstream rewrote something major, reconsider the merge window rather than papering over it.

3. **Report**
   Include in your PR description:
   - Upstream commit hash merged.
   - Highlights of upstream changes adopted vs. intentionally skipped.
   - Any fork-specific cleanups or follow-up tasks (link the critical-change markdown for owners).

---

## Notes & Tips
- `codex-rs/` is now just data; **never edit it directly**. All fixes belong in `code-rs/`.
- `scripts/check-codex-path-deps.sh` is part of CI. If it fails locally, fix the offending `Cargo.toml`/code before pushing.
- Prefer focused merge windows. If upstream touches core + TUI + SDK simultaneously, merge them in separate commits so regressions are easier to spot.
- Keep `../codex` up to date. If you forget to pull upstream before rsync, you’ll end up mirroring stale bits.
- When reviewing TUI diffs, double-check ordering tokens (`request_ordinal`, `output_index`, `sequence_number`) and approval flows—they are easy to regress.

This playbook reflects the Workstream B→C migration where `code-rs/` owns all custom crates and `codex-rs/` tracks upstream verbatim. Follow it each time we ingest `openai/codex:main`.
