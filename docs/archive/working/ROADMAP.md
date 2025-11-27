# Roadmap

## Current Objectives
- Reuse upstream `codex-*` crates and modules wherever behavior matches, keeping fork-only UX, policy, and tooling layers in `code-rs`.
- Rebuild a lean, representative test suite that exercises current workflows instead of legacy codex paths.
- Maintain a predictable cadence for upstream diffs and documentation so merges stay low-drama.

## Recent Milestones *(Completed October 5, 2025)*
- Retired the legacy vt100 and executor test suites (~35K lines removed, 106 files).
- Converted six infrastructure crates (`code-ansi-escape`, `code-backend-client`, `code-cloud-tasks-client`, `code-execpolicy`, `code-git-apply`, `code-linux-sandbox`) into thin re-exports of their upstream counterparts.
- Pruned obsolete TUI overlay/backtrack modules and archived earlier planning artifacts.

## Active Initiatives

### 1. Upstream Reuse Audit
- Catalogue modules in `code-rs/core`, `code-rs/exec`, and `code-rs/tui` that are still identical to upstream.
- Replace identical modules with `pub use codex_*::…` re-exports; retain fork-only policy, approval, and UX layers locally.
- After each pass, capture remaining divergences in-line with the module list and update this roadmap.

### 2. Test Suite Rebuild *(October 20 – November 30, 2025)*
- Port six core runtime tests (`model_tools.rs`, `tool_harness.rs`, `tools.rs`, `read_file.rs`, `view_image.rs`, `unified_exec.rs`).
- Port three TUI rendering tests with deterministic fixtures and golden outputs.
- Expand smoke harness coverage for exec, approval, and tool flows using the helpers documented in `TEST_SUITE_RESET.md`.

### 3. Maintenance & Tooling Ready State
- Run the first-Monday diff cadence with `scripts/upstream-merge/diff-crates.sh --all`; escalate to critical analysis only when changes warrant it.
- Track prompt/config deltas through `docs/maintenance/upstream-diff.md` so downstream teams can react quickly.
- Monitor thin wrapper crates for API drift and keep their configuration nubs documented in the crate READMEs.

## Next Actions
- [ ] Prepare a fresh `codex-rs` checkout (or update the mirror) for upcoming module comparisons.
- [ ] Diff `code-rs/core/src/history/` against upstream and replace identical modules with re-exports.
- [ ] Kick off Phase 2 of the test rebuild by porting `model_tools.rs`.
- [ ] Schedule the next upstream diff review on October 13, 2025 (second-Monday merge planning hold).

## Reference Docs
- `DEAD_CODE_INVENTORY.md` – current cleanup watchlist and outstanding audits.
- `TEST_SUITE_RESET.md` – smoke harness coverage and remaining porting tasks.
- `docs/maintenance/upstream-diff.md` – cadence, tooling, and logging workflow for upstream merges.
