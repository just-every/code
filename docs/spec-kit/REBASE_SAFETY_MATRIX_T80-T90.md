# Rebase Safety Matrix - Tasks T80-T90

**Date:** 2025-10-17
**Context:** Architecture review identified 10 improvement tasks
**Critical Requirement:** ALL implementations must maintain 98.8% isolation for easy upstream rebasing

---

## Rebase Safety Principles

### ✅ SAFE PATTERNS (Zero/Low Conflict Risk)
1. **New files in isolated modules** (`tui/src/chatwidget/spec_kit/`)
2. **New test files** (`tui/tests/`)
3. **New documentation** (`docs/spec-kit/`)
4. **New scripts** (`scripts/`)
5. **Minimal delegation** in upstream files (1-5 lines per touchpoint)

### ❌ UNSAFE PATTERNS (High Conflict Risk)
1. **Inline code in chatwidget/mod.rs** (>10 lines)
2. **New fields in upstream structs** (unless absolutely necessary)
3. **Modifications to upstream enums** (beyond minimal fork variants)
4. **Logic in app.rs** (keep routing only)
5. **Logic in core/** (unless in new modules)

---

## Task-by-Task Analysis

### T80: Unify Orchestration Paths ⚠️ MODERATE RISK → ✅ MITIGATED

**Problem:** Duplicate Rust (handler.rs) + bash (spec_auto.sh) orchestration

**UNSAFE Approach:**
```rust
// ❌ Adding 500 lines to handler.rs
pub fn handle_spec_auto_native(widget: &mut ChatWidget) {
    // Inline all bash logic here
    // ...500 lines...
}
```

**SAFE Approach:**
```rust
// ✅ NEW FILE: tui/src/chatwidget/spec_kit/orchestrator.rs (500 lines)
pub struct Orchestrator {
    evidence: Box<dyn EvidenceRepository>,
    context: Box<dyn SpecKitContext>,
}

impl Orchestrator {
    pub fn run_pipeline(&mut self, spec_id: &str) -> Result<()> {
        // All logic isolated here
    }
}

// ✅ MINIMAL CHANGE: handler.rs (+5 lines)
pub fn handle_spec_auto(widget: &mut ChatWidget, spec_id: String) {
    let orchestrator = Orchestrator::new(widget);
    orchestrator.run_pipeline(&spec_id);
}

// ✅ DEPRECATE: spec_auto.sh becomes thin wrapper
# spec_auto.sh (5 lines)
#!/bin/bash
exec codex-tui --spec-auto "$1"  # Calls Rust directly
```

**Rebase Impact:**
- New file: `orchestrator.rs` - **Zero conflict**
- Changed: `handler.rs` - **+5 lines** (minimal delegation)
- Scripts: wrapper only - **Low conflict**

**Isolation:** 99.5% (stays isolated in spec_kit module)

---

### T81: Consolidate Consensus Logic ⚠️ HIGH RISK → ✅ MITIGATED

**Problem:** bash→Python→MCP chain is brittle

**UNSAFE Approach:**
```rust
// ❌ Modify core/src/mcp_client.rs (upstream file)
impl McpClient {
    pub fn query_consensus(&self) -> Result<Consensus> {
        // Adding 200 lines to upstream code
    }
}
```

**SAFE Approach:**
```rust
// ✅ NEW FILE: tui/src/chatwidget/spec_kit/consensus_native.rs (300 lines)
pub struct ConsensusEngine {
    mcp: McpClient,  // Use existing, don't modify
}

impl ConsensusEngine {
    pub fn synthesize(&self, spec_id: &str, stage: SpecStage) -> Result<ConsensusSynthesis> {
        // Pure Rust logic
        // Calls MCP via existing public API
        self.mcp.call_tool("local-memory", json!({"query": ...}))
    }
}

// ✅ REPLACE: consensus.rs now uses ConsensusEngine
pub fn run_spec_consensus(ctx: &impl SpecKitContext, spec_id: &str) -> Result<bool> {
    let engine = ConsensusEngine::new(ctx.mcp_client());
    engine.synthesize(spec_id, stage)
}
```

**Rebase Impact:**
- New file: `consensus_native.rs` - **Zero conflict**
- Changed: `consensus.rs` - **Replace 200 lines** (internal to spec_kit)
- Deleted: `scripts/spec_consensus.sh`, `local-memory-stub.py` - **Zero conflict** (fork-only files)
- Unchanged: `mcp-client/src/lib.rs` - **Zero conflict** (no upstream modifications)

**Isolation:** 100% (no upstream files touched)

---

### T82: Complete SpecKitContext Migration ✅ LOW RISK

**Problem:** 8/10 handlers still use `&mut ChatWidget` directly

**UNSAFE Approach:**
```rust
// ❌ Add methods to ChatWidget (upstream struct)
impl ChatWidget {
    pub fn spec_kit_method_1(&mut self) { /* ... */ }
    pub fn spec_kit_method_2(&mut self) { /* ... */ }
    // +200 lines in upstream file
}
```

**SAFE Approach:**
```rust
// ✅ EXTEND EXISTING: tui/src/chatwidget/spec_kit/context.rs (+50 lines)
pub trait SpecKitContext {
    // Existing methods...

