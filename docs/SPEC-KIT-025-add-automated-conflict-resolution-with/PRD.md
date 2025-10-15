# PRD: Automated Conflict Resolution Arbiter (T36)

## Summary
- **Objective.** Automate consensus conflict resolution across `/spec-plan`, `/spec-tasks`, `/spec-implement`, and `/spec-auto` by invoking a dedicated GPT-5 arbiter agent whenever agent outputs disagree.
- **Problem.** Today consensus runner halts on conflicts and waits for a human to adjudicate, stretching `/spec-auto` lead time and leaving telemetry without a final verdict.
- **Outcome.** Conflicted stages resolve without manual intervention, telemetry v1 captures arbiter verdict metadata, and operators retain an auditable override path.

## Users & Jobs
- **Spec Kit operators** want `/spec-*` and `/spec-auto` to finish without babysitting stalled consensus stages.
- **Governance & audit reviewers** need durable artefacts showing how conflicts were resolved, which model acted, and why.
- **Reliability engineers** must detect degraded runs quickly and confirm telemetry schema compliance.

## Goals
1. Detect consensus conflicts or ties and launch a GPT-5 arbiter with `reasoning_mode=high` automatically.
2. Persist arbiter verdicts, rationale digests, and model metadata inside consensus artefacts and telemetry v1 without schema changes.
3. Preserve manual override with comprehensive evidence so operators can document exceptional decisions.

## Non-Goals
- Replacing the current Gemini research or Claude synthesis agents.
- Introducing telemetry schema v2; all updates must fit within schema v1 envelopes.
- Altering guardrail hook ordering or `/spec-ops-*` script behavior outside the new arbiter flow.

## Requirements
| ID | Description | Acceptance |
| --- | --- | --- |
| R1 | **Automatic arbiter invocation.** Consensus runner detects `consensus.conflicts`/`ties_pending` in stage artefacts and immediately calls GPT-5 with reasoning mode `high`. | Run logs show arbiter execution without operator prompts; conflicted runs no longer stall unless arbiter responds `unresolved`. |
| R2 | **Complete artefact bundle.** Arbiter input includes Gemini research output, Claude synthesis, prior arbiter verdict JSON (if any), and guardrail context digests. | Evidence directory `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/<SPEC-ID>/` stores a manifest listing each artefact with checksum references. |
| R3 | **Telemetry enrichment.** Telemetry schema v1 payload gains an `arbiter` block (`model`, `model_release`, `reasoning_mode`, `verdict`, `rationale_digest`, `escalated`). | `jq '.arbiter.model' telemetry.jsonl` returns `gpt-5`; existing schema validator passes without modifications. |
| R4 | **Consensus artefact update.** Arbiter verdict merges into stage synthesis JSON with explicit status (`resolved`, `degraded`, `manual_override`) and links back to telemetry artefact IDs. | New consensus files include `arbiter` section; consensus checker accepts them and highlights degraded states. |
| R5 | **Degraded handling + halt.** When arbiter cannot reconcile (missing artefacts, policy breach), mark run `degraded`, halt `/spec-auto`, and surface CLI guidance pointing to evidence + override docs. | Simulated failure produces CLI banner referencing evidence path and records `status="degraded"` in telemetry. |
| R6 | **Manual override logging.** Provide documented override flag (e.g. `/spec-consensus --override`) that records operator, timestamp, reason, and artefact diffs under evidence tree. | Override run writes `override.json` in evidence directory and updates SPEC.md task notes per constitution governance. |
| R7 | **Model strategy compliance.** Implementation obeys `docs/spec-kit/model-strategy.md`; consensus metadata continues to list Gemini → Claude → GPT-5 stack. | Consensus validator rejects any run with unexpected model IDs; smoke tests confirm success path. |
| R8 | **Validation & docs.** Add automated tests (unit/integration) covering resolved conflict, degraded halt, and manual override; update CLAUDE.md and slash-command docs to explain arbiter automation. | `cargo test -p codex-tui spec_auto` (or equivalent suite) passes with new cases; docs mention feature flag and telemetry fields. |

## Dependencies & Risks
- Requires stable consensus runner hooks (T28) and telemetry schema v1 validator updates to accept the new `arbiter` block.
- GPT-5 high reasoning increases cost; need monitoring dashboards and guardrails for repeated retries.
- Missing or corrupted artefacts could mislead arbiter; implement checksum validation before invocation.
- HAL automation and guardrails must continue to function with added latency.

## Rollout & Success Metrics
- Ship behind feature flag `SPEC_KIT_AUTOMATED_ARBITER=1`, roll out stage-by-stage after validating telemetry captures.
- Success when ≥80 % of consensus conflicts resolve automatically with <10 % manual overrides over a 14-day window.
- Maintain <30 % runtime overhead for conflicted runs compared to manual baseline and 100 % telemetry compliance (every conflict run includes `arbiter.verdict`).
- Capture run metrics inside HAL telemetry summaries for governance cost tracking.

## Open Questions
- What retry budget should `/spec-auto` allocate before declaring a run degraded?
- Should manual overrides require secondary approval for production specs?
- How will cost and latency telemetry surface in governance dashboards to flag regression?
