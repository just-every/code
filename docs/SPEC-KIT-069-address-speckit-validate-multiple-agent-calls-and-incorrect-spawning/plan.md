# Plan: Stabilize /speckit.validate agent orchestration

**SPEC-ID**: SPEC-KIT-069
**Plan Version**: v0.1
**Created**: 2025-10-23

---

## Inputs

**Spec**: docs/SPEC-KIT-069-address-speckit-validate-multiple-agent-calls-and-incorrect-spawning/spec.md (hash: d71b1e4d2281b5d2366cc497feb1b8192aaac4ecbef829be019a8db8235d40d5)
**Constitution**: memory/constitution.md (v1.1)
**PRD**: docs/SPEC-KIT-069-address-speckit-validate-multiple-agent-calls-and-incorrect-spawning/PRD.md
**Prompt Version**: 20251002-plan-a

---

## Work Breakdown

### Step 1: Introduce validate run lifecycle state

**Description**: Extend `SpecAutoState` with validate-specific lifecycle fields (`validate_stage_run_id`, `validate_stage_status`, `validate_attempt`, optional `validate_dedupe_hash`, cancellation token). Provide helpers to begin, finish, and reset runs while keeping scope limited to `SpecStage::Validate`.

**Dependencies**: Existing retry counters in `state.rs:76`, AR-2 policy constants in `handler.rs:23` & `handler.rs:409`.

**Success Signal**: Unit tests cover helper transitions (queued → dispatched → checking_consensus → complete/cancelled) and guard against illegal state changes.

**Owner**: Code

**Estimated Effort**: 0.5 day

### Step 2: Enforce single-flight dispatch & completion

**Description**: Update `auto_submit_spec_stage_prompt()` (`handler.rs:518`) to compute a deterministic dedupe hash (spec + prompt + attempt), perform compare-and-set on the lifecycle helpers, and short-circuit duplicate triggers with UX notice. Ensure `on_spec_auto_agents_complete()` and `check_consensus_and_advance_spec_auto()` transition lifecycle exactly once and ignore late callbacks when status is terminal.

**Dependencies**: Step 1 helpers, existing quality gate single-flight pattern (`quality_gate_handler.rs:18`).

**Success Signal**: Storm integration test confirms exactly one dispatch per attempt and “Validate run already active (run-id …)” appears when re-triggered mid-run.

**Owner**: Code

**Estimated Effort**: 1 day

### Step 3: Align retries & cancellation hygiene

**Description**: Integrate lifecycle resets with AR-2 retry loop (`handler.rs:360-423`) so each implement→validate cycle acquires a fresh run-id, clears dedupe hash, and increments attempt count. Add guard to halt pipeline cleanly after max retries. Introduce cancellation cleanup that clears lifecycle state, placeholder tasks, and issues a single cancel broadcast.

**Dependencies**: Steps 1–2, existing retry counters and cancel flows in `handler.rs:631`.

**Success Signal**: Integration test simulating validation failure shows exactly one validate run per retry attempt, halts after configured cap, and no orphaned placeholder tasks remain after cancel.

**Owner**: Code

**Estimated Effort**: 0.5 day

### Step 4: Telemetry & evidence lifecycle tagging

**Description**: Propagate run-id, attempt, dedupe counters, and roster into telemetry/evidence. Add `remember_validate_lifecycle(...)` utility (patterned after `consensus.rs:966`) to persist lifecycle transitions via local-memory with tags `spec:SPEC-KIT-069`, `stage:validate`, `artifact:agent_lifecycle`. Extend validate telemetry JSON written through `evidence.rs` to include new metadata.

**Dependencies**: Steps 1–3, MCP local-memory availability, existing evidence helpers in `evidence.rs:1-120`.

**Success Signal**: `local-memory search "spec:SPEC-KIT-069 stage:validate artifact:agent_lifecycle"` returns lifecycle entries with matching run-id; evidence telemetry file contains appended fields.

**Owner**: Code

**Estimated Effort**: 0.5 day

### Step 5: Verification suite & performance guardrails

**Description**: Add unit coverage for lifecycle helpers and dispatcher guard; extend `spec_auto_e2e.rs` with callback-storm, manual+auto concurrency, and retry-cycle scenarios; add micro-benchmark validating guard overhead ≤15 ms and duplicate dispatch rate <0.1% across 500 shuffled events. Document manual verification steps (TUI run, evidence inspection).

**Dependencies**: Steps 1–4, existing integration harness (`tui/tests/spec_auto_e2e.rs`).

**Success Signal**: New tests pass; benchmark meets NFR1/NFR2 targets; manual validation checklist completed and recorded in evidence.

**Owner**: Code

**Estimated Effort**: 1 day

---

## Technical Design

### Data Model Changes

**Entity**: `SpecAutoState` (`state.rs:76`)
**Changes**:
- Add lifecycle fields: `validate_stage_run_id: Option<String>`, `validate_stage_status: ValidateLifecycleStatus`, `validate_attempt: u32`, `validate_dedupe_hash: Option<String>`, `validate_cancel_token: Option<String>`.
- Introduce `ValidateLifecycleStatus` enum to mirror quality gate processing states.

**Migration**: No persistent storage; state initializes/reset on /speckit command boundaries.

### API Contracts

