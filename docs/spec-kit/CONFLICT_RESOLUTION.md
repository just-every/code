# Conflict Resolution & Arbiter System

**Status**: v1.0 (2025-10-18)
**Task**: MAINT-9 (Document arbiter trigger conditions)
**SPEC Reference**: SPEC.md line 24 ("Automatic arbiter, <5% deadlocks")

---

## 1. Executive Summary

**SPEC Claim**: "Automatic arbiter, <5% deadlocks"
**Current Reality**: **Arbiter not yet implemented** - Conflicts detected and reported, but no automatic arbiter spawning

**Actual Conflict Handling** (as of 2025-10-18):
1. Agents execute in parallel (gemini, claude, gpt_pro, optionally code/gpt_codex)
2. gpt_pro serves as **aggregator** (synthesizes outputs, identifies agreements/conflicts)
3. Consensus verdict includes `conflicts[]` array from aggregator
4. Pipeline continues with `status: "conflict"` or `status: "degraded"`
5. **No automatic arbiter agent spawned** (design exists in SPEC_AUTO_FLOW.md, not implemented)

**Quality Gate System** (T85) serves similar role:
- GPT-5 validation for 2/3 majority answers (not full arbiter)
- Auto-resolution for unanimous (3/3) agreement
- Human escalation for low confidence

---

## 2. Current Consensus Flow

### 2.1 Agent Roles

| Agent | Role | Conflicts Detection |
|-------|------|---------------------|
| **gemini** | Research, exploration | Provides perspective |
| **claude** | Analysis, edge cases | Provides perspective |
| **code** | General-purpose | Provides perspective (optional) |
| **gpt_codex** | Code generation | Provides perspective (implement stage only) |
| **gpt_pro** | **Aggregator/Synthesizer** | **Identifies conflicts** |

**Key**: gpt_pro's `consensus.conflicts[]` array is authoritative (not voted/calculated)

### 2.2 Conflict Detection Logic

**Location**: `tui/src/chatwidget/spec_kit/consensus.rs:609, 654, 659`

```rust
// Extract conflicts from aggregator (gpt_pro)
if artifact.agent == "gpt_pro" || artifact.agent == "gpt-5" {
    if let Some(consensus_node) = artifact.content.get("consensus") {
        conflicts = extract_string_list(consensus_node.get("conflicts"));
    }
}

// Determine if conflict exists
has_conflict = summary.status.eq_ignore_ascii_case("conflict")
    || !conflicts.is_empty();

// Consensus OK only if no conflicts
consensus_ok = !aggregator_summary.is_none()
    && conflicts.is_empty()
    && missing_agents.is_empty()
    && required_fields_ok;
```

**Verdict Status** (consensus.rs:981):
```rust
"status": if verdict.consensus_ok {
    "ok"
} else if !verdict.conflicts.is_empty() {
    "conflict"  // ← But no arbiter spawned
} else {
    "degraded"
}
```

### 2.3 What Happens When Conflicts Detected

**Current Behavior** (verified in code):
1. Consensus verdict: `status: "conflict"`, `conflicts: ["description1", "description2"]`
2. Verdict displayed in TUI: `"  Conflicts: <conflict descriptions>"`
3. Pipeline **continues** (no halt, no arbiter)
4. Stage considered complete (consensus exists, even if conflicted)

**Expected Behavior** (per SPEC_AUTO_FLOW.md):
1. Conflicts detected
2. Spawn arbiter agent (gpt-5 with high reasoning mode)
3. Arbiter analyzes all outputs + conflicts
4. Arbiter chooses best approach
5. Pipeline continues with arbiter decision as consensus

**Gap**: Steps 2-5 not implemented

---

## 3. Arbiter Design (SPEC_AUTO_FLOW.md)

From `SPEC_AUTO_FLOW.md` lines 89-100:

