# Tasks: T36 SPEC-KIT-025 Automated Conflict Resolution Arbiter

| Order | Task | Owner | Status | Validation |
| --- | --- | --- | --- | --- |
| 1 | Wire GPT-5 arbiter invocation across `scripts/spec_ops_004/consensus_runner.sh` and new `codex-rs/tui/src/spec_auto/arbiter.rs`, gate with `SPEC_KIT_AUTOMATED_ARBITER`, bundle Gemini/Claude artefacts, update consensus state, and persist evidence under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-025/`. | Code | Backlog | `cargo test -p codex-tui spec_auto::arbiter_triggers_on_conflict`; verify artefact bundle via `ls docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-025/arbiter*` |
| 2 | Extend telemetry schema v1 emitters (bash + Rust) with optional `arbiter` block holding model metadata, verdict, rationale digest, retry counters, and escalation flag while keeping validators green. | Code | Backlog | `scripts/validate_telemetry.py --schema v1 --feature arbiter`; `cargo test -p codex-tui telemetry::arbiter_block_backward_compat` |
| 3 | Implement degraded handling and manual override controls: enforce retry budget, annotate `missing_agents` (Qwen gap) and `degraded` state, expose CLI override flag, and log operator/rationale artefact diffs. | Code | Backlog | `cargo test -p codex-tui spec_auto::arbiter_degraded_retry_budget`; manual smoke `scripts/spec_ops_004/spec_auto.sh SPEC-KIT-025 --manual-resolve --dry-run` with `grep MANUAL_OVERRIDE docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-025/*.log` |
| 4 | Add unit + integration coverage for happy, degraded, and override flows plus consensus metadata snapshots within `codex-rs/tui/tests/spec_auto/arbiter.rs` and consensus runner fixtures. | Code | Backlog | `cargo test -p codex-tui spec_auto::arbiter`; capture `/spec-auto SPEC-KIT-025 --dry-run` evidence showing updated telemetry |
| 5 | Update operator documentation (CLAUDE.md, docs/slash-commands.md, docs/spec-kit/model-strategy.md) and changelog to describe automated arbitration, telemetry fields, cost monitoring, and override workflow; sync SPEC tracker notes. | Code | Backlog | `scripts/doc-structure-validate.sh --mode=templates`; `python3 scripts/spec-kit/lint_tasks.py` |

## Notes
- Ensure telemetry validators and consensus runner updates land together so `/spec-ops-plan` continues to pass; schema drift will halt guardrails.
- According to Byterover memory layer, the PRD emphasises auditability of arbiter verdicts—persist model metadata and rationale in telemetry JSON and evidence bundles.
- Document the accepted Qwen degradation from the plan and schedule follow-up validation once the agent returns; include cost telemetry hooks so GPT-5 spend is observable.
