**SPEC-ID**: SPEC-KIT-069
**Feature**: Stabilize /speckit.validate agent orchestration
**Status**: Backlog
**Created**: 2025-10-23
**Branch**: (pending)
**Owner**: Code

**Context**: Running `/speckit.validate` currently emits repeated `agent_run` invocations, immediate cancel cascades, and placeholder tasks because multiple callbacks race to re-submit the stage. This produces noisy telemetry, wastes credits, and leaves evidence folders with duplicated artifacts. The fix must make validate stage scheduling single-flight, align retries with AR-2 policy, and ensure local-memory artifacts remain canonical. Relevant orchestration resides in `codex-rs/tui/src/chatwidget/spec_kit/handler.rs` (auto submission and completion handlers), `state.rs` (`validate_retries`, `SpecAutoPhase`), `quality_gate_handler.rs` (multi-agent fan-out patterns), and `routing.rs` (subagent command formatting).

---

## User Scenarios

### P1: Deterministic manual validate run

**Story**: As a maintainer invoking `/speckit.validate SPEC-KIT-###`, I want exactly one trio of validation agents to execute so the evidence and telemetry reflect a single run.

**Priority Rationale**: Manual checkpoints must be predictable to keep maintainers’ trust in automation.

**Testability**: Inspect local-memory (`spec:SPEC-KIT-### stage:validate`) to confirm one artifact per configured agent; verify `docs/SPEC-OPS-004.../commands/SPEC-KIT-###/validate_*` contains a single run-id.

**Acceptance Scenarios**:
- Given `/speckit.validate SPEC-KIT-069`, when multiple `AgentStatusUpdateEvent`s arrive in quick succession, then no new `agent_run` call is issued after the first dispatch.
- Given the user re-enters the command while the stage is active, then the TUI surfaces a dedupe notice and does not spawn another agent set.
- Given a manual cancel, then all agents stop once and no placeholder tasks remain in the task list.

### P2: Implement→Validate retry cycle in auto mode

**Story**: As a maintainer using `/speckit.auto`, I want the automatic Implement→Validate retry loop to schedule validate exactly once per retry attempt so that retries are bounded and observable.

**Priority Rationale**: Retries inflate cost when duplicate validate runs occur; bounding them keeps budgets and timelines sane.

**Testability**: Induce a failing validation; observe that each injected retry adds a single validate run with the same `stage_run_id` reused for telemetry, never spawning extraneous agents.

**Acceptance Scenarios**:
- Given a validation failure, when the retry path re-inserts Implement→Validate, then the validate stage reuses the existing attempt token or issues exactly one new run-id.
- Given retry exhaustion, then the pipeline halts with a clear message and no additional validate spawns are queued.

### P3: Manual + auto coexistence

**Story**: As an operator, if I fire a manual `/speckit.validate` while an automated run is active, I need the system to serialize runs or reject the manual attempt cleanly to avoid interleaved evidence.

**Priority Rationale**: Support for operational overrides without corrupting automation state.

**Testability**: Attempt to launch manual validate during an auto cycle; confirm the command returns a UX warning referencing the active run and refuses to double-schedule.

**Acceptance Scenarios**:
- Given an active validate run (manual or auto), when a second trigger is issued, then the handler declines with `stage_run_id` info and collects no new telemetry.
- Given the prior run finishes, when a new trigger is issued, then the orchestrator dispatches agents once and refreshes the `stage_run_id`.

---

## Edge Cases

- Duplicate invocations of `on_spec_auto_agents_complete()` while consensus is still pending.
- Guardrail retry rewrites that re-enter validate scheduling.
- Late-arriving local-memory artifacts that should not resurrect a cancelled run.
- UI redraws reusing cached prompts that could re-submit without checking active state.
- Concurrent telemetry persistence failures that must not drop the single-flight guard.

---

## Requirements

### Functional Requirements