    // NEW: Add missing abstractions
    fn submit_quality_gate_agents(&mut self, checkpoint: QualityCheckpoint) -> Result<()>;
    fn handle_gpt5_validation(&mut self, issues: Vec<QualityIssue>) -> Result<()>;
}

// ✅ MINIMAL CHANGE: chatwidget/mod.rs (+20 lines)
impl SpecKitContext for ChatWidget {
    fn submit_quality_gate_agents(&mut self, checkpoint: QualityCheckpoint) -> Result<()> {
        // Thin delegation to existing ChatWidget methods
        self.submit_operation(Op::SubmitPrompt { /* ... */ })
    }
}

// ✅ CHANGE INTERNAL: handler.rs (update function signatures)
// Before: pub fn on_quality_gate_agents_complete(widget: &mut ChatWidget)
// After:  pub fn on_quality_gate_agents_complete(ctx: &mut impl SpecKitContext)
```

**Rebase Impact:**
- Changed: `context.rs` - **+50 lines** (isolated module)
- Changed: `chatwidget/mod.rs` - **+20 lines delegation** (minimal)
- Changed: `handler.rs` - **Signature changes only** (internal to spec_kit)

**Isolation:** 99.8% (minimal ChatWidget surface expansion)

---

### T83: Configuration Schema Validation ⚠️ HIGH RISK → ✅ MITIGATED

**Problem:** Need to add validation to config loading

**UNSAFE Approach:**
```rust
// ❌ Modify core/src/config.rs (upstream file)
impl Config {
    pub fn load() -> Result<Self> {
        // Add 200 lines of validation logic inline
        validate_json_schema(&toml)?;
        // ...
    }
}
```

**SAFE Approach:**
```rust
// ✅ NEW FILE: tui/src/chatwidget/spec_kit/config_validator.rs (200 lines)
pub struct SpecKitConfigValidator;

impl SpecKitConfigValidator {
    pub fn validate(config: &Config) -> Result<(), Vec<ValidationError>> {
        // All validation logic isolated here
        self.validate_agent_config(&config.agent_config)?;
        self.validate_quality_gates(&config.quality_gates)?;
        Ok(())
    }
}

// ✅ MINIMAL CHANGE: handler.rs (+3 lines)
pub fn handle_spec_auto(widget: &mut ChatWidget, spec_id: String) {
    SpecKitConfigValidator::validate(&widget.config)?;  // +1 line
    // ... existing logic
}

