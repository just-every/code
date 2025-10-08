# Plan: SPEC-KIT-035 spec-status diagnostics
## Inputs
- Spec: docs/SPEC-KIT-035-spec-status-diagnostics/spec.md (pending; interim requirements from product owner brief dated 2025-10-07)
- Constitution: memory/constitution.md (4e159c7eccd2cba0114315e385584abc0106834c)

## Work Breakdown
1. **Establish baseline context and artifacts.** Confirm SPEC packet scaffolding (PRD/spec/plan/tasks) and SPEC.md tracker row exist for SPEC-KIT-035; capture any gaps as blockers. Audit current `scripts/spec_ops_004/commands/spec_ops_status.sh` output and evidence directories to catalogue missing data points called out in the brief and, according to Byterover memory layer, prior telemetry coverage gaps (HAL summaries, policy layers, checksum manifests).
2. **Design data ingestion & modeling layer.** Introduce a Rust module (e.g., `codex-rs/tui/src/spec_status.rs`) that discovers SPEC packets, loads guardrail telemetry (schema v1 + v2), consensus synthesis, agent artifacts, and evidence sizes. Model stage health, agent activity, policy/HAL/baseline verdicts, timestamps, and degradation states with graceful fallbacks for missing or malformed JSON.
3. **Implement TUI-facing report generation.** Wire `/spec-status` to call the new aggregator and render a structured dashboard in the chat output. Prioritize an MVP markdown-style layout (header, SPEC packet health, SPEC.md tracker, stage table, blockers) that fits within existing TUI ergonomics, while documenting optional enhancements (collapsible sections, richer agent tables) for a follow-on iteration.
4. **Refine guardrail scripts & telemetry hooks.** Extend shared helpers (`scripts/spec_ops_004/common.sh`) or stage scripts as needed to emit machine-readable timestamps, HAL/policy summaries, agent metadata, and evidence footprint hints so the dashboard does not rely on brittle text parsing. Keep `spec_ops_status.sh` as a CLI fallback but align its sections with the new data model.
5. **Build validation & fixture suite.** Create unit tests for telemetry/consensus parsing, SPEC packet checks, and formatter helpers; add integration tests with fixture evidence trees that cover healthy, stale, failing, and conflict scenarios. Exercise `/spec-status SPEC-KIT-DEMO` manually during guardrail runs to ensure read safety while guardrails stream.
6. **Document workflows and ship readiness.** Update `docs/slash-commands.md`, CLAUDE.md troubleshooting, and a new SPEC-KIT-035 page describing interpretation of the dashboard and remediation steps (stale telemetry, HAL failures, consensus conflicts, oversized evidence). Ensure SPEC.md tasks lint passes once tracker notes are updated and capture next-actions guidance in local-memory.

## Acceptance Mapping
| Requirement (Spec) | Validation Step | Test/Check Artifact |
| --- | --- | --- |
| R1: Stage/guardrail/consensus status visible for all six stages | Integration test invoking `/spec-status` against fixtures with pass/fail/conflict telemetry | `codex-rs/tui/tests/spec_status_integration.rs` (fixture bundle) |
| R2: SPEC packet & SPEC.md tracker verification | Unit test for packet checker + manual invocation on SPEC-KIT-DEMO missing files | `spec_status::packet_checks` unit test & CLI screenshot |
| R3: Execution timeline and evidence footprint reported | Fixture with synthetic timestamps and large artifacts to assert warning thresholds | `tests/fixtures/spec_status/stale_and_large` dataset + snapshot |
| R4: Failure diagnostics and remediation hints surfaced | Inject malformed baseline/HAL telemetry and confirm blockers list actionable guidance | Integration scenario `spec_status_failures.json` |
| R5: Agent tracking and policy/HAL summaries aggregated | Unit tests covering consensus synthesis parsing and policy/hal blocks; manual run with `SPEC_OPS_TELEMETRY_HAL=1` | `spec_status::consensus_from_json` tests + guardrail log capture |

## Risks & Unknowns
- Spec packet for SPEC-KIT-035 not yet generated (`/specify` pending); treat creation of PRD/spec/tasks as gating prerequisite for implementation.
- Telemetry schemas may drift; parsing must handle legacy schema v1, optional HAL summaries, and absent policy layers without crashing.
- Evidence trees can be large; need efficient discovery (latest file per stage) to avoid TUI lag, especially while guardrails stream.
- HAL access and policy commands remain conditional on secrets/tool availability; dashboard must clearly mark "skipped" vs "failed" to avoid confusion.
- Scope creep toward fully interactive UI or live refresh could delay delivery; enforce MVP boundary and track enhancements separately.

## Consensus & Risks (Multi-AI)
- Agreement: All agents converged on building a Rust aggregation layer, presenting a concise TUI dashboard with SPEC packet checks, stage health, agent metrics, and blockers, plus investing in thorough fixtures/tests and documentation to support debugging workflows.
- Disagreement & resolution: Gemini and Code favored delivering a markdown-style report first, while Claude advocated for a richer interactive overlay. We will ship the textual dashboard MVP now and log interactive controls and live refresh as follow-up enhancements to keep timelines controllable.

## Exit Criteria (Done)
- `/spec-status SPEC-ID` renders the MVP dashboard with packet health, tracker row, stage table, agent summaries, timeline, and blockers populated from real telemetry.
- Parsers tolerate schema v1/v2 artifacts, surfacing clear degraded states rather than panicking on gaps.
- Unit, integration, and doc validators pass; manual runs on at least SPEC-KIT-DEMO and one active SPEC match expectations.
- Guardrail telemetry updates (if any) merged with shellcheck coverage and evidence directory growth documented.
- Slash-command docs, CLAUDE.md, and SPEC.md notes updated with new workflow guidance; local-memory captures decision history and remediation playbooks.
