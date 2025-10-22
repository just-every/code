# PRD: Restore Spec-Kit Quality Gates

**SPEC-ID**: SPEC-KIT-068
**Status**: Draft
**Created**: 2025-10-22
**Author**: Multi-agent consensus (gemini, claude, code)

---

## 1. Problem Statement

The `/speckit.auto` pipeline currently skips the Clarify, Checklist, and Analyze quality checkpoints because the orchestration was disabled after repeated runtime panics. Those panics were caused by synchronous `tokio::Handle::block_on` calls that attempted to run asynchronous MCP requests from the TUI event loop. With the checkpoints disabled, specifications advance without the intended multi-agent review, lowering spec quality, forcing manual review, and breaking telemetry continuity.

**Current Symptoms**
- `determine_quality_checkpoint()` in `quality_gate_handler.rs` returns `None`, short-circuiting all checkpoints.
- Quality-gate artifacts are absent from local-memory and evidence folders, leaving the consensus runner without inputs.
- Developers rely on manual review, increasing cycle time and risk of missed requirements.

**Why Now**
- Guardrail hangs have been addressed, so quality gates are the blocking gap before Tier-2 automation can be trusted end-to-end.
- Recent async refactors (e.g., `block_on_sync` helpers) provide a model for fixing the remaining quality-gate paths.

---

## 2. Goals & Non-Functional Requirements

### Goals
1. Re-enable the Clarify, Checklist, and Analyze checkpoints for `/speckit.auto`, `/speckit.plan`, and `/speckit.tasks` flows.
2. Execute multi-agent reviews (Gemini, Claude, Code; GPT Pro as aggregator) without blocking the TUI runtime.
3. Persist all agent artifacts to local-memory with compliant tags and to evidence folders with file locking.
4. Provide clear UX updates (progress, retries, degraded consensus) and continue the automation pipeline seamlessly after gates succeed.

### Non-Functional Requirements
- **Stability:** Zero runtime panics across a one-hour soak of repeated `/speckit.auto` runs.
- **Responsiveness:** The TUI event loop remains responsive (<32 ms stall) while gates execute asynchronously.
- **Latency:** Each checkpoint completes within 60 s typical (allows for multi-agent transit). MCP search/store paths retain the 8.7 ms average benchmark when warmed.
- **Evidence Hygiene:** Quality-gate telemetry respects the 25 MB per SPEC soft cap and existing file-locking guarantees.

---

## 3. Requirements

