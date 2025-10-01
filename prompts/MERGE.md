Goal
◦ Merge the latest `openai/codex:main` changes into our codebase.
◦ Prefer our changes, but adopt upstream TUI improvements where compatible.
◦ Verify only with ./build-fast.sh.

Constraints
◦ Keep our TUI architecture (themes/colors, browser, multi-agent) intact.

Essential Steps
◦ Prep:
  · Stash if dirty. (Optional) create a throwaway local branch from `main` so you can abandon it if the merge goes sideways.
  · Ensure the `upstream` remote exists (`git remote add upstream https://github.com/openai/codex.git` if needed).
  · Pull latest `origin/main` and fetch `upstream/main` (`git fetch origin main && git fetch upstream main`).
◦ Merge:
  · Merge `upstream/main` with `-X ours` (e.g. `git merge upstream/main -X ours`).
  · Resolve conflicts; don’t blanket keep ours for TUI — review diffs and integrate upstream fixes that don’t break our themes/browser/agents. Most of the time you WILL keep our TUI changes, but still review to make sure there's nothing we can merge which is missed.
  · For TUI files (chatwidget, history_cell, bottom_pane/*), pull in upstream improvements selectively; keep our theme hooks (mod colors;), browser HUD, and agent UI.
  · Keep our versions of AGENTS.md, CHANGELOG.md and README.md
  · Apply merge-policy cleanups: consult `.github/merge-policy.json` and remove any `purge_globs`/`perma_removed_paths` entries that reappear after the merge.
  · Double-check TUI invariants: confirm the ordering tokens (`request_ordinal`, `output_index`, `sequence_number`) still exist under `codex-rs/tui/`.
  · Spot-check user-visible “Codex” strings (TUI + docs) for branding regressions if those areas changed.
◦ Build/fix:
  · Run ./build-fast.sh.
  · Fix all errors
  · Fix all warnings
◦ Finalize:
  · Commit the result locally (no push required yet).
  · Ensure `git merge-base --is-ancestor 5b038135dead11f9aa44ecbe5341a859b5ceec69 HEAD` (or whichever upstream commit you targeted) succeeds.
  · If you created a temporary branch, fast-forward `main` locally or cherry-pick as needed once you’re satisfied.
  · If you stashed files, unstash them.

Report:
◦ Finally please produce a report on;
  · What upstream changes were incorporated
  · What upstream changes were dropped
  · Any other code which was cleaned up or changed