```
alt Conflicts Detected
    Note over Orchestrator: Automatic conflict resolution

    Orchestrator->>Arbiter: agent_run name=arbiter-plan
                            All outputs + conflicts
    Arbiter->>Arbiter: Analyze disagreements
    Arbiter->>Arbiter: Choose best approach
    Arbiter->>FS: Write arbiter result
    Arbiter-->>Orchestrator: Decision + rationale

    Orchestrator->>Orchestrator: Apply arbiter decision
    Orchestrator->>FS: Write synthesis.json
                      status=ok (arbiter resolved)
```

**Design Intent**:
- Arbiter spawns when `conflicts.is_empty() == false`
- Arbiter receives all agent outputs + conflict descriptions
- Arbiter makes binding decision
- Final synthesis: `status: "ok"` (arbiter resolved), `arbiter_decision: {...}`

---

## 4. Quality Gate System (Partial Arbiter)

**What Actually Works** (T85 implementation):

Quality gates provide **limited arbiter functionality** for requirement quality:

| Scenario | Handling |
|----------|----------|
| **3/3 agents agree** | Auto-resolve (High confidence) |
| **2/3 agents agree** | GPT-5 validation (acts as arbiter for that specific issue) |
| **0-1/3 agents agree** | Human escalation (modal UI) |

**Location**: `tui/src/chatwidget/spec_kit/quality_gate_handler.rs:on_gpt5_validations_complete()`

**GPT-5 as Limited Arbiter**:
```rust
// GPT-5 validates 2/3 majority answers
for validation_item in validation_array {
    let agrees = validation_item["agrees_with_majority"].as_bool();

    if agrees {
        // GPT-5 validates → auto-apply
    } else {
        // GPT-5 rejects → escalate to human
    }
}
```

**Difference from Full Arbiter**:
- Quality gates: Validate individual requirement issues
- Full arbiter: Resolve consensus conflicts between agents' overall plans/approaches
- Quality gates run at 3 checkpoints (PrePlanning, PostPlan, PostTasks)
- Arbiter would run per-stage when consensus conflicts detected

---

## 5. Why Arbiter Not Implemented

**Hypothesis** (from codebase archaeology):

1. **gpt_pro as de facto arbiter**: Aggregator role already synthesizes and resolves most conflicts internally
2. **Quality gates handle edge cases**: GPT-5 validation catches issues gpt_pro missed
3. **Low conflict rate**: If <5% of runs have unresolvable conflicts, arbiter automation not urgent
4. **Human escalation acceptable**: Modal UI for quality gates provides manual resolution path

**Evidence Supporting Low Conflict Rate**:
- Testing policy mentions "automatic arbiter" aspirationally
- No reports of pipeline deadlocks in SPEC.md completed tasks
- Quality gates (T85) added later suggest conflicts were manageable without arbiter

---

## 6. Recommended Implementation (If Needed)

**Trigger Condition** (when to spawn arbiter):
```rust
// In handler.rs::check_consensus_and_advance_spec_auto()

let (consensus_lines, degraded) = consensus_result?;

// Check if conflicts exist
let has_conflicts = consensus_lines.iter().any(|line| {
    line.to_string().contains("Conflicts:")
});

if has_conflicts {
    // Spawn arbiter agent
    let arbiter_prompt = build_arbiter_prompt(spec_id, stage, &consensus_lines);
    widget.submit_prompt_with_display(
        format!("[Arbiter] {} {}", spec_id, stage.display_name()),
        arbiter_prompt
    );

    // Wait for arbiter response
    state.phase = SpecAutoPhase::ArbiterExecuting { ... };
    return; // Don't advance yet
}
```

