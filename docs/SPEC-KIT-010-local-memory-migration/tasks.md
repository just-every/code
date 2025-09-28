# Tasks: T10 Local-memory Migration

| Order | Task | Owner | Status | Validation |
| --- | --- | --- | --- | --- |
| 1 | Build migration tooling and baseline scripts (dry-run/apply + summary) | Code | Done | Fixture-backed tests complete; dry-run/apply evidence stored under SPEC-KIT-010 |
| 2 | Update CLI/TUI flows to prefer local-memory and persist fallbacks | Code | Done | `cargo test -p codex-tui spec_auto` (2025-09-28) |
| 3 | Document workflow and capture migration evidence (baseline, dry-run, apply) | Code | Done | Runbook + evidence bundle committed (2025-09-28) |
| 4 | Update SPEC tracker and run lint | Code | Done | `python3 scripts/spec-kit/lint_tasks.py` (2025-09-28) |
