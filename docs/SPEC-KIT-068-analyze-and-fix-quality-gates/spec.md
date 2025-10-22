**SPEC-ID**: SPEC-KIT-068
**Feature**: Restore Spec-Kit quality gates
**Status**: Backlog
**Created**: 2025-10-22
**Branch**: (pending)
**Owner**: Code

**Context**: Quality gates (clarify, checklist, analyze) were disabled in `/speckit.auto` after async panics caused by nested `tokio::Handle::block_on` calls. Guardrail orchestration has been stabilized, so restoring the gate workflow is the next critical automation gap. See [PRD](./PRD.md) for full requirements and architecture notes.

---

## User Scenarios

### P1: Automated spec validation before planning proceeds

**Story**: As a maintainer running `/speckit.auto`, I want the clarify and checklist checkpoints to execute automatically so that ambiguous requirements are surfaced before tasks are generated.

**Priority Rationale**: Restores the baseline assurance that existed before gates were disabled; prevents wasted effort downstream.

**Testability**: Run `/speckit.auto SPEC-KIT-068 --from plan` with deliberate PRD gaps and verify the gates produce issues, retries, and evidence.

### P2: Analyze checkpoint after plan completion

**Story**: As a maintainer transitioning from plan to tasks, I want the analyze checkpoint to validate consistency so that conflicting guidance is resolved before implementation.

**Priority Rationale**: Analyze stage catches plan-level contradictions; without it, later stages inherit bad assumptions.

**Testability**: Inject conflicting plan guidance, run the pipeline, confirm analyze gate escalates or resolves issues per acceptance criteria.

### P3: Graceful degraded mode and evidence capture

**Story**: As a developer observing automation, I want degraded consensus and retries to be visible in the TUI and evidence so that I can trust the pipeline when agents are flaky.

**Priority Rationale**: Async failures are inevitable; degraded but transparent behavior maintains confidence.

**Testability**: Simulate an agent timeout, confirm retries then degraded consensus with warnings, telemetry, and pipeline continuation.

---

## Requirements Snapshot

- Implement the quality gate broker and channel architecture described in the PRD.
- Re-enable checkpoint scheduling through `determine_quality_checkpoint()`.
- Ensure all quality gate artifacts follow the local-memory tagging and evidence policies.
- Provide TUI notices for progress, retries, degraded consensus, conflicts, and completion.
- Deliver regression, integration, and soak tests that cover the restored workflow.

---

## References

- [PRD](./PRD.md)
- `codex-rs/tui/src/chatwidget/spec_kit/handler.rs`
- `codex-rs/tui/src/chatwidget/spec_kit/quality_gate_handler.rs`
- `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/`

