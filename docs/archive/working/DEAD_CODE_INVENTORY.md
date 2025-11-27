# Dead Code Inventory

**Updated:** October 6, 2025

The heavyweight inventory used during the initial cleanup is no longer needed. This lightweight version tracks only the active watchlist so we can focus on forward progress.

## What’s Done
- Removed ~35K lines of legacy tests and deleted 106 files tied to the old codex execution flow.
- Dropped the `legacy_tests` and `vt100-tests` feature flags plus their helper modules.
- Deleted obsolete TUI overlay/backtrack code and archived historic planning notes under `docs/archive/`.
- Converted six infrastructure crates to thin upstream re-exports (`code-ansi-escape`, `code-backend-client`, `code-cloud-tasks-client`, `code-execpolicy`, `code-git-apply`, `code-linux-sandbox`).

## Current Watchlist
- Review `code-rs/core` while the upstream reuse audit runs; flag any modules that can be replaced wholesale once identical.
- Track `code-rs/tui` files still carrying `#![allow(dead_code)]` (mostly streaming helpers) and either legitimize or delete them as part of the history subsystem cleanup.
- Confirm the rebuilt test suites cover vt100 rendering once Phase 3 lands so no dead fixtures sneak back in.

## Quick Verification Tips
- `rg "#!\[allow(dead_code)" code-rs/tui` – sanity-check remaining allowances.
- `./build-fast.sh` – must stay warning-free after any cleanup.
- Note findings directly here so the roadmap can link to a single source of truth.