- **FR1**: Track a unique `stage_run_id` for each validate attempt in `SpecAutoState`, enforcing compare-and-set semantics before any agent dispatch.
- **FR2**: Make `auto_submit_spec_stage_prompt()` idempotent for `SpecStage::Validate` by hashing the request payload and exiting early if an active run exists.
- **FR3**: Route cancel signals through a single cancellation token per run-id and ensure task list cleanup removes placeholder items tied to the completed/cancelled run.
- **FR4**: Record run lifecycle (queued → dispatched → consensus → complete/cancel) to local-memory with tags `spec:SPEC-KIT-###`, `stage:validate`, `artifact:agent_lifecycle` and importance ≥8.

### Non-Functional Requirements

- **Performance**: Validate scheduling overhead remains ≤15 ms beyond current baseline (consensus check still ~8.7 ms).
- **Reliability**: Duplicate dispatch probability <0.1% across 500 simulated runs with randomized event ordering.
- **Observability**: Telemetry and evidence clearly expose run-id, agent collection, retries, and dedupe decisions.
- **Cost Control**: Wasted agent invocations reduced by ≥90% compared to pre-fix baseline logs (Oct 22, 2025 sample).

---

## Success Criteria

- `/speckit.validate` emits exactly three agent spawn events per run (gemini, claude, gpt_pro) with no redundant `agent_run` calls captured in spec-kit logs.
- Evidence directories and local-memory search show a single artifact set per validate attempt.
- Auto retry path respects configured retry limits without over-scheduling validate.
- Telemetry dashboards reflect new deduplication counters and stage_run_id references.

---

## Task Breakdown (2025-10-23)

| Task ID | Summary | Dependencies | Validation Hooks | Documentation Touch |
|---------|---------|--------------|------------------|---------------------|
| **T69-01** | Extend `SpecAutoState` with validate lifecycle fields and `ValidateLifecycleStatus` helpers (`codex-rs/tui/src/chatwidget/spec_kit/state.rs:69`). | None | `cargo test -p codex-tui state::validate_lifecycle_transitions` | Update lifecycle description in this spec (Section: Requirements). |
| **T69-02** | Implement lifecycle helper APIs (`begin_validate_run`, `finish_validate_run`, `reset_validate_run`) providing compare-and-set semantics (`codex-rs/tui/src/chatwidget/spec_kit/state.rs:69`). | T69-01 | Unit tests asserting single-flight behaviour; property test covering invalid transitions. | Add helper reference to plan.md (Step 1/2 alignment). |
| **T69-03** | Add single-flight guard and dedupe notice to `auto_submit_spec_stage_prompt()` (`codex-rs/tui/src/chatwidget/spec_kit/handler.rs:518`). | T69-01, T69-02 | Integration: `cargo test -p codex-tui spec_auto_e2e::validate_single_flight_storm`; manual rerun of `/speckit.validate SPEC-KIT-069`. | Capture UX copy in `CLAUDE.md` command reference. |
| **T69-04** | Gate `on_spec_auto_agents_complete()` and `check_consensus_and_advance_spec_auto()` on active run-id to ignore late callbacks (`codex-rs/tui/src/chatwidget/spec_kit/handler.rs:599`, `handler.rs:761`). | T69-01 – T69-03 | Integration: `spec_auto_e2e::validate_manual_auto_coexistence`; verifies single consensus transition. | Document callback guard rationale in `docs/spec-kit/spec-auto-automation.md`. |
| **T69-05** | Align Implement→Validate retry loop with lifecycle resets and cancel hygiene (`codex-rs/tui/src/chatwidget/spec_kit/handler.rs:409`). | T69-01 – T69-04 | Integration: `spec_auto_e2e::validate_retry_cycle`; manual cancel flow test; confirms capped retries. | Add retry/cancel notes to PRD acceptance tables. |
| **T69-06** | Persist validate lifecycle telemetry via MCP (`remember_validate_lifecycle`) (`codex-rs/tui/src/chatwidget/spec_kit/consensus.rs:966`). | T69-01 – T69-05 | `local-memory search "spec:SPEC-KIT-069 stage:validate artifact:agent_lifecycle"`; unit test mocking MCP store. | Add new tag guidance to `docs/spec-kit/evidence-policy.md`. |
| **T69-07** | Extend filesystem telemetry with run-id, attempt, dedupe count (`codex-rs/tui/src/chatwidget/spec_kit/evidence.rs:60`). | T69-06 | Validate telemetry JSON in `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-069/validate_*`; smoke test ensures backward compatibility. | Update telemetry schema description in PRD §Evidence & Telemetry. |
| **T69-08** | Emit operator-facing dedupe notice when validate is already running (`codex-rs/tui/src/chatwidget/spec_kit/handler.rs:518`). | T69-03 | Manual check: re-trigger `/speckit.validate` during active run; ensure no agent dispatch. | Add UX string to `docs/spec-kit/spec-auto-automation.md`. |
| **T69-09** | Expand automated coverage: unit, E2E, stress (500 shuffled events) and micro-bench to keep overhead ≤15 ms (`codex-rs/tui/tests/spec_auto_e2e.rs:248`). | T69-01 – T69-08 | `cargo test -p codex-tui spec_auto_e2e::validate_*`; `cargo bench spec_kit::validate_guard_overhead`. | Record benchmark artefacts in evidence directory per plan Step 5. |
| **T69-10** | Refresh documentation (spec, PRD, automation guide) with lifecycle semantics, degraded-mode notes, and search examples. | T69-06 – T69-09 | Documentation review; run `/speckit.analyze SPEC-KIT-069` to confirm sync. | Update PRD acceptance mapping & evidence references. |