**Arbiter Prompt Template**:
```
You are resolving conflicts in multi-agent consensus for SPEC {spec_id} stage {stage}.

Agent Outputs:
- Gemini: {gemini_output}
- Claude: {claude_output}
- GPT-5: {gpt_output}

Aggregator (GPT-5) Identified Conflicts:
{conflicts_array}

Your Task:
1. Analyze root cause of disagreement
2. Evaluate each approach against SPEC intent
3. Choose best approach or synthesize hybrid
4. Provide binding decision

Output JSON:
{
  "decision": "approach_a | approach_b | hybrid",
  "rationale": "Why this approach aligns with SPEC",
  "modifications": ["Change X in plan", "Adjust Y in tasks"],
  "confidence": "high | medium | low"
}
```

**Integration Points**:
1. `handler.rs::check_consensus_and_advance()` - Detect conflicts, spawn arbiter
2. `handler.rs::on_arbiter_complete()` - Apply arbiter decision, advance stage
3. `consensus.rs::run_spec_consensus()` - Include arbiter metadata in verdict
4. New phase: `SpecAutoPhase::ArbiterExecuting { stage, awaiting_arbiter }`

---

## 7. Current Workarounds

**Without Arbiter** (how conflicts are handled today):

1. **Trust gpt_pro aggregator**: Accept synthesized plan even if conflicts noted
2. **Quality gates catch issues**: PostPlan/PostTasks checkpoints validate output
3. **Retry on failure**: If implementation fails tests, retry cycle (max 2x)
4. **Manual intervention**: Human reviews synthesis files if deadlock suspected

**Measured Impact**:
- No reported deadlocks in completed tasks (T60-T90, AR-1-AR-4, DOC-1-DOC-9, MAINT-1-MAINT-5)
- All 26 completed tasks progressed to Done
- Suggests conflict rate is low or gpt_pro resolves internally

---

## 8. Deadlock Definition

**True Deadlock** (per SPEC "<5% deadlocks"):
- Consensus conflicts detected
- No arbiter or arbiter also conflicts
- No clear path forward
- Pipeline halts, human intervention required

**Current Deadlock Rate**: **0%** (as of 2025-10-18)
- Observation: 26 tasks completed, zero deadlocks reported
- Caveat: Only 3 SPECs in evidence (DEMO, 025, 045-mini)
- Insufficient data to validate "<5%" claim

---

## 9. Implementation Priority

**Status**: **Deferred** (not blocking production usage)

**Triggers for Implementation**:
1. Conflict rate >5% (currently 0% measured)
2. gpt_pro synthesis quality degrades
3. Manual conflict resolution becomes frequent
4. Quality gates insufficient for catching consensus issues

**Effort**: 1-2 days (prompt design, phase addition, integration)

**Alternative**: Let gpt_pro continue as de facto arbiter (current approach working)

---

## 10. Related Documentation

- `SPEC.md` line 24: "Automatic arbiter" claim (aspirational)
- `SPEC_AUTO_FLOW.md` lines 89-100: Arbiter design
- `tui/src/chatwidget/spec_kit/consensus.rs`: Conflict detection logic
- `tui/src/chatwidget/spec_kit/quality_gate_handler.rs`: GPT-5 validation (partial arbiter for quality issues)
- `docs/spec-kit/testing-policy.md`: Quality gate testing strategy

---

## 11. Change History

| Version | Date | Changes | Author |
|---------|------|---------|--------|
| v1.0 | 2025-10-18 | Initial documentation (MAINT-9) | theturtlecsz |

---

## Appendix: Honest Assessment

**SPEC vs Reality**:
- SPEC claims: "Automatic arbiter"
- Reality: gpt_pro aggregator + quality gate GPT-5 validation
- Functional gap: No dedicated arbiter agent spawning on conflicts
- Practical gap: Zero deadlocks observed, current approach sufficient

**Recommendation**: Update SPEC.md line 24 to reflect reality:
- Current: "✅ **Conflict resolution**: Automatic arbiter, <5% deadlocks"
- Accurate: "✅ **Conflict resolution**: gpt_pro aggregator + quality gate validation, 0% deadlocks observed"

Or implement arbiter per SPEC_AUTO_FLOW.md if conflict rate increases.
