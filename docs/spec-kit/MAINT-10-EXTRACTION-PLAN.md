# MAINT-10: Spec-Kit Extraction Plan

**Status**: Foundation Complete, Migration In Progress
**Started**: 2025-10-18
**Estimated Completion**: 2-4 weeks
**Priority**: P3 (Deferred until reusability need)

---

## 1. Foundation Complete ✅

**New Crate Created**: `codex-rs/spec-kit/`

**Files Created**:
- `Cargo.toml` - Package manifest with dependencies
- `src/lib.rs` - Crate root with path helpers
- `src/error.rs` - SpecKitError taxonomy (migrated from TUI)
- `src/types.rs` - SpecStage, SpecAgent enums (migrated from spec_prompts.rs)
- `src/api.rs` - Async-first API surface (SpecKitEngine, traits)

**API Design**:
```rust
// Async-first engine (no Handle::block_on needed)
let engine = SpecKitEngine::new(cwd, mcp_manager)?;

// Run consensus (async)
let (summary, degraded) = engine.run_consensus("SPEC-065", SpecStage::Plan).await?;

// Run full pipeline with UI context
let result = engine.run_auto_pipeline("SPEC-065", &mut context).await?;

// SpecKitContext trait abstracts UI (TUI, CLI, API)
#[async_trait]
trait SpecKitContext {
    async fn display_message(&mut self, msg: String);
    async fn submit_agent_prompt(&mut self, ...) -> Result<String>;
    // ... etc
}
```

---

## 2. Migration Phases

### Phase 1: Foundation (DONE, ~1 hour)
- ✅ Create new crate structure
- ✅ Define error types (SpecKitError)
- ✅ Define core types (SpecStage, SpecAgent)
- ✅ Design async API surface (SpecKitEngine, SpecKitContext trait)

### Phase 2: Core Modules (3-5 days)
- [ ] Move `evidence.rs` (499 LOC) - EvidenceRepository trait, file locking
- [ ] Move `consensus.rs` (992 LOC) - MCP native consensus, artifact parsing
- [ ] Move `state.rs` (414 LOC) - SpecAutoState, SpecAutoPhase
- [ ] Move `schemas.rs` (226 LOC) - JSON schemas
- [ ] Convert to async-native (remove Handle::block_on)

### Phase 3: Handlers & Logic (5-7 days)
- [ ] Move `handler.rs` (961 LOC) - Orchestration logic
- [ ] Move `quality_gate_handler.rs` (869 LOC) - Quality gates
- [ ] Move `quality.rs` (807 LOC) - Auto-resolution logic
- [ ] Move `guardrail.rs` (589 LOC) - Validation harness
- [ ] Move `file_modifier.rs` (429 LOC) - Spec modifications

### Phase 4: Commands & Registry (3-4 days)
- [ ] Move `command_registry.rs` (410 LOC) - Dynamic registry
- [ ] Move `commands/*` (6 files, ~600 LOC) - Command implementations
- [ ] Move `routing.rs` (134 LOC) - Dispatch helpers
- [ ] Move `config_validator.rs` (294 LOC) - Config validation

### Phase 5: TUI Adapter (2-3 days)
- [ ] Implement SpecKitContext for ChatWidget (in TUI crate)
- [ ] Create async→sync bridge (Handle::block_on in TUI, not spec-kit)
- [ ] Update all TUI imports (spec_kit:: → codex_spec_kit::)
- [ ] Remove old `tui/src/chatwidget/spec_kit/` directory

### Phase 6: CLI & Testing (3-4 days)
- [ ] Create CLI proof-of-concept (`code spec-auto --headless SPEC-ID`)
- [ ] Migrate all tests (178 tests → spec-kit crate)
- [ ] Update MockSpecKitContext for new API
- [ ] Verify zero regressions

---

## 3. API Surface (Final Design)

### SpecKitEngine (Main Entry Point)

```rust
pub struct SpecKitEngine {
    cwd: PathBuf,
    mcp_manager: Arc<McpConnectionManager>,
}

impl SpecKitEngine {
    // Core operations
    pub async fn run_consensus(spec_id, stage) -> Result<(ConsensusSummary, bool)>;
    pub async fn run_auto_pipeline(spec_id, context) -> Result<PipelineResult>;
    pub async fn run_quality_checkpoint(spec_id, checkpoint, context) -> Result<QualityGateResult>;

    // Per-stage operations
    pub async fn run_stage(spec_id, stage, context) -> Result<StageResult>;
    pub async fn validate_guardrail(spec_id, stage) -> Result<GuardrailOutcome>;
}
```

### SpecKitContext Trait (UI Abstraction)

```rust
#[async_trait]
pub trait SpecKitContext: Send + Sync {
    // Display operations
    async fn display_message(&mut self, message: String);
    async fn display_error(&mut self, error: String);
    async fn display_progress(&mut self, stage: SpecStage, percent: u8);

    // Agent operations
    async fn submit_agent_prompt(&mut self, display: String, prompt: String) -> Result<String>;
    async fn active_agent_names(&self) -> Vec<String>;

    // User interaction
    async fn request_quality_answers(&mut self, checkpoint, questions) -> Result<Vec<String>>;

    // Configuration
    fn working_directory(&self) -> &Path;
    fn agent_config(&self) -> &[AgentConfig];
}
```

### Implementation Examples