---

## Tasks Consensus Summary

- **Degraded participation**: gemini agent produced no usable output (empty artifact `.code/agents/9094fefd-7ac1-45f9-ad7d-1be84a4c4b92/result.txt`), so consensus proceeded with claude + gpt_pro only. Marked as degraded for downstream tracking.
- **Agreements (claude + gpt_pro)**:
  - Validate-specific lifecycle guard with CAS semantics (Tasks T69-01‒T69-04).
  - Telemetry tagging via local-memory (`spec:SPEC-KIT-069`, `stage:validate`, `artifact:agent_lifecycle`) and evidence JSON augmentation (T69-06/T69-07).
  - Comprehensive verification suite including stress + bench work (T69-09).
- **Disagreement & resolution**: Scope of lifecycle helpers. Claude proposed validate-only helpers now; gpt_pro concurred while noting structure should allow future reuse. Consensus: implement validate-focused helpers with extensible naming, revisit generalisation if additional stages require it.

---

## Unresolved Risks (Tasks Stage)

1. **Stale run-id after unexpected crash** — Mitigation pending implementation of inactivity timeout (≥30 min) and explicit reset path in `halt_spec_auto_with_error` (`codex-rs/tui/src/chatwidget/spec_kit/handler.rs:685`). Owner: Code.
2. **Telemetry consumers parsing new fields** — Guardrail scripts must tolerate optional `stage_run_id`/`validate_attempt` fields; coordinate validation after T69-07. Owner: Code + Docs.

---

## Evidence & Validation

**Acceptance Tests**: Implement integration tests in `codex-rs/tui/tests/spec_auto_e2e.rs` and targeted unit coverage in `handler.rs` verifying stage_run_id guards; add load test to `spec-kit` bench harness simulating rapid callback storms.

**Telemetry Path**: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-069/validate_<timestamp>_telemetry.json`

**Consensus Evidence**: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-069/validate_<timestamp>_verdict.json`

**Validation Commands**:
```bash
/speckit.validate SPEC-KIT-069
/speckit.auto SPEC-KIT-069 --from implement --until validate
/spec-consensus SPEC-KIT-069 validate
```

---

## Clarifications

### 2025-10-23 - Initial Spec Creation

**Clarification needed**: Do we enforce serialization at command parsing time or within the orchestrator state machine?

**Resolution**: Pending design spike; default assumption is orchestrator-level guard with UX feedback.

**Updated sections**: To be determined during `/speckit.plan`.

---

## Dependencies

- Stable retry policy constants defined in `codex-rs/tui/src/chatwidget/spec_kit/handler.rs` (AR-2/AR-3 implementations).
- Local-memory MCP availability for tagging run lifecycle artifacts.
- Consensus module (`consensus.rs`) for validate stage verification.

---

## Notes

- Coordinate with SPEC-KIT-068 quality gate restoration to reuse shared single-flight utilities if implemented there.
- Ensure UX copy references the new dedupe guard (e.g., “Validate run already active (run-id …)”).
