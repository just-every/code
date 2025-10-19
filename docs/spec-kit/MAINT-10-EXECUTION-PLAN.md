# MAINT-10 Execution Plan: Spec-Kit Extraction

**Status**: Ready for execution
**Estimated Effort**: 20-30 hours over 2-4 weeks
**Risk Level**: HIGH (604 tests @ 100% pass rate at risk)
**Strategic Value**: Deferred until reusability need emerges

---

## Executive Summary

**DO NOT EXECUTE YET - DEFER UNTIL**:
1. ✅ Upstream sync complete (post 2026-01-15)
2. ✅ Reusability requirement emerges (CLI tool, API server, library need)
3. ✅ Fresh session with dedicated focus (20-30 hour commitment)

**Current State**: Phase 1 foundation complete (10%), remaining 90% requires careful execution

---

## Phase 1: Foundation (COMPLETE ✅)

**Delivered** (2025-10-18):
- ✅ spec-kit/Cargo.toml - Workspace member, dependencies
- ✅ spec-kit/src/error.rs - SpecKitError enum (89 LOC)
- ✅ spec-kit/src/types.rs - SpecStage, SpecAgent, HalMode (145 LOC)
- ✅ spec-kit/src/api.rs - Async API skeleton (135 LOC)
- ✅ spec-kit/src/lib.rs - Module exports (35 LOC)

**Total**: ~400 LOC, 10% complete

---

## Phase 2A: Foundation Modules (2-3 hours, LOW RISK)

### Objective
Migrate lowest-risk data structure modules to prove extraction approach

### Modules to Migrate
1. **state.rs** (490 LOC):
   - Core: SpecAutoState, SpecAutoPhase, QualityCheckpoint
   - Quality types: QualityIssue, GPT5ValidationResult, Resolution
   - Helpers: validate_guardrail_evidence, get_nested, require_*

2. **schemas.rs** (197 LOC):
   - JSON schemas for agent responses
   - Schema generation functions
   - Provider support detection

### Migration Steps

**Step 1**: Extract HalMode to spec-kit
```bash
# Already added to spec-kit/src/types.rs
# Add unit tests for HalMode::from_str()
```

**Step 2**: Copy state.rs to spec-kit
```bash
# Create spec-kit/src/state.rs
# Remove TUI-specific parts:
#   - guardrail_for_stage() (returns SlashCommand)
#   - GuardrailWait struct (contains SlashCommand)
#   - Change waiting_guardrail to waiting_guardrail_name: Option<String>
# Update imports:
#   - use crate::types::{SpecStage, HalMode};
#   - Remove crate::slash_command imports
```

**Step 3**: Copy schemas.rs to spec-kit
```bash
# Create spec-kit/src/schemas.rs
# Update imports:
#   - use crate::state::QualityGateType;
# Keep all logic intact (no TUI dependencies)
```

**Step 4**: Update spec-kit/src/lib.rs
```rust
pub mod error;
pub mod types;
pub mod state;
pub mod schemas;

pub use error::{Result, SpecKitError};
pub use types::{SpecStage, SpecAgent, HalMode};
pub use state::*;  // Export all state types
pub use schemas::*;  // Export schema functions
```

**Step 5**: Update TUI imports
```bash
# In tui/src/chatwidget/spec_kit/mod.rs:
# Change: pub use super::state::*;
# To: pub use codex_spec_kit::state::*;

# Keep TUI-specific helpers in TUI:
# - guardrail_for_stage() stays in TUI
# - GuardrailWait stays in TUI (or deprecated)
```

**Step 6**: Update test imports
```bash
# In all test files:
# Change: use codex_tui::{SpecStage, SpecAutoState, HalMode, ...};
# To: use codex_spec_kit::{SpecStage, SpecAutoState, HalMode, ...};
# Keep: use codex_tui::{ChatWidget, ...} for TUI-specific imports
```

**Step 7**: Verify compilation
```bash
cd /home/thetu/code/codex-rs
cargo build -p codex-spec-kit
cargo build -p codex-tui
```

**Step 8**: Run full test suite
```bash
cargo test -p codex-tui --features test-utils --lib --tests
# Verify: 604 tests, 100% pass rate
```

### Validation Gate
- ✅ Both crates compile
- ✅ All 604 tests passing
- ✅ No new warnings
- **STOP if ANY test fails** - debug before continuing

---

## Phase 2B: Evidence Module (2-3 hours, MEDIUM RISK)

### Modules to Migrate
- **evidence.rs** (679 LOC):
  - EvidenceRepository trait
  - FilesystemEvidence implementation
  - File locking logic
  - Path helpers

### Dependencies
- fs2 (already in spec-kit Cargo.toml)
- serde_json ✅
- PathBuf ✅
- SpecStage ✅ (from Phase 2A)

### Migration Steps

