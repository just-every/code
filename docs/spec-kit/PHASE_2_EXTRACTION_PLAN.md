# Phase 2 Spec-Kit Extraction Plan

**Date:** 2025-10-15
**Phase 1 Status:** ✅ Complete (merged to main)
**Current State:** chatwidget/mod.rs at 22,279 lines
**Phase 2 Goal:** Extract consensus & guardrail infrastructure
**Target:** Reduce to ~20,400 lines (-1,853 lines)

---

## Remaining Spec-Kit Code Analysis

### Consensus Infrastructure (~1,093 lines)

**Types (113 lines, lines 13901-14013):**
```rust
struct ConsensusArtifactData        // Memory artifact metadata
struct ConsensusEvidenceHandle      // File path handle
struct ConsensusTelemetryPaths      // Evidence file paths
struct ConsensusArtifactVerdict     // Per-artifact verdict
struct ConsensusVerdict             // Overall consensus result
struct ConsensusSynthesisSummary    // Loaded synthesis data
struct ConsensusSynthesisRaw        // JSON schema
struct ConsensusSynthesisConsensusRaw  // Nested consensus data
```

**Helper Functions (~284 lines):**
- `parse_consensus_stage` (14014) - 12 lines - Parse stage name
- `collect_consensus_artifacts` (14026) - ~200 lines - Query local-memory
- `expected_agents_for_stage` (14226) - 13 lines - Agent roster
- `extract_string_list` (14239) - 19 lines - JSON parsing
- `validate_required_fields` (14258) - ~40 lines - Schema validation

**Implementation Methods (~696 lines):**
- `handle_consensus_impl` (15102) - ~49 lines - Command entry point
- `load_latest_consensus_synthesis` (15151) - ~92 lines - File loading
- `run_spec_consensus` (15243) - ~293 lines - Core consensus logic
- `persist_consensus_verdict` (15536) - ~39 lines - Write verdict
- `persist_consensus_telemetry_bundle` (15575) - ~193 lines - Write telemetry
- `remember_consensus_verdict` (15768) - ~80 lines - Store to local-memory
- `queue_consensus_runner` (16625) - ~50 lines - Background execution

**Total:** ~1,093 lines

---

### Guardrail Infrastructure (~760 lines)

**Helper Functions (~337 lines):**
- `validate_guardrail_schema` (16678) - ~157 lines - Schema validation
- `evaluate_guardrail_value` (16835) - ~180 lines - Status evaluation

**Implementation Methods (~423 lines):**
- `handle_guardrail_impl` (14871) - ~223 lines - Command parsing & env setup
- `collect_guardrail_outcome` (17016) - ~100 lines - Read telemetry
- `read_latest_spec_ops_telemetry` - ~100 lines - File discovery

**Total:** ~760 lines

---

## Phase 2 Extraction Strategy

### Batch 1: Consensus Types & Module Setup (2-3 hours)

**Create:** `tui/src/chatwidget/spec_kit/consensus.rs`

**Extract:**
1. All 8 consensus structs (113 lines)
2. Helper functions that don't call ChatWidget methods:
   - parse_consensus_stage (12 lines)
   - expected_agents_for_stage (13 lines)
   - extract_string_list (19 lines)
   - validate_required_fields (40 lines)

**Steps:**
```bash
# 1. Create consensus.rs
touch tui/src/chatwidget/spec_kit/consensus.rs

# 2. Add module declaration
# In spec_kit/mod.rs:
pub mod consensus;

# 3. Copy types and simple helpers
sed -n '13901,14013p' tui/src/chatwidget/mod.rs >> spec_kit/consensus.rs
# Add parse_consensus_stage, expected_agents_for_stage, etc.

# 4. Update imports in chatwidget/mod.rs
# Replace struct definitions with: use spec_kit::consensus::*;

# 5. Test
cargo build -p codex-tui --profile dev-fast
```

**Risk:** LOW - No ChatWidget method dependencies

---

### Batch 2: Consensus Core Logic (4-6 hours)

**Extract to:** `spec_kit/consensus.rs`

**Functions:**
- `collect_consensus_artifacts` (~200 lines) - Depends on local-memory CLI
- `run_spec_consensus` (~293 lines) - Core logic, calls helpers
- `load_latest_consensus_synthesis` (~92 lines) - File I/O

**Convert to free functions:**
```rust
pub fn run_spec_consensus(
    widget: &ChatWidget,  // Read-only access needed
    spec_id: &str,
    stage: SpecStage,
) -> Result<(Vec<Line<'static>>, bool), String> {
    // Move implementation here
}
```