**TUI (Ratatui)**:
```rust
// In codex-tui/src/chatwidget/spec_kit_adapter.rs
impl SpecKitContext for ChatWidget {
    async fn display_message(&mut self, msg: String) {
        // Bridge async→sync (block_on in TUI, not spec-kit)
        self.history_push(PlainHistoryCell::new(vec![Line::from(msg)], Notice));
    }

    async fn submit_agent_prompt(&mut self, display: String, prompt: String) -> Result<String> {
        self.submit_prompt_with_display(display, prompt);
        Ok("agent_id".to_string())
    }

    fn working_directory(&self) -> &Path {
        &self.config.cwd
    }
}
```

**CLI (Headless)**:
```rust
struct CliContext {
    cwd: PathBuf,
    agents: Vec<AgentConfig>,
}

#[async_trait]
impl SpecKitContext for CliContext {
    async fn display_message(&mut self, msg: String) {
        println!("{}", msg);  // Direct stdout (no TUI)
    }

    async fn submit_agent_prompt(&mut self, display: String, prompt: String) -> Result<String> {
        // Call codex-core directly
        codex.submit_prompt(prompt).await
    }

    async fn request_quality_answers(&mut self, checkpoint, questions) -> Result<Vec<String>> {
        // CLI prompts user for answers
        for q in questions {
            print!("{}: ", q);
            let answer = read_line()?;
            answers.push(answer);
        }
        Ok(answers)
    }
}
```

---

## 4. Migration Checklist

**Per Module** (repeat for each of 15 modules):
- [ ] Copy module file to `spec-kit/src/{module}.rs`
- [ ] Update imports (`super::` → `crate::`, `ChatWidget` → generic context)
- [ ] Remove `Handle::block_on` (make functions async)
- [ ] Update `spec-kit/src/lib.rs` to expose module
- [ ] Add module to TUI imports (`use codex_spec_kit::{module}`)
- [ ] Run tests, fix breakage
- [ ] Delete original TUI file once migration confirmed

**Workspace Integration**:
- [ ] Add `spec-kit` to workspace members in `Cargo.toml`
- [ ] Add `codex-spec-kit = { path = "../spec-kit" }` to TUI dependencies
- [ ] Update TUI re-exports: `pub use codex_spec_kit::*;`

---

## 5. Breaking Changes & Risks

**API Changes**:
- Functions become async (callers must await)
- SpecKitContext trait required (can't use ChatWidget directly)
- Module paths change (`tui::spec_kit` → `codex_spec_kit`)

**Risks**:
- Test breakage (178 tests to migrate)
- Import churn (every file importing spec_kit needs update)
- Async conversion bugs (Handle::block_on removal)
- Performance regression (async overhead)

**Mitigations**:
- Migrate one module at a time (incremental)
- Keep TUI re-exports working during migration
- Run tests after each module
- Use feature flags if partial migration needed

---

## 6. Benefits

**Reusability**:
- CLI mode: `code spec-auto --headless SPEC-ID` (no TUI)
- API mode: HTTP server exposing spec-kit endpoints
- CI/CD: Automated spec generation in pipelines

**Maintainability**:
- TUI chatwidget: 21,412→13,529 LOC (36% reduction)
- Spec-kit: Independent crate, async-native
- Cleaner boundaries: UI concerns separate from automation logic

**Upstream Sync**:
- Conflict surface: ~180 LOC → ~50 LOC (trait impl only)
- Isolation: 98.8% → ~99.8%
- Merge pain: Reduced (spec-kit entirely fork-only crate)

---

## 7. Completion Criteria

**Phase Complete When**:
- [ ] All 15 modules migrated to `spec-kit/`
- [ ] TUI uses `codex_spec_kit` crate (no local spec_kit/ directory)
- [ ] All 185 tests passing (no regressions)
- [ ] CLI proof-of-concept works: `code spec-auto --headless SPEC-TEST`
- [ ] Documentation updated (SPEC.md, AGENTS.md, PLANNING.md)
- [ ] chatwidget/mod.rs reduced by ~7,883 LOC

**Success Metrics**:
- TUI chatwidget: <14k LOC (currently 21,412)
- Spec-kit crate: ~8k LOC (isolated, reusable)
- Test coverage: Maintained or improved
- Performance: No regression (8.7ms MCP maintained)

---

## 8. Current Progress (2025-10-18)

**Foundation**: ✅ Complete (~1 hour)
- New crate created with Cargo.toml
- Error types defined (SpecKitError, 15 variants)
- Core types migrated (SpecStage, SpecAgent)
- Async API designed (SpecKitEngine, SpecKitContext trait)
- Path helpers centralized (consensus_dir, commands_dir)

**Remaining**: ~13-19 days
- Move 15 modules + commands (Phase 2-4)
- Create TUI adapter (Phase 5)
- CLI proof-of-concept (Phase 6)

**Next Session**: Start Phase 2 (move evidence.rs, consensus.rs, state.rs)

---

## 9. Handoff Notes

**When Resuming MAINT-10**:
1. Start with evidence.rs (smallest, self-contained)
2. Then consensus.rs (core logic, well-tested)
3. Build up incrementally (one module per commit)
4. Keep TUI working throughout (re-exports)

**Quick Start**:
```bash
# Add to workspace
echo '    "spec-kit",' >> Cargo.toml  # In [workspace.members]

# Build new crate
cargo build -p codex-spec-kit

# Start migrating modules
cp tui/src/chatwidget/spec_kit/evidence.rs spec-kit/src/
# ... update imports, make async, test ...
```

---

## 10. Why Foundation Only (For Now)

**Reason**: 2-4 week effort, limited session tokens

**Foundation Value**:
- API design validated (SpecKitEngine, SpecKitContext)
- Clear migration plan (6 phases, checklists)
- New crate scaffolding ready
- Handoff documented for future sessions

**Decision**: Prioritize test coverage Phase 2 (higher immediate value)

**Resume When**: Reusability need emerges or next maintenance sprint