**Step 1**: Copy evidence.rs to spec-kit
```bash
# Create spec-kit/src/evidence.rs
# Update imports:
#   - use crate::types::SpecStage;
#   - use crate::error::{Result, SpecKitError};
# Remove any TUI-specific code
```

**Step 2**: Add dependencies to Cargo.toml (if needed)
```toml
# spec-kit/Cargo.toml already has fs2
```

**Step 3**: Update lib.rs
```rust
pub mod evidence;
pub use evidence::*;
```

**Step 4**: Update TUI imports
```rust
// Change: use super::evidence::*;
// To: use codex_spec_kit::evidence::*;
```

**Step 5**: Run tests
```bash
cargo test -p codex-spec-kit  # New crate tests
cargo test -p codex-tui --features test-utils --test evidence_tests
# Verify: 24 evidence tests passing
```

### Validation Gate
- ✅ Evidence tests passing (24 tests)
- ✅ File locking works
- ✅ Path helpers correct

---

## Phase 2C: Consensus Module (3-4 hours, HIGH RISK)

### Modules to Migrate
- **consensus.rs** (1,024 LOC):
  - run_spec_consensus() - Main async function
  - collect_consensus_artifacts()
  - MCP integration
  - Consensus synthesis logic

### Dependencies
- codex-core::mcp_connection_manager ✅ (already in spec-kit Cargo.toml)
- evidence.rs ✅ (from Phase 2B)
- SpecStage ✅

### Migration Steps

**Critical**: consensus.rs is already async, good fit for spec-kit

**Step 1**: Copy consensus.rs to spec-kit
```bash
# Create spec-kit/src/consensus.rs
# Update imports:
#   - use crate::types::SpecStage;
#   - use crate::evidence::*;
#   - use crate::error::Result;
```

**Step 2**: Handle SpecKitContext dependency
```rust
// consensus.rs calls context for UI updates
// Use SpecKitContext trait from api.rs
```

**Step 3**: Update lib.rs
```rust
pub mod consensus;
pub use consensus::*;
```

**Step 4**: Update TUI
```rust
// Change: use super::consensus::*;
// To: use codex_spec_kit::consensus::*;
```

**Step 5**: Run tests
```bash
cargo test -p codex-tui --features test-utils --test consensus_logic_tests
# Verify: 42 consensus tests passing
```

### Validation Gate
- ✅ Consensus logic tests passing (42 tests)
- ✅ MCP integration works
- ✅ Evidence writes correct

---

## Phase 2D: Quality Modules (3-4 hours, HIGH RISK)

### Modules to Migrate
- **quality.rs** (837 LOC)
- **quality_gate_handler.rs** (869 LOC)

### Dependencies
- evidence.rs ✅
- state.rs ✅ (QualityCheckpoint, QualityIssue)
- SpecKitContext trait (for UI callbacks)

### Migration Steps

**Step 1**: Copy quality.rs
```bash
# Create spec-kit/src/quality.rs
# Update imports to use spec-kit modules
```

**Step 2**: Copy quality_gate_handler.rs
```bash
# Create spec-kit/src/quality_gate.rs (rename for clarity)
# Update imports
# Use SpecKitContext trait for UI callbacks
```

**Step 3**: Update lib.rs
```rust
pub mod quality;
pub mod quality_gate;
```

**Step 4**: Run tests
```bash
cargo test -p codex-tui --features test-utils --test quality_resolution_tests
# Verify: 33 quality tests passing
```

### Validation Gate
- ✅ Quality resolution tests passing (33 tests)
- ✅ Auto-resolution logic works
- ✅ GPT-5 validation works

---

## Phase 2E: Guardrail Module (2-3 hours, MEDIUM RISK)

### Modules to Migrate
- **guardrail.rs** (670 LOC):
  - Guardrail execution logic
  - Telemetry parsing
  - Schema validation

### Dependencies
- evidence.rs ✅
- schemas.rs ✅
- state.rs ✅

### Migration Steps

**Step 1**: Copy guardrail.rs
```bash
# Create spec-kit/src/guardrail.rs
```

**Step 2**: Update lib.rs and run tests
```bash
cargo test -p codex-tui --features test-utils --test guardrail_tests
# Verify: 25 guardrail tests passing
```

### Validation Gate
- ✅ Guardrail tests passing (25 tests)
- ✅ Telemetry parsing works

---

## Phase 3: Handler Orchestration (4-6 hours, VERY HIGH RISK)

### Modules to Migrate
- **handler.rs** (962 LOC):
  - run_spec_auto_interactive()
  - check_consensus_and_advance_spec_auto()
  - Retry logic (AR-2, AR-3, AR-4)
  - Stage advancement

### Dependencies
- ALL previous modules ✅
- SpecKitContext trait (for UI callbacks)

### Migration Steps

