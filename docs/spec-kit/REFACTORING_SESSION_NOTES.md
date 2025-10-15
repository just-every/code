# Refactoring Session Notes - Phase 1 Extraction

**Date:** 2025-10-15
**Branch:** refactor/spec-kit-module-extraction
**Base Commit:** 892d1e4a2 (foundation complete)

---

## Step 1.1: COMPLETE ✅

**Created:**
- `codex-rs/tui/src/spec_kit/mod.rs`
- `codex-rs/tui/src/spec_kit/state.rs` (foundation)
- `codex-rs/tui/src/spec_kit/handler.rs` (skeleton)
- Registered module in lib.rs
- **Commit:** 892d1e4a2

**Compilation:** ✅ Successful

---

## Step 1.2: COMPLETE ✅

**Replaced state.rs with actual definitions:**
- Extracted 245 lines from chatwidget.rs
- 5 structs (SpecAutoPhase, GuardrailWait, SpecAutoState, GuardrailEvaluation, GuardrailOutcome)
- 3 helper functions
- 4 validation functions
- **Commit:** 3448e2bcb

**Compilation:** ✅ Successful

---

## Step 1.3: COMPLETE ✅

**Removed duplicate inline definitions from chatwidget.rs:**
- Deleted 223 lines (181 predicted)
- Added import statement
- chatwidget.rs: 23,028 → 22,847 lines
- **Commit:** 872a9e03c

**Compilation:** ✅ Successful (35.01s)

---

## Step 1.4: Handler Method Extraction (TODO - NEXT)

### Inline Structs Found in chatwidget.rs

**Line 16678: enum SpecAutoPhase**
```rust
enum SpecAutoPhase {
    Guardrail,
    ExecutingAgents {
        expected_agents: Vec<String>,
        completed_agents: std::collections::HashSet<String>,
    },
    CheckingConsensus,
}
```
**Action:** This is MORE complex than state.rs simple enum. Need to preserve this.

**Line 16711: struct GuardrailEvaluation**
```rust
struct GuardrailEvaluation {
    // Need to read full definition
}
```

**Line 17145: struct GuardrailWait**
```rust
struct GuardrailWait {
    // Need to read full definition
}
```

**Line 17152: struct SpecAutoState**
```rust
struct SpecAutoState {
    spec_id: String,
    goal: String,
    stages: Vec<SpecStage>,
    current_index: usize,
    phase: SpecAutoPhase,
    waiting_guardrail: Option<GuardrailWait>,
    validate_retries: u32,
    pending_prompt_summary: Option<String>,
    hal_mode: Option<HalMode>,
}
```
**Action:** This is the REAL state struct. My state.rs has wrong fields.

**Line 17165: struct GuardrailOutcome**
```rust
struct GuardrailOutcome {
    success: bool,
    summary: String,
    telemetry_path: Option<PathBuf>,
    failures: Vec<String>,
}
```

### Next Actions

1. **Read full definitions** of all 5 structs (lines 16678-17200)
2. **Replace state.rs** with actual struct definitions
3. **Find helper functions** (guardrail_for_stage, spec_ops_stage_prefix) - move to state.rs or separate module
4. **Update imports** in chatwidget.rs

---

## Step 1.3: Handler Method Extraction (TODO)

### Methods to Extract (Found via grep)

**Search command:**
```bash
grep -n "fn handle_spec_.*(&mut self" codex-rs/tui/src/chatwidget.rs
```

**Expected methods (~10 total, ~2,500 lines):**
- handle_spec_plan_command
- handle_spec_tasks_command
- handle_spec_implement_command
- handle_spec_validate_command
- handle_spec_audit_command
- handle_spec_unlock_command
- handle_spec_ops_command
- handle_spec_consensus_command
- handle_spec_status_command
- advance_spec_auto_phase
- auto_submit_spec_stage_prompt
- halt_spec_auto_with_error

**Strategy:**
- Extract each method to SpecKitHandler
- Methods need access to ChatWidget fields (config, history, app_event_tx, etc.)
- Option A: Pass &mut ChatWidget to handler methods (simpler)
- Option B: Create ChatContext struct with needed fields (cleaner but more work)

**Recommendation:** Option A for speed

---

## Step 1.4: Delegation (TODO)

### ChatWidget Changes

**Add field:**
```rust
pub struct ChatWidget {
    // ... existing fields ...
    spec_kit: SpecKitHandler,  // New field
}
```

**Replace inline methods with delegation:**
```rust
// Before (inline, ~200 lines)
fn handle_spec_plan_command(&mut self, args: &str) {
    // ... 200 lines of logic ...
}

// After (delegation, ~5 lines)
fn handle_spec_plan_command(&mut self, args: &str) {
    self.spec_kit.handle_plan(args, self);
}
```

---

## Compilation Testing Protocol

**After each sub-step:**
```bash
cd codex-rs
cargo build -p codex-tui --profile dev-fast
```

**Expected:** May need 2-3 iterations to fix:
- Field access errors (self.field → widget.field)
- Import errors (use statements)
- Type mismatches

---

## Session Continuation Prompt

**For next session:**

```
Continue refactoring Phase 1 - spec-kit module extraction.

Current state:
- Branch: refactor/spec-kit-module-extraction
- Last commit: 892d1e4a2 (foundation)
- Module structure created ✅
- State extraction: IN PROGRESS

Next steps (Step 1.2):
1. Read actual state struct definitions from chatwidget.rs:16678-17200
2. Replace spec_kit/state.rs with complete definitions
3. Update chatwidget.rs imports
4. Test compilation

Reference:
- docs/spec-kit/REFACTORING_PLAN.md - Overall plan
- docs/spec-kit/REFACTORING_SESSION_NOTES.md - This document (session notes)
- docs/spec-kit/FORK_ISOLATION_AUDIT.md - Why we're doing this

Start with: Read lines 16678-17300 of chatwidget.rs and update state.rs with complete definitions.
```

---

**Document Version:** 1.0 (Session 1)
**Status:** Step 1.1 complete, Step 1.2 in progress
**Owner:** @just-every/automation