**Challenges:**
- `run_spec_consensus` needs `widget.config.cwd` (read-only)
- `collect_consensus_artifacts` calls local-memory CLI
- File I/O operations need proper error handling

**Steps:**
1. Extract as free functions taking `&ChatWidget` (not `&mut`)
2. Update handle_consensus_impl to call free functions
3. Test compilation
4. Verify tests pass

**Risk:** MEDIUM - Complex logic, but mostly self-contained

---

### Batch 3: Consensus Persistence (3-4 hours)

**Extract to:** `spec_kit/consensus.rs`

**Functions:**
- `persist_consensus_verdict` (~39 lines) - Write verdict to file
- `persist_consensus_telemetry_bundle` (~193 lines) - Write telemetry JSON
- `remember_consensus_verdict` (~80 lines) - Store to local-memory
- `queue_consensus_runner` (~50 lines) - Background task

**Pattern:**
```rust
pub fn persist_consensus_verdict(
    cwd: &Path,
    spec_id: &str,
    stage: SpecStage,
    verdict: &ConsensusVerdict,
) -> Result<(), String> {
    // Move implementation
}
```

**Challenges:**
- File path construction
- JSON serialization
- Error handling consistency

**Risk:** LOW - Mostly I/O operations

---

### Batch 4: Guardrail Types & Helpers (3-4 hours)

**Create:** `spec_kit/guardrail.rs`

**Extract:**
- `validate_guardrail_schema` (~157 lines) - Pure function
- `evaluate_guardrail_value` (~180 lines) - Pure function
- `collect_guardrail_outcome` (~100 lines) - Calls read_latest_spec_ops_telemetry
- `read_latest_spec_ops_telemetry` (~100 lines) - File discovery

**Pattern:**
```rust
pub fn validate_guardrail_schema(
    stage: SpecStage,
    telemetry: &Value,
) -> Vec<String> {
    // Pure function, no widget access needed
}
```

**Risk:** LOW - Mostly pure functions

---

### Batch 5: Guardrail Implementation (4-5 hours)

**Extract:** `handle_guardrail_impl` (~223 lines)

**Challenges:**
- Argument parsing logic
- Environment variable setup
- Calls `queue_project_command` (ChatWidget method)
- HAL mode handling

**Strategy:**
```rust
pub fn handle_guardrail(
    widget: &mut ChatWidget,
    command: SlashCommand,
    raw_args: String,
    hal_override: Option<HalMode>,
) {
    // Full implementation moved from handle_guardrail_impl
    // Call widget.queue_project_command when needed
}
```

**Risk:** MEDIUM - Complex environmental setup

---

## Execution Plan

### Week 1: Consensus Extraction
**Day 1:** Batch 1 - Types & helpers (3 hours)
**Day 2:** Batch 2 - Core logic (6 hours)
**Day 3:** Batch 3 - Persistence (4 hours)

### Week 2: Guardrail Extraction
**Day 4:** Batch 4 - Helpers (4 hours)
**Day 5:** Batch 5 - Implementation (5 hours)

**Total:** ~22 hours over 5 days

---

## Module Structure After Phase 2

```
tui/src/chatwidget/spec_kit/
├── mod.rs (exports)
├── handler.rs (582 lines) - Command handlers
├── state.rs (244 lines) - State types
├── consensus.rs (~1,093 lines) - NEW
└── guardrail.rs (~760 lines) - NEW
```

**Total spec_kit:** ~2,679 lines
**chatwidget/mod.rs:** ~20,426 lines
**Isolation:** 99.1% of spec-kit code separated

---

## Risk Assessment

### Low Risk Extractions
- Consensus types (113 lines)
- Pure helper functions (237 lines)
- File I/O operations (312 lines)

**Total low risk:** 662 lines

### Medium Risk Extractions
- Consensus core logic (293 lines)
- Guardrail implementation (223 lines)
- Method interdependencies (400 lines)

**Total medium risk:** 916 lines

### Mitigation Strategy

1. **Incremental commits:** Commit after each batch
2. **Test-driven:** Run tests after each extraction
3. **Rollback ready:** Keep batches small for easy revert
4. **Documentation:** Update as we go

---

## Decision Point

**Proceed with Phase 2?**

**PRO:**
- Further reduce rebase conflict surface
- Complete isolation of spec-kit code
- Cleaner architecture
- Easier to maintain/test

**CON:**
- Significant time investment (22 hours)
- Medium complexity risk
- Phase 1 already achieved 96.3% isolation
- Diminishing returns

**Recommendation:** Proceed with Batch 1 (low risk, 3 hours) and evaluate.

---

**Document Version:** 1.0
**Status:** Ready for Phase 2 execution
**Author:** Refactoring continuation from Phase 1
