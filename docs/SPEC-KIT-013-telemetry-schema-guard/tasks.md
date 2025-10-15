# Tasks: T13 Telemetry Schema Guard

## Task Board
| Order | Task | Owner | Status | Validation |
| --- | --- | --- | --- | --- |
| 1 | Implement stage-aware telemetry validators in `codex-rs/tui/src/chatwidget.rs` | Code | Done | `cargo test -p codex-tui spec_auto` |
| 2 | Enforce schema during `/spec-auto` guardrail collection (fail-fast messaging) | Code | Done | Simulated run via unit tests capturing error lines |
| 3 | Add regression tests for malformed telemetry per stage | Code | Done | `cargo test -p codex-tui spec_auto` |
| 4 | Ensure docs/spec stays current & add inline TODO where guardrail shells must sync | Code | Done | Review `spec.md`, commit message checklist |

## Notes
- Coordinate with guardrail shell owners if validation highlights missing fields; update shells or relax schema with explicit allowlists.
- After implementation, rerun `scripts/spec-kit/lint_tasks.py` and capture `/spec-auto` demo evidence for SPEC tracker.