// ✅ ALTERNATIVE: New config.toml section (no code changes)
[spec_kit]
quality_gates_enabled = true
telemetry_hal_enabled = false
# Schema validation via external tool (pre-commit hook)
```

**Rebase Impact:**
- New file: `config_validator.rs` - **Zero conflict**
- Changed: `handler.rs` - **+3 lines** (validation call)
- Changed: `config.toml` - **New section** (zero conflict, fork-specific)
- Unchanged: `core/src/config.rs` - **Zero conflict** (no upstream modifications)

**Isolation:** 100% (validation is opt-in, doesn't touch core)

---

### T84: Typed Error Handling Migration ✅ ZERO RISK

**Problem:** Replace `Result<T, String>` with `SpecKitError`

**UNSAFE Approach:**
```rust
// ❌ N/A - no unsafe approach (internal refactoring only)
```

**SAFE Approach:**
```rust
// ✅ CHANGE INTERNAL: spec_kit/guardrail.rs, consensus.rs
// Before: fn foo() -> Result<T, String>
// After:  fn foo() -> Result<T, SpecKitError>

// All changes internal to spec_kit/ module (already isolated)
```

**Rebase Impact:**
- Changed: `guardrail.rs`, `consensus.rs`, `quality.rs` - **Internal only**
- Unchanged: Any upstream files - **Zero conflict**

**Isolation:** 100% (purely internal refactoring)

---

### T86: Code Hygiene Pass ✅ ZERO RISK

**Problem:** 50 compiler warnings (unused imports, dead code)

**UNSAFE Approach:**
```rust
// ❌ N/A - automated tooling only
```

**SAFE Approach:**
```bash
# ✅ Automated fixes
cargo fix --allow-dirty --allow-staged
cargo clippy --fix --allow-dirty --allow-staged

# Only touches spec_kit/ files (isolated module)
```

**Rebase Impact:**
- Changed: Multiple files in `spec_kit/` - **Internal cleanup**
- Unchanged: Upstream files - **Zero conflict**

**Isolation:** 100% (warnings only in isolated code)

---

### T87: E2E Pipeline Tests ✅ ZERO RISK

**Problem:** Need full `/speckit.auto` integration test

**UNSAFE Approach:**
```rust
// ❌ N/A - tests don't touch production code
```

**SAFE Approach:**
```rust
// ✅ NEW FILE: tui/tests/spec_auto_e2e.rs (500 lines)
#[test]
fn test_full_pipeline_6_stages_3_checkpoints() {
    let mut ctx = MockSpecKitContext::new();
    // ... test logic
}
```

**Rebase Impact:**
- New file: `tui/tests/spec_auto_e2e.rs` - **Zero conflict** (new file)
- Unchanged: All production code - **Zero conflict**

**Isolation:** 100% (tests are separate)

---

### T88: Agent Cancellation Protocol ⚠️ HIGH RISK → ✅ MITIGATED

**Problem:** Need SIGTERM propagation and timeouts

**UNSAFE Approach:**
```rust
// ❌ Modify core agent spawning (upstream code)
impl AgentManager {
    pub fn spawn_agent(&mut self) {
        // Add 100 lines of cancellation logic to upstream
    }
}
```

**SAFE Approach:**
```rust
// ✅ NEW FILE: tui/src/chatwidget/spec_kit/agent_lifecycle.rs (300 lines)
pub struct AgentLifecycleManager {
    agent_pids: HashMap<String, u32>,
    timeouts: HashMap<String, Duration>,
}

impl AgentLifecycleManager {
    pub fn spawn_with_timeout(&mut self, config: &AgentConfig) -> Result<AgentHandle> {
        let child = Command::new(&config.command).spawn()?;
        self.agent_pids.insert(agent_id.clone(), child.id());

        // Set timeout watchdog (separate thread)
        self.start_timeout_watchdog(agent_id, timeout);

        Ok(AgentHandle { agent_id, child })
    }

    pub fn cancel_all(&mut self) {
        for (agent_id, pid) in &self.agent_pids {
            unsafe { libc::kill(*pid as i32, libc::SIGTERM); }
        }
    }
}

// ✅ INTEGRATE: handler.rs (+10 lines)
pub fn handle_spec_auto(widget: &mut ChatWidget, spec_id: String) {
    let lifecycle = AgentLifecycleManager::new();
    // Use lifecycle manager instead of direct spawning
}

// ✅ BETTER: Drop implementation (automatic cleanup, NO app.rs hook)
impl Drop for AgentLifecycleManager {
    fn drop(&mut self) {
        // Automatically cancel all agents when dropped
        self.cancel_all();
    }
}

