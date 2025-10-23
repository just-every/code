# PRD: Stabilize /speckit.validate agent orchestration

**SPEC-ID**: SPEC-KIT-069
**Status**: Draft
**Created**: 2025-10-23
**Author**: Multi-agent consensus (gemini, claude, code)

---

## Problem Statement

**Current State**: Triggering `/speckit.validate` (manually or via `/speckit.auto`) issues repeated `agent_run` commands, produces instant cancel events, and leaves placeholder tasks because validate scheduling lacks single-flight guards. Evidence folders contain duplicate artifacts and telemetry obscures true run counts.

**Pain Points**:
- Duplicate agent invocations inflate model spend and delay consensus.
- Cancel storms spam the TUI history, confusing operators and obscuring real failures.
- Placeholder tasks linger after cancellation, making pipeline status unclear.

**Impact**: The validate stage no longer provides trustworthy signal, forcing manual babysitting and undermining the promise of autonomous quality gates.

---

## Target Users & Use Cases

### Primary User: Maintainer running `/speckit.auto`

**Profile**: Senior contributor managing Spec-Kit pipelines for feature delivery.

**Current Workflow**: Launches `/speckit.auto SPEC-ID`, monitors TUI, manually intervenes when validate enters cancel loops.

**Pain Points**: Credited runs multiply, consensus artifacts conflict, and retries exceed expected counts.

**Desired Outcome**: Validate executes once per cycle, with telemetry highlighting success/failure clearly.

### Secondary User: Incident responder investigating automation glitches

**Profile**: Engineer on call to diagnose Spec-Kit runtime regressions.

**Use Case**: Replays `/speckit.validate` manually to gather evidence and confirm fixes.

---

## Goals

### Primary Goals

1. **Eliminate duplicate validate agent dispatches**: Introduce atomic scheduling so only one agent batch runs per validate attempt.
   **Success Metric**: ≤1.0 agent batches per attempt across 50 stress runs (baseline >2.4).

2. **Clarify telemetry and evidence**: Persist run lifecycle metadata and dedupe counters to local-memory and evidence files.
   **Success Metric**: 100% validate runs exhibit single run-id across telemetry + evidence.

### Secondary Goals

1. **Bound retries to AR-2 policy**: Align Implement→Validate retry loop with configurable max attempts and expose exhaustion cause in UX.

---

## Non-Goals

**Explicitly Out of Scope**:
- Replacing validate agents or models (agent roster remains gemini, claude, gpt_pro).
- Refactoring guardrail or audit stages beyond shared helper extraction.
- Implementing cloud telemetry dashboards (local evidence remains primary signal).

**Rationale**: Focus on orchestration correctness; broader refactors belong to future specs.

---

## Scope & Assumptions

**In Scope**:
- Apply single-flight guardrails to validate scheduling paths (`handler.rs`, `routing.rs`).
- Ensure UI messaging surfaces active run-id and dedupe outcomes.
- Extend integration tests covering validate retries and concurrency.

**Assumptions**:
- Local-memory MCP is available for tagging lifecycle artifacts.
- Retry constants from AR-2/AR-3 remain unchanged (3 attempts max).
- Evidence storage quota (25 MB soft limit) is sufficient after duplication removal.

**Constraints**:
- Must preserve existing command interfaces (`/speckit.validate`, `/speckit.auto`).
- Changes must ship with regression tests (unit + integration) before enabling in production TUI.
- Avoid blocking Ratatui event loop; guard logic must execute within current sync boundary.

---

## Functional Requirements

| ID | Requirement | Acceptance Criteria | Priority |
|----|-------------|---------------------|----------|
| FR1 | Introduce `stage_run_id` compare-and-set before dispatching validate agents | Dispatch occurs only when no active run-id exists; dedupe notice logged otherwise | P1 |
| FR2 | Persist validate run lifecycle and dedupe telemetry to local-memory | Entries tagged `spec:SPEC-KIT-069 stage:validate` appear for each lifecycle state | P1 |
| FR3 | Align Implement→Validate retry loop to reuse run-id and cap retries | Retry counter stops at configured max; no extra dispatches appear | P1 |
| FR4 | Clean up placeholder tasks on cancel/completion | Task list contains zero orphaned validate placeholders after run | P2 |

---

## Non-Functional Requirements

| ID | Requirement | Target Metric | Validation Method |
|----|-------------|---------------|-------------------|
| NFR1 | Performance | Validate dispatch guard adds ≤15 ms latency | Benchmark harness simulating 500 events | Bench test (`cargo test --bench spec_kit`) |
| NFR2 | Reliability | Duplicate dispatch rate <0.1% under randomized callbacks | Stress test in `spec_auto_e2e.rs` with shuffled events | Integration test |
| NFR3 | Observability | Telemetry exposes run-id, dedupe counter, retry attempts | Inspect TUI history + evidence JSON | Manual validation |
| NFR4 | Cost Control | Credit usage reduced ≥90% vs. baseline duplicate logs | Compare Oct 22 vs. post-fix run telemetry | Evidence analysis |