**Step 1**: Move handler logic to spec-kit
```bash
# Create spec-kit/src/handler.rs
# This is the orchestrator - depends on everything
```

**Step 2**: Update SpecKitEngine API
```rust
// In spec-kit/src/api.rs:
impl SpecKitEngine {
    pub async fn run_auto_pipeline<C: SpecKitContext>(...) -> Result<PipelineResult> {
        // Call handler::run_spec_auto_interactive()
    }
}
```

**Step 3**: Run handler tests
```bash
cargo test -p codex-tui --features test-utils --test handler_orchestration_tests
# Verify: 58 handler tests passing
```

### Validation Gate
- ✅ Handler orchestration tests passing (58 tests)
- ✅ Retry logic works (AR-2, AR-3, AR-4)
- ✅ Stage advancement correct

---

## Phase 4: Context & TUI Adapter (2-3 hours, MEDIUM RISK)

### Modules to Migrate/Create
- **context.rs** (210 LOC) - SpecKitContext trait implementation
- **TUI adapter** - Bridge sync Ratatui → async spec-kit

### Strategy

**Option A**: Move SpecKitContext trait to spec-kit (RECOMMENDED)
```rust
// spec-kit/src/api.rs already has SpecKitContext trait
// TUI implements it via adapter

// In TUI:
pub struct TuiSpecKitContext<'a> {
    widget: &'a mut ChatWidget<'a>,
    runtime: tokio::runtime::Handle,
}

#[async_trait]
impl SpecKitContext for TuiSpecKitContext<'_> {
    async fn display_message(&mut self, msg: String) {
        self.runtime.block_on(async {
            // Call widget methods
        });
    }
    // ... implement other methods
}
```

**Step 1**: Create TUI adapter
```bash
# Create tui/src/chatwidget/spec_kit_adapter.rs
# Implements SpecKitContext trait for ChatWidget
```

**Step 2**: Update handler calls in TUI
```rust
// In chatwidget/mod.rs:
let mut adapter = TuiSpecKitContext::new(&mut self, runtime);
let engine = SpecKitEngine::new(cwd, mcp_manager)?;
engine.run_auto_pipeline(spec_id, &mut adapter).await?;
```

### Validation Gate
- ✅ TUI can call spec-kit APIs
- ✅ Async bridge works (Handle::block_on)
- ✅ All 604 tests still passing

---

## Phase 5: Slash Commands (2-3 hours, LOW RISK)

### Commands to Update (6 files)
- commands/new.rs
- commands/special.rs (auto, status, consensus)
- commands/stages.rs (plan, tasks, implement, validate, audit, unlock)
- commands/quality.rs (clarify, checklist, analyze)
- commands/guardrail.rs (guardrail wrappers)

### Strategy

**Keep commands in TUI** (thin wrappers calling spec-kit)

```rust
// Before:
pub fn handle_spec_plan(&mut self, args: String) {
    // Direct call to spec_kit modules
}

// After:
pub fn handle_spec_plan(&mut self, args: String) {
    let engine = SpecKitEngine::new(self.cwd(), self.mcp_manager())?;
    let mut adapter = TuiSpecKitContext::new(self);
    self.runtime.block_on(async {
        engine.run_stage("SPEC-ID", SpecStage::Plan, &mut adapter).await
    })?;
}
```

### Validation Gate
- ✅ All 13 /speckit.* commands work
- ✅ All 7 /guardrail.* commands work
- ✅ Slash command tests passing

---

## Phase 6: Cleanup & Documentation (1-2 hours, LOW RISK)

### Tasks

**Step 1**: Remove duplicate code from TUI
```bash
# Delete migrated modules from tui/src/chatwidget/spec_kit/
rm tui/src/chatwidget/spec_kit/state.rs
rm tui/src/chatwidget/spec_kit/schemas.rs
# ... etc for all migrated modules
```

**Step 2**: Update re-exports in TUI
```rust
// tui/src/chatwidget/spec_kit/mod.rs becomes mostly re-exports:
pub use codex_spec_kit::*;

// Keep TUI-specific code:
mod adapter;  // TuiSpecKitContext
mod commands;  // Slash command handlers
```

**Step 3**: Update documentation
- SPEC.md: Mark MAINT-10 as DONE
- Update MAINT-10-EXTRACTION-PLAN.md with completion status
- Document API usage examples

**Step 4**: Final test verification
```bash
cargo test --workspace --features test-utils
# Verify: 604 tests @ 100% pass rate
```

### Validation Gate
- ✅ No duplicate code remains
- ✅ Clean separation: spec-kit (library) vs TUI (consumer)
- ✅ All 604 tests passing
- ✅ Documentation updated

---

## Dependency Extraction Strategy

### Types Already in spec-kit
- ✅ SpecStage
- ✅ SpecAgent
- ✅ HalMode (added Phase 2A)
- ✅ SpecKitError