// ✅ STORE: In SpecAutoState (gets dropped when pipeline completes)
pub struct SpecAutoState {
    // ... existing fields ...
    agent_lifecycle: Option<AgentLifecycleManager>,  // +1 field
}

// When pipeline completes: widget.spec_auto_state = None; → Drop → agents cancelled
```

**Rebase Impact (REVISED - ZERO UPSTREAM CHANGES)**:
- New file: `agent_lifecycle.rs` - **Zero conflict**
- Changed: `state.rs` - **+1 field** (internal to spec_kit)
- Changed: `handler.rs` - **+10 lines** (use new manager)
- Changed: `app.rs` - **ZERO LINES** (Drop handles cleanup automatically)

**Isolation:** 100% (was 99%, now improved to 100%)

---

### T89: MCP Tool Discovery ⚠️ HIGH RISK → ✅ MITIGATED

**Problem:** Need dynamic MCP tool loading

**UNSAFE Approach:**
```rust
// ❌ Modify mcp-client/src/lib.rs (upstream code)
impl McpClient {
    pub fn discover_tools(&mut self) {
        // Add 200 lines to upstream MCP client
    }
}
```

**SAFE Approach:**
```rust
// ✅ NEW FILE: tui/src/chatwidget/spec_kit/mcp_registry.rs (250 lines)
pub struct McpToolRegistry {
    discovered_tools: Vec<ToolDefinition>,
    client: McpClient,  // Use existing, don't modify
}

impl McpToolRegistry {
    pub fn discover_from_directory(&mut self, path: &Path) -> Result<()> {
        // Scan for mcp-server binaries
        // Query each for schema via existing MCP protocol
        // Register in local cache
    }

    pub fn get_tool(&self, name: &str) -> Option<&ToolDefinition> {
        self.discovered_tools.iter().find(|t| t.name == name)
    }
}

// ✅ OPTIONAL: New config section (no code changes to config loading)
# config.toml
[spec_kit.mcp_discovery]
search_paths = ["/usr/local/bin", "~/.code/tools"]
auto_discover = true
```

**Rebase Impact:**
- New file: `mcp_registry.rs` - **Zero conflict**
- New config section: `config.toml` - **Zero conflict** (fork-specific)
- Unchanged: `mcp-client/src/lib.rs` - **Zero conflict** (no modifications)

**Isolation:** 100% (pure addition, uses existing MCP protocol)

---

### T90: Observability Metrics ⚠️ MODERATE RISK → ✅ MITIGATED

**Problem:** Need metrics endpoint for success rates, timing, errors

**UNSAFE Approach:**
```rust
// ❌ Add metrics to core protocol (upstream)
pub struct Op {
    // Add metrics fields to every operation
    pub metrics: Metrics,
}
```

**SAFE Approach:**
```rust
// ✅ NEW FILE: tui/src/chatwidget/spec_kit/metrics.rs (300 lines)
pub struct SpecKitMetrics {
    success_count: AtomicU64,
    failure_count: AtomicU64,
    timing_histogram: Mutex<HashMap<String, Vec<Duration>>>,
}

static METRICS: Lazy<SpecKitMetrics> = Lazy::new(SpecKitMetrics::new);

impl SpecKitMetrics {
    pub fn record_success(&self, stage: SpecStage, duration: Duration) {
        self.success_count.fetch_add(1, Ordering::Relaxed);
        // ...
    }

    pub fn export_json(&self) -> Result<String> {
        // Prometheus/JSON format
    }
}

// ✅ INSTRUMENT: handler.rs (+2 lines per function)
pub fn handle_spec_plan(ctx: &mut impl SpecKitContext, spec_id: String) {
    let start = Instant::now();
    // ... existing logic
    METRICS.record_success(SpecStage::Plan, start.elapsed());  // +1 line
}