---

## User Experience

**Key Workflows**:

### Workflow 1: Manual `/speckit.validate`

**Steps**:
1. User runs `/speckit.validate SPEC-KIT-069`.
2. System checks `stage_run_id` and dispatches agents once.
3. TUI shows progress with run-id and dedupe counter = 0.
4. System collects consensus, stores evidence, renders success or failure.

**Success Path**: Single set of agent outputs, consensus verdict stored, UX indicates completion.

**Error Paths**:
- If dedupe guard denies dispatch: UX displays “Validate run already active (run-id …)”.
- If consensus fails: retry cycle engages respecting AR-2 policy.

### Workflow 2: Auto retry cycle

**Steps**:
1. `/speckit.auto` completes Implement stage.
2. System queues validate run with new `stage_run_id`.
3. On failure, retry path resets Implement→Validate once per policy.
4. After retry limit, UX reports exhaustion and halts pipeline.

**Success Path**: Validate executes once per attempt; retries are bounded and visible.

**Error Paths**:
- If run-id guard malfunctions, duplicate dispatch test fails (integration coverage).
- If cancellation occurs mid-run, system emits final cancel event and cleans tasks.

---

## Dependencies

**Technical**:
- `codex-rs/tui/src/chatwidget/spec_kit/handler.rs` for state machine updates.
- `state.rs` for storing new run-id fields and retry counters.
- `routing.rs` and `commands/plan.rs` for prompt formatting adjustments.

**Organizational**:
- Review by Spec-Kit maintainers (theturtlecsz, Code) before release.
- Coordination with ARCH-002/AR-2 owners to confirm retry constants.

**Data**:
- Existing evidence logs from Oct 2025 to benchmark improvements.

---

## Risks & Mitigations

| Risk | Impact | Probability | Mitigation | Owner |
|------|--------|-------------|------------|-------|
| R1: Guard rejects legitimate re-run after crash | High | Medium | Reset run-id on crash detection; add health check | Spec-Kit maintainer |
| R2: Telemetry writing fails, losing lifecycle evidence | Medium | Low | Write-through to file system with retry + warning | Spec-Kit maintainer |
| R3: Idempotency guard leaks across other stages | Medium | Low | Scope guard to `SpecStage::Validate`, add tests on other stages | Spec-Kit maintainer |

---

## Success Metrics

**Launch Criteria**:
- Duplicate validate dispatch detected in <1% of stress runs.
- New telemetry fields visible in TUI and evidence JSON.
- Integration + e2e test suite passes.

**Post-Launch Metrics** (30 days):
- Average agent cost per validate run reduced ≥40%.
- Spec auto incident count related to validate duplication = 0.
- Maintainer satisfaction survey on validate stability ≥4/5.

---

## Validation Plan

### Testing Strategy

1. **Unit Tests**: Cover new run-id guard logic in `handler.rs` with deterministic scenarios.
2. **Integration Tests**: Extend `spec_auto_e2e.rs` to simulate rapid callback storms and manual + auto concurrency.
3. **E2E Tests**: Record `/speckit.auto` flows ensuring Implement→Validate retries remain bounded.
4. **Performance Tests**: Add benchmark comparing dispatch latency pre/post fix.

### Review Process

1. **PRD Review**: Spec-Kit architecture working group.
2. **Design Review**: Focus on state machine updates and telemetry schema.
3. **Code Review**: Standard PR review with maintainers.
4. **Security Review**: Not required (no new external surfaces).

---

## Multi-Agent Consensus

### PRD Quality Assessment

**Completeness**: Captures orchestration, telemetry, and retry requirements.

**Clarity**: Single-flight guard expectations defined with measurable metrics.

**Testability**: Each requirement maps to integration or unit coverage.

### Conflicts Resolved

**Issue**: Agents disagreed whether to block at command layer or orchestrator.

**Resolution**: Choose orchestrator-level guard with UX notice; allows `/speckit.plan` to refine if needed.

---

## Evidence & Telemetry

**PRD Creation Evidence**: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-069/prd-consensus.json`

**Agent Outputs**: `.code/agents/*SPEC-KIT-069*/` for raw proposals.

**Validation**: Run `/speckit.analyze SPEC-KIT-069` after implementation to ensure spec ↔ PRD alignment.

---

## Open Questions

1. **Should manual `/speckit.validate` enforce cool-down between runs?**
   **Impact**: Medium (prevents accidental replays)
   **Blocker**: No (can be addressed in plan stage)

2. **Do we surface dedupe counters in telemetry API for downstream dashboards?**
   **Impact**: Low
   **Resolution Path**: Coordinate with telemetry owners during implementation.

**Use `/speckit.clarify SPEC-KIT-069` to resolve systematically.**

---

## Changelog

### 2025-10-23 - Initial PRD
- Drafted by multi-agent consensus.
- Defined single-flight guard, telemetry, and retry alignment scope.