### Types to Extract (from state.rs)
- SpecAutoState
- SpecAutoPhase
- QualityCheckpoint
- QualityGateType
- QualityIssue
- GPT5ValidationResult
- Resolution
- EscalatedQuestion
- Confidence, Magnitude, Resolvability

### TUI-Specific (Keep in TUI)
- SlashCommand enum
- ~~GuardrailWait~~ (deprecated, replace with string)
- guardrail_for_stage() (maps SpecStage → SlashCommand)

---

## Test Migration Strategy

### Imports to Update (All 19 test files)

**From**:
```rust
use codex_tui::{SpecStage, SpecAutoState, HalMode, QualityCheckpoint, ...};
```

**To**:
```rust
use codex_spec_kit::{SpecStage, SpecAutoState, HalMode, QualityCheckpoint, ...};
use codex_tui::{/* TUI-specific only */};
```

### Test Files Affected (19 files, 604 tests)
1. workflow_integration_tests.rs
2. error_recovery_integration_tests.rs
3. state_persistence_integration_tests.rs
4. quality_flow_integration_tests.rs
5. concurrent_operations_integration_tests.rs
6. edge_case_tests.rs
7. property_based_tests.rs
8. handler_orchestration_tests.rs
9. consensus_logic_tests.rs
10. quality_resolution_tests.rs
11. evidence_tests.rs
12. guardrail_tests.rs
13. state_tests.rs
14. schemas_tests.rs
15. error_tests.rs
16. spec_auto_e2e.rs
17. quality_gates_integration.rs
18. mcp_consensus_integration.rs
19. spec_status.rs

### Migration Script
```bash
# Automated import updates:
for file in tui/tests/*.rs; do
    sed -i 's/use codex_tui::{SpecStage/use codex_spec_kit::{SpecStage/g' "$file"
    sed -i 's/use codex_tui::SpecStage/use codex_spec_kit::SpecStage/g' "$file"
    # Add more patterns as needed
done
```

---

## Risk Mitigation

### Risk 1: Test Breakage (HIGH)
**Mitigation**:
- Run tests after EACH module migration
- Keep TUI version alongside spec-kit during transition
- Use feature flags if needed (`spec-kit-extracted`)

### Risk 2: Circular Dependencies (HIGH)
**Mitigation**:
- SpecKitContext trait stays in spec-kit
- TUI implements trait, doesn't define it
- No spec-kit → TUI dependencies

### Risk 3: Async/Sync Mismatch (MEDIUM)
**Mitigation**:
- Spec-kit fully async
- TUI adapter uses Handle::block_on()
- Clear boundary at adapter layer

### Risk 4: Path Handling (MEDIUM)
**Mitigation**:
- Pass cwd through API
- Keep DEFAULT_EVIDENCE_BASE constant
- Test on relative & absolute paths

---

## Rollback Plan

**If extraction fails at any phase**:

**Step 1**: Identify failure point
- Which module caused test failures?
- Which tests are failing?

**Step 2**: Revert changes
```bash
git reset --hard <last-good-commit>
```

**Step 3**: Document blocker
```markdown
# In MAINT-10-EXECUTION-PLAN.md:
## Blocker Encountered
- Phase: <phase name>
- Module: <module name>
- Tests failing: <count>
- Root cause: <description>
- Resolution: <defer|redesign|investigate>
```

**Step 4**: Defer to future
- Phase 1 foundation remains
- Can resume when blocker resolved

---

## Success Criteria

**MAINT-10 Complete When**:
- ✅ All 15 modules migrated to spec-kit crate
- ✅ TUI adapter implements SpecKitContext trait
- ✅ All 604 tests passing @ 100% pass rate
- ✅ Both crates compile without warnings
- ✅ No code duplication (clean extraction)
- ✅ Documentation updated (SPEC.md, API docs)
- ✅ CLI proof-of-concept works (optional)

**Estimated Timeline**: 3-4 weeks part-time OR 1 week full-time focus

---

## RECOMMENDATION

**DEFER EXECUTION** until:
1. Upstream sync complete (2026-01-15)
2. Strategic value emerges (CLI, API server, library need)
3. Dedicated 1-week focus window available

**Current Priorities** (Higher Value):
- Production use of spec-kit (13 /speckit.* commands operational)
- New feature development
- User feedback incorporation
- Performance optimization

**Phase 1 Foundation Sufficient**:
- Types extracted (SpecStage, SpecAgent, HalMode)
- API skeleton defined
- Can resume extraction when needed (10% → 100% in 20-30 hours)

---

**Decision Point**: Proceed with Phase 2A NOW or defer to future session?

**If deferring**: Mark MAINT-10 as "Deferred" in SPEC.md with clear resumption criteria.
**If proceeding**: Start Phase 2A immediately (expect 2-3 hour commitment for state.rs + schemas.rs).