// ✅ OPTIONAL: Expose via CLI
// NEW FILE: tui/src/bin/spec-metrics.rs
fn main() {
    println!("{}", SpecKitMetrics::export_json()?);
}
```

**Rebase Impact:**
- New file: `metrics.rs` - **Zero conflict**
- New binary: `spec-metrics.rs` - **Zero conflict**
- Changed: `handler.rs` - **+20 lines** (instrumentation calls)
- Unchanged: Core protocol/types - **Zero conflict** (no upstream changes)

**Isolation:** 99.5% (instrumentation is non-invasive)

---

## Summary Table

| Task | Risk | Mitigation | New Files | Modified Upstream | Modified spec_kit | Isolation |
|------|------|------------|-----------|-------------------|-------------------|-----------|
| T80  | ⚠️ MOD | Orchestrator module | orchestrator.rs | handler.rs (+5) | handler.rs (refactor) | 99.5% |
| T81  | ⚠️ HIGH | Native consensus engine | consensus_native.rs | None | consensus.rs (replace) | 100% |
| T82  | ✅ LOW | Extend trait | None | chatwidget/mod.rs (+20) | context.rs (+50), handler.rs (sigs) | 99.8% |
| T83  | ⚠️ HIGH | Validator module | config_validator.rs | None | handler.rs (+3) | 100% |
| T84  | ✅ ZERO | Internal refactor | None | None | Multiple (internal) | 100% |
| T86  | ✅ ZERO | Automated cleanup | None | None | Multiple (cleanup) | 100% |
| T87  | ✅ ZERO | New test file | spec_auto_e2e.rs | None | None | 100% |
| T88  | ⚠️ HIGH | Lifecycle manager | agent_lifecycle.rs | app.rs (+5) | handler.rs (+10) | 99% |
| T89  | ⚠️ HIGH | Registry module | mcp_registry.rs | None | None | 100% |
| T90  | ⚠️ MOD | Metrics module | metrics.rs | None | handler.rs (+20) | 99.5% |

**Overall Isolation After All Tasks:** **99.6%** (maintained from 98.8%)

---

## Rebase Protocol Compliance

### Pre-Implementation Checklist

For EVERY task, before writing code:
- [ ] Read this matrix for the task
- [ ] Follow SAFE approach (never UNSAFE)
- [ ] Create new files in `spec_kit/` (not inline code)
- [ ] Minimize upstream file changes (<10 lines per file)
- [ ] Add tests in `tui/tests/` (separate files)
- [ ] Document in `docs/spec-kit/` (zero conflict)

### Post-Implementation Validation

After completing each task:
```bash
# 1. Verify isolation maintained
bash scripts/fork_maintenance/validate_rebase.sh

# 2. Check upstream file changes
git diff main --stat | grep -E "chatwidget/mod.rs|app.rs|slash_command.rs"
# Should show minimal changes (<50 lines total)

# 3. Verify new files are isolated
git diff main --name-only | grep "spec_kit/"
# Should show only spec_kit/ changes

# 4. Test compilation
cargo build --all-features

# 5. Run tests
cargo test --package codex-tui
```

---

## Emergency Rebase Test

Before merging any task, simulate rebase:
```bash
# 1. Create test branch
git checkout -b test-rebase-t<NUM>

# 2. Fetch upstream
git fetch upstream master

# 3. Attempt rebase
git rebase upstream/master

# 4. Count conflicts
git status | grep "both modified"
# Should be <5 files, <200 lines total

# 5. If clean, approve task
git rebase --abort
git checkout main
```

---

## Long-Term Maintenance

**Quarterly Review:**
- Re-run isolation audit
- Update this matrix with new patterns discovered
- Verify actual rebase conflicts match predictions
- Adjust strategies if patterns change

**Incident Response:**
If any task causes >500 lines of rebase conflicts:
1. **STOP** - Do not merge
2. **ANALYZE** - What went wrong?
3. **REFACTOR** - Apply stricter isolation
4. **UPDATE** - This matrix with lessons learned
5. **RE-IMPLEMENT** - Following updated guidance

---

**Document Owner:** Architecture Team
**Last Updated:** 2025-10-17
**Status:** Active guidance for T80-T90 implementation
