# Upstream Merge Report

- Upstream: `openai/codex@main`
- Base: `origin/main`
- Merge branch: `upstream-merge`
- Strategy: allow unrelated histories; keep ours for Rust (`codex-rs/**`) and CLI; drop upstream-only additions unless trivial and non-conflicting.

## Incorporated
- None. This merge links histories without bringing in upstream content changes.

## Dropped
- Upstream-only new files under `codex-rs/**` and `codex-cli/**` (tests, new modules, scripts).
- Upstream `.github/` assets and workflows.
- Upstream additional docs and READMEs where they conflicted (kept ours).

Rationale: maintainers’ policy prefers our implementations and UX; avoid reintroducing removed theming/UX or diverging build/test infra. We will cherry-pick specific upstream improvements separately.

## Conflict Resolution
- For files present in both trees (AA conflicts), resolved to `ours` across the board.
- For files added only in upstream (A), removed them from the merge to respect default-to-ours policy.

## Build Status
- Attempted `./build-fast.sh`. Environment sandbox denied cargo registry writes (`Permission denied` to `.cargo-home/registry/...`).
- No code changes were introduced by this merge; functional behavior remains identical to `origin/main`.
- Action needed: run `./build-fast.sh` locally or in CI with normal permissions. We expect a clean build, identical to pre-merge state.

## Next Steps
- If we want any upstream features or docs, propose targeted cherry-picks/ports with focused PRs.