### Functional Requirements
- **R1:** `determine_quality_checkpoint()` returns the next pending checkpoint (PrePlanning → Clarify+Checklist, PostPlan → Analyze, PostTasks → Analyze) based on stage progression.
- **R2:** For each checkpoint, spawn all configured agents (Gemini, Claude, Code) and collect results via local-memory MCP with tags `spec:SPEC-KIT-068`, `stage:<clarify|checklist|analyze>`, `checkpoint:<name>`, `agent:<name>` and `importance >= 8`.
- **R3:** Consensus aggregation uses GPT Pro to produce `agreements[]`/`conflicts[]` summaries; degraded consensus (2/3 agents) is accepted with warnings, conflicts trigger retries or escalation.
- **R4:** Implement retry policy (up to 3 attempts, 100 ms → 200 ms → 400 ms backoff) for missing/invalid agent results or MCP errors.
- **R5:** Telemetry and evidence files for each checkpoint are stored under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/quality-gate/` and `…/commands/`, reusing existing schema v1 fields plus `checkpoint` metadata.
- **R6:** When checkpoints succeed, `/speckit.auto` advances automatically to the next stage; failures halt the pipeline with specific remediation instructions.

### Non-Functional Requirements (supporting)
- No blocking `tokio::Handle::block_on` calls from TUI-facing code; all async work runs inside dedicated broker tasks.
- Guardrail stages remain first; quality gates run immediately afterward without reintroducing hangs.
- Evidence writes continue to use `fs2` locks; local-memory remains the single source for shared artifacts per the 2025-10-18 policy.

---

## 4. Acceptance Criteria

1. **Async Safety:** Running `/speckit.auto SPEC-KIT-068` completes without panic, and stack traces show no nested `block_on` from TUI handlers.
2. **Agent Coverage:** For each checkpoint, local-memory contains entries for Gemini, Claude, and Code with correct tags and JSON content; GPT Pro consensus files exist.
3. **Retry & Degraded Paths:** Simulated agent failure triggers retries; after 3 attempts the checkpoint continues in degraded (2/3) mode with UX notice and telemetry entry.
4. **Evidence Artifacts:** Quality-gate telemetry JSON is written to the evidence tree with schema fields `command`, `specId`, `checkpoint`, `summary`, `auto_resolved`, `escalated`, `timestamp`.
5. **UX Feedback:** The TUI renders progress updates, retry banners, degraded warnings, and completion notices for each checkpoint, and transitions automatically to guardrails / subsequent stages.
6. **Regression Tests:** New integration/E2E suites covering gates pass in CI, including a regression that previously triggered the tokio runtime panic.

---

## 5. Architecture & Design Overview

### Async Orchestration
- Introduce a single **Quality Gate Broker** async task that owns checkpoint scheduling, agent fan-out, retries, and consensus aggregation.
- Communication:
  - TUI → Broker: bounded `mpsc` channel carrying `RunCheckpoint` commands.
  - Broker → TUI: status channel emitting `Progress`, `Retrying`, `Completed`, `Failed`, `Escalate`, etc.
- Concurrency controls:
  - `Semaphore(1)` per SPEC to serialize checkpoints (Clarify → Checklist → Analyze).
  - `JoinSet` or `FuturesUnordered` to launch agent calls in parallel within the broker.
- Cancellation:
  - Each checkpoint uses a `CancellationToken` linked to the active `/speckit.auto` session; cancellation occurs if the user stops automation.

### Agent & MCP Integration
- Reuse prompts from `docs/spec-kit/prompts.json`; ensure prompt versions (e.g., `20251002-clarify-b`) are surfaced in stored artifacts.
- Agents store JSON results via MCP `remember` with required tags and `importance: 8`.
- Broker queries local-memory with stage/tag filters and waits asynchronously for all expected artifacts.

### Guardrail & Pipeline Flow
1. Guardrail scripts in `scripts/spec_ops_004/` execute first.
2. Quality Gate Broker runs the checkpoint and posts results to the TUI.
3. If consensus succeeds (or degraded without conflicts), broker signals completion and the state machine advances to the stage’s consensus and next guardrail.
4. Conflicts after retries escalate to human modal; pipeline pauses with resume instructions.

### Telemetry & Evidence
- Extend `FilesystemEvidence` helpers to write `quality-gate` telemetry bundles.
- Ensure JSON includes summary counts for auto-resolved vs escalated issues and checkpoint metadata.
- Rotate older artifacts if footprint exceeds policy limit.

### UX Updates
- Render progress banners in history view.
- For escalations, display modal summarizing agent disagreements and providing quick actions for human resolution.
- Provide explicit notice when degraded consensus is used.

---

## 6. Risks & Mitigations

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|------------|
| Async deadlock or channel leak | High | Medium | Single broker task, bounded channels, structured phases, async integration tests. |
| Missing agent artifacts | Medium | Medium | Retry with exponential backoff, degrade to 2/3 consensus, emit warnings, store context for debugging. |
| Evidence bloat | Low | Low | Reuse evidence policy, rotate stale artifacts. |
| Developer confusion / overrides | Medium | Medium | Document broker architecture, provide feature flag to disable gates for debugging, emit clear UX instructions. |
| GPT-5 validation latency | Medium | Medium | Run GPT-5 validations asynchronously with 30 s timeout; fall back to human escalation on timeout. |

---

## 7. Test Strategy

- **Unit Tests**
  - `determine_quality_checkpoint()` sequencing.
  - Retry/backoff logic for agent completions.
  - Classification functions (unanimous, majority, no-consensus) with degraded flags.
  - Telemetry builder validating required fields.
- **Integration Tests**
  - Full broker path using mock MCP client to simulate agent outputs, retries, and degraded consensus.
  - Guardrail → quality gate → guardrail chain ensuring pipeline continues.
  - Human escalation path: simulate unresolved conflict and ensure modal invocation.
- **End-to-End Tests**
  - `/speckit.auto` happy path with gates enabled; verify all evidence artifacts and stage transitions.
  - Soak test running multiple SPECs to ensure no async leaks or panics.
- **Regression Tests**
  - Scenario replicating the original `block_on` crash to guarantee absence after the fix.
  - MCP initialization race (`MCP manager not initialized yet`) with retry coverage.

---

## 8. Milestones & Timeline

| Milestone | Target | Deliverables |
|-----------|--------|--------------|
| M1: Async foundation | Day 2 | Broker scaffold, channels, semaphore, no `block_on` in UI paths |
| M2: Agent integration | Day 4 | Agent fan-out, local-memory storage, retry/degraded logic |
| M3: Telemetry & UX | Day 5 | Evidence persistence, UX notices, modal escalation |
| M4: Testing & hardening | Day 7 | Unit/integration/E2E suites, regression coverage, soak run |
| M5: Documentation & rollout | Day 8 | Update CLAUDE.md, SPEC docs, enable gates in `/speckit.auto` default |

---

## 9. Open Questions

1. Do we require GPT Codex participation for quality gates or limit to Gemini/Claude/Code + GPT Pro aggregator?
2. Should degraded consensus automatically trigger a follow-up reminder or is the warning banner sufficient?
3. What default timeout should we adopt for GPT-5 validations (current proposal: 30 s)?

---

## 10. Dependencies

- Recent async helper (`block_on_sync`) already merged for stage prompts; reuse for consensus paths where needed.
- Local-memory MCP service must be running and reachable (existing `/speckit.auto` prerequisite).
- Evidence policy / telemetry schema remain valid for new files.

---

## 11. Approval Checklist

- [ ] PRD reviewed by Spec-Kit maintainers
- [ ] Async architecture signed off by TUI owners
- [ ] Evidence strategy reviewed by documentation owners
- [ ] Testing plan validated by QA stakeholders

