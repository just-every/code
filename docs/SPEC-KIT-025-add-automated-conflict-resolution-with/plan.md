# Plan: SPEC-KIT-025-add-automated-conflict-resolution-with
## Inputs
- Spec: docs/SPEC-KIT-025-add-automated-conflict-resolution-with/spec.md (16b96150)
- Constitution: memory/constitution.md (4e159c7e)

## Work Breakdown
1. Wire `SPEC_KIT_AUTOMATED_ARBITER` feature flag into consensus runner so conflicts detected via `jq '.consensus.conflicts'` trigger GPT-5 arbiter calls automatically, bundling Gemini/Claude artefacts and guardrail diagnostics (According to Byterover memory layer).
2. Extend telemetry schema v1 emission with a backward-compatible `arbiter` block (model metadata, verdict, rationale digest, retry counters, escalation flag) and persist manifests/verdicts under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-025/`.
3. Implement degraded-path controls: enforce retry budget, annotate `degraded` status and `missing_agents` (note Qwen gap) in consensus metadata, halt `/spec-auto`, and surface CLI guidance plus override instructions.
4. Capture manual override flow (`/spec-consensus --override` or equivalent) that records operator, rationale, chosen verdict, and artefact diffs, ensuring SPEC.md notes update per constitution governance.
5. Update documentation (CLAUDE.md, docs/slash-commands.md, docs/spec-kit/model-strategy.md, PRD/spec cross-links) to describe automated arbitration, telemetry additions, cost monitoring, and override procedures.
6. Add automated tests and validation hooks: resolved vs degraded vs override scenarios, telemetry snapshots, guardrail integration; run `cargo test -p codex-tui spec_auto::arbiter*`, `scripts/spec-kit/lint_tasks.py`, `scripts/doc-structure-validate.sh --mode=templates`, and `scripts/spec_ops_004/baseline_audit.sh --out docs/SPEC-OPS-004-integrated-coder-hooks/baseline.md`.

## Acceptance Mapping
| Requirement (Spec) | Validation Step | Test/Check Artifact |
| --- | --- | --- |
| R1: Auto GPT-5 arbiter invocation on conflicts | `cargo test -p codex-tui spec_auto::arbiter_triggers_on_conflict` | tests/spec_auto/arbiter.rs::arbiter_triggers_on_conflict |
| R2: Artefact bundling for arbitration | `cargo test -p codex-tui spec_auto::arbiter_bundles_artifacts` | tests/spec_auto/arbiter.rs::arbiter_bundles_artifacts |
| R3: Telemetry `arbiter` block enrichment (schema v1 compatible) | `cargo test -p spec-ops-telemetry telemetry::arbiter_block_backward_compat` | tests/telemetry/arbiter_block.rs::backward_compat_snapshot |
| R4: Consensus metadata updates post-arbitration | `cargo test -p codex-tui spec_auto::arbiter_updates_consensus` | tests/spec_auto/arbiter.rs::updates_consensus_metadata |
| R5: Degraded handling, retry budgets, and cost monitoring | `cargo test -p codex-tui spec_auto::arbiter_degraded_retry_budget` | tests/spec_auto/arbiter.rs::degraded_retry_budget |
| R6: Manual override logging with evidence linkage | `cargo test -p codex-tui spec_auto::arbiter_manual_override_logging` | tests/spec_auto/arbiter.rs::manual_override_logging |
| R7: Model strategy alignment & guardrail compliance | `cargo test -p codex-tui spec_auto::arbiter_model_strategy_guardrail` | tests/spec_auto/arbiter.rs::model_strategy_guardrail |
| R8: Docs & validation updates | `scripts/doc-structure-validate.sh --mode=templates` & `scripts/spec-kit/lint_tasks.py` | docs/slash-commands.md diff & validator logs |

## Risks & Unknowns
- Telemetry consumers may break if optional `arbiter` fields drift; review golden snapshots with telemetry schema guard maintainers.
- GPT-5 high reasoning retries can raise cost; ensure cost telemetry hooks fire before enabling flag broadly.
- Lack of Qwen agent during development limits degraded-path validation; schedule follow-up tests when agent access returns.

## Consensus & Risks (Multi-AI)
- Agreement: `gemini-2.5-pro` (research), `claude-4.5-sonnet` (synthesis), and `gpt-5` (arbiter, reasoning high) concur on staged rollout, telemetry enrichment, and validation coverage.
- Disagreement & resolution: Qwen agent unavailable, so plan records degraded consensus metadata and mandates regression tests that assert `missing_agents` handling before GA.

## Exit Criteria (Done)
- All acceptance checks pass
- Docs updated (CLAUDE.md, docs/slash-commands.md, docs/spec-kit/model-strategy.md, docs/SPEC-KIT-025-add-automated-conflict-resolution-with/{PRD.md,spec.md})
- Changelog/PR prepared
