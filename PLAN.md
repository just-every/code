# Next-Step Plan (Post Workstreams B → C)

## Current State
- `code-rs/` owns every forked crate; no crate depends on `../codex-rs` (guarded by `scripts/check-codex-path-deps.sh`).
- `codex-rs/` is synced to `openai/codex@c3951e50` and treated as a pure upstream mirror.
- Diff tooling (`scripts/upstream-merge/diff-crates.sh`, `highlight-critical-changes`) highlights where our fork diverges.

## Workstream C Priorities
1. **Triage Critical Diffs**
   - Review `.github/auto/upstream-diffs/critical-changes/*.md`.
   - For each crate (core, app-server, apply-patch, etc.), decide whether to adopt upstream wholesale, selectively port fixes, or keep the forked version.

2. **Identify Re-export Candidates**
   - Crates now identical between `codex-rs` and `code-rs` (e.g., git-apply, linux-sandbox, responses-api-proxy) can potentially be removed from `code-rs` in favor of upstream to reduce maintenance.
   - Document which crates can be dropped vs. which require fork-only patches.

3. **Plan Merge Windows**
   - Use the updated `prompts/MERGE.md` playbook for each upstream intake.
   - Prefer small, focused merges (e.g., TUI-only, CLI-only) followed by testing and guard verification.

4. **Port Upstream Improvements**
   - Where we lag (e.g., new config loader, execpolicy2, feedback view), cherry-pick or manually port upstream commits into `code-rs` so future merges shrink.
   - Keep fork-specific UX (themes, browser) while importing upstream bug fixes.

5. **Clean Up Unused Crates**
   - Large upstream-only crates (async-utils, app-server-test-client, windows-sandbox-rs, etc.) exist solely under `codex-rs`. Confirm they’re unnecessary for the fork and, if so, exclude them from future workstreams.

6. **Automation**
   - `scripts/check-codex-path-deps.sh` now runs automatically at the start of `./build-fast.sh`, so both local builds and GH workflows that invoke it enforce the guard.
   - Continue regenerating diff/critical reports after every upstream merge to maintain visibility.

## Success Criteria
- Regular merges from `openai/codex:main` proceed with minimal conflict.
- Critical upstream fixes/features are either upstreamed back or intentionally tracked in `code-rs` diffs.
- `codex-rs/` remains a byte-for-byte mirror at every merge point.