No external API changes. Internal MCP telemetry payloads extended with optional fields (`stage_run_id`, `validate_attempt`, `dedupe_count`, `lifecycle_status`). Consumers treat absent fields as backwards-compatible.

### Component Architecture

**New Components**:
- Lifecycle helper module/functions encapsulating begin/reset semantics (within `state.rs`).
- `remember_validate_lifecycle` async helper in `consensus.rs` for MCP logging.

**Modified Components**:
- `handler.rs`: dispatch guard, retry alignment, cancellation cleanup, consensus transition gating.
- `evidence.rs`: telemetry struct augmentation for validate stage outputs.
- `spec_auto_e2e.rs`: new test scenarios covering storms, retries, and manual+auto coexistence.

**Interactions**:
```
auto_submit_spec_stage_prompt -> state.begin_validate_run -> dispatch agents
AgentStatusUpdateEvent -> on_spec_auto_agents_complete (guarded by status)
check_consensus_and_advance_spec_auto -> state.finish_validate_run -> remember_validate_lifecycle -> evidence telemetry
retry/cancel paths -> state.reset_validate_run & remember_validate_lifecycle(cancelled)
```

---

## Acceptance Mapping

| Requirement (from Spec) | Validation Step | Test/Check Artifact |
|-------------------------|-----------------|---------------------|
| FR1: Single-flight scheduling (spec.md:64) | Storm integration test + manual re-trigger check | `cargo test spec_auto_e2e::validate_single_flight`; TUI log shows dedupe notice |
| FR2: Canonical telemetry (spec.md:70) | Local-memory search & evidence audit | `local-memory search "spec:SPEC-KIT-069 stage:validate artifact:agent_lifecycle"`; `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-069/validate_<ts>_telemetry.json` |
| FR3: Retry alignment (spec.md:75) | Induced failure test covering Implement→Validate loops | `cargo test spec_auto_e2e::validate_retry_cycle`; history log indicates capped retries |
| FR4: Cancellation cleanup (spec.md:82) | Manual cancel during active run | `/speckit.validate SPEC-KIT-069` + `Ctrl+C`; confirm no placeholder tasks and lifecycle status=cancelled |
| NFR1/NFR2: Performance & reliability (spec.md:90) | Guard benchmark + duplicate rate sampling | `cargo bench spec_kit::validate_guard`; benchmark report stored in `docs/SPEC-KIT-069-.../evidence/perf.txt` |

---

## Risks & Unknowns

### Risk 1: Lifecycle state not cleared after unexpected panic

**Impact**: Medium — Future runs may be blocked by stale run-id.

**Probability**: Low — TUI resets state on new /speckit command but panic mid-session possible.

**Mitigation**: Reset lifecycle inside global error handlers (`halt_spec_auto_with_error`), add inactivity timeout in Step 3, document manual `/spec-auto reset` fallback.

### Risk 2: Telemetry schema change breaks downstream tooling

**Impact**: Medium — Guardrail scripts parsing JSON could fail.

**Probability**: Low — Fields added as optional.

**Mitigation**: Append new fields without removing existing keys; update schema documentation; dry-run `/spec-consensus` to ensure compatibility.

### Risk 3: Manual + auto concurrency edge cases still produce overlap

**Impact**: High — Core objective would regress.

**Probability**: Medium — Manual triggers during auto cycle are common.

**Mitigation**: Add explicit manual trigger guard using lifecycle helpers; integration test covers concurrent scenario; UX message guides operator.

---

## Multi-Agent Consensus

### Agreements

**All agents aligned on:**
- Implementing a validate-specific single-flight guard backed by lifecycle state and compare-and-set logic (`handler.rs:518`, `state.rs:76`).
- Recording run lifecycle to local-memory/evidence with tags (`spec:SPEC-KIT-069`, `stage:validate`, `artifact:agent_lifecycle`) at importance ≥8.
- Keeping AR-2 retry caps (agent retries=3, validate retries=2) while ensuring each attempt spawns exactly one validate run.

### Conflicts Resolved

**Issue**: Scope of lifecycle helpers (validate-only vs stage-agnostic abstraction).

**Positions**:
- Gemini: Favors generic `StageRunGuard` to reuse for future stages.
- Claude: Prefers validate-only scope now to limit blast radius.
- GPT-Pro: Adopt validate-only implementation but design helpers to be extensible later.

**Resolution**: Proceed with validate-focused helpers (minimal change set) while structuring code so enum/field naming can extend to other stages without churn. No arbiter escalation required.

---

## Exit Criteria

- [ ] All work breakdown steps completed and merged.
- [ ] New unit/integration/benchmark tests pass in CI.
- [ ] Telemetry and evidence artifacts include run-id metadata.
- [ ] Manual validation checklist executed (storm, retry, cancel scenarios).
- [ ] Risks reviewed; mitigations implemented or accepted.
- [ ] Plan consensus artifacts stored via local-memory.
- [ ] Ready to proceed to `/speckit.tasks` stage.

---

## Evidence References

**Plan Consensus**: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-069/spec-plan_synthesis.json`

**Telemetry**: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-069/spec-plan_<timestamp>.json`

**Agent Outputs**: `.code/agents/fdef3871-aba1-4c95-80db-118d59c4eb6c/result.txt`, `.code/agents/72865a03-188f-4cef-997b-7e1aa99700ef/result.txt`, `.code/agents/d0c57ee4-ffc0-4131-9c86-cc7f6297659c/result.txt`

