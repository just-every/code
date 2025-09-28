# Tasks: T10 Local-memory Migration

| Order | Task | Owner | Status | Validation |
| --- | --- | --- | --- | --- |
| 1 | Build migration tooling and baseline scripts (dry-run/apply + summary) | Code | In Progress | Baseline + CLI committed; add fixture tests & bulk export helper |
| 2 | Update CLI/TUI flows to prefer local-memory and persist fallbacks | Code | In Progress | `cargo test -p codex-tui spec_auto` (local-memory helper wired, add coverage) |
| 3 | Document workflow and capture migration evidence (baseline, dry-run, apply) | Code | Pending | Evidence JSON/log + doc review |
| 4 | Update SPEC tracker and run lint | Code | Pending | `python3 scripts/spec-kit/lint_tasks.py` |
