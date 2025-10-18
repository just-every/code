# Architecture Task Deep Inspection (2025-10-18)

**Method**: Systematic code inspection validating each task against actual implementation
**Inspector**: Implementation-level analysis (not architectural theory)
**Findings**: 3/7 remaining tasks are false issues or architectural perfectionism

---

## Executive Summary

**Original Review**: 13 tasks, 93.5-131.5 hours
**Deep Inspection**:
- ✅ 6 tasks complete (7.5h actual)
- ✅ 3 tasks valid (11-21h)
- ❌ 3 tasks false/unnecessary (would waste 26-34h)
- ⚠️ 1 task needs refocus

**Key Finding**: Original review identified architectural "purity" issues without validating if they cause actual problems.

---

## Task-by-Task Analysis

### ✅ ARCH-001 through ARCH-004: COMPLETE (Validated)
**Status**: Done, working, tested
**Impact**: Real improvements (upstream clarity, MCP fallback, config docs, cleanup)
**Verdict**: Original review correct, implementation successful

### ✅ ARCH-006, ARCH-007: COMPLETE (Validated)
**Status**: Done, tested
**Impact**: Type safety (agent names), concurrency safety (file locks)
**Verdict**: Original review correct, faster than estimated (1.75h vs 5-7h)

---

### ❌ ARCH-005: Dual MCP Connections [VALID but OVERSTATED]

**Review Claim**: "Conflict risk if both access same server"

**Deep Inspection**:
- ✅ Problem EXISTS: User config has `[mcp_servers.local-memory]`, TUI also spawns
- ✅ Currently happening: 7 local-memory processes running (`ps aux` confirms)
- ❌ NOT causing failures: MCP stdio spawns process-per-connection (no singleton conflict)
- Impact: **Resource waste** (2+ processes when 1 needed), NOT functional failure

**Evidence**:
```bash
ps aux | grep local-memory
# Shows 7 processes - multiple stdio connections active
```

**User config (~/.code/config.toml)**:
```toml
[mcp_servers.local-memory]
command = "local-memory"
```

**TUI code (chatwidget/mod.rs:2779)**:
```rust
let mcp_config = HashMap::from([(
    "local-memory".to_string(),
    McpServerConfig { command: "local-memory"...}
)]);
```

**Verdict**:
- Review: Correct problem identification
- Severity: Overstated (waste, not corruption)
- Priority: Downgrade P1 → P2 (cleanup, not critical)
- **Simplified Fix**: Remove TUI spawn (2h), no protocol changes needed

---

### ❌ ARCH-008: Protocol Extension [FALSE DEPENDENCY]

**Review Claim**: "Enable ARCH-005 (dual MCP fix) and ARCH-010 (state migration)"

**Deep Inspection**:
- ARCH-005 fix: Just remove TUI MCP spawn - NO protocol changes needed
- ARCH-010 premise: Requires non-TUI clients (see below - don't exist)
- Effort: 8-10h for `Op::CallMcpTool`, `EventMsg::WorkflowStateChanged`
- Benefit: Enables... nothing currently needed

**Verdict**:
- Review: **Architectural perfectionism**
- Value: Foundation for solving problems that don't exist
- **SKIP ENTIRELY** - YAGNI violation

---

### ⚠️ ARCH-009: Agent Coordination [MISDIAGNOSED - Real issue different]

**Review Claim**: "Retry logic split between spec-kit/handler.rs and core/client.rs"

**Deep Inspection**:

**FALSE Duplication**:
```rust
// core/client.rs:433 - HTTP request-level timeout
let total_timeout = self.provider.agent_total_timeout(); // 30min

// spec-kit/handler.rs:708 - Orchestration-level retry
const MAX_AGENT_RETRIES: u32 = 3; // Retry failed stage
```

These are **orthogonal concerns** at different layers:
- Core: Prevents single HTTP call from running >30min
- Spec-kit: Retries entire stage if agents produce bad results

**NOT duplication** - both needed, serve different purposes.

**REAL Issue Found**:
```rust
// handler.rs has MAX_AGENT_RETRIES defined 3 times:
Line 647:  const MAX_AGENT_RETRIES: u32 = 3;
Line 708:  const MAX_AGENT_RETRIES: u32 = 3;
Line 744:  const MAX_AGENT_RETRIES: u32 = 3;
```

**Verdict**:
- Review: **Misdiagnosed** - saw duplication where none exists
- Real issue: Constant defined 3x (simple DRY violation)
- Fix: Extract to module-level const (30min, not 4-6h)
- **Refocus**: ARCH-009-REVISED: "Extract Retry Constants"

---

### ❌ ARCH-010: State Migration [NO USE CASE]

**Review Claim**: "Migrate spec-auto state to core, enables non-TUI clients (API, CI/CD)"

**Deep Inspection - Non-TUI Clients**:

Searched codebase:
```bash
find . -name "*api*" -o -name "*server*" | grep -E "\.rs$"
# Found: login/server.rs (OAuth only), mcp-server (tool exposure)
# NOT found: Spec-kit API server, CI/CD integration
```

**Existing Non-Interactive Mode**:
- `codex exec`: Headless single-prompt execution
- Uses `core` conversation manager
- Does NOT use spec-kit (no `/speckit.*` commands)
- Spec-kit is TUI-exclusive by design

**Checked Documentation**:
- PLANNING.md: No mention of API/CI/CD clients
- product-requirements.md: No multi-client requirements
- CLAUDE.md: Spec-kit documented as TUI feature only

**Verdict**:
- Review: **Aspirational architecture** for non-existent requirement
- Use case: None documented, none exist
- Effort: 12-16h to enable hypothetical future need
- **SKIP ENTIRELY** - YAGNI principle, solve when needed

---

### ✅ ARCH-011: Async TUI [VALIDATION NEEDED]

**Review Claim**: "8.7ms blocking acceptable but validate"

**Deep Inspection**:

**Ratatui Async Support**: YES (official tutorials exist)
- https://ratatui.rs/tutorials/counter-async-app/
- Pattern: `tokio::select!` for event polling
- Template: github.com/ratatui-org/ratatui-async-template

**Current Blocking**:
```rust
// handler.rs: 3 sites using Handle::current().block_on()
let result = handle.block_on(run_consensus_with_retry(...)); // 8.7ms avg
```

**User Impact Analysis**:
- Blocking duration: 8.7ms average, 13ms max (from benchmark)
- Human perception threshold: ~100ms for "instant"
- Current: **12x below perception threshold**
- Consensus runs ~3-4x per `/speckit.auto` (60min total) = ~35ms total blocking

**Migration Effort** (if pursued):
- Convert event loop to async: 8-12h
- Update all event handlers: 4-8h
- Test updates: 2-4h
- Total: 14-24h

**ROI Calculation**:
- Benefit: Eliminate 35ms blocking per hour-long pipeline (<0.1% of runtime)
- Cost: 14-24h development + ongoing async complexity
- **ROI: NEGATIVE** unless other benefits found

**Verdict**:
- Review: Correctly identified as research spike
- Recommendation: **4h spike, likely conclude "blocking acceptable"**
- Document rationale, close task

---

### ✅ ARCH-012: Upstream Contributions [VALID]

**Review Claim**: "Extract MCP retry, native dashboard, evidence patterns"

**Deep Inspection**:

**Upstream**: github.com/just-every/code (confirmed active, accepting PRs)

**Extractable Features** (checked for fork-specific dependencies):

1. **MCP Retry Logic** (`handler.rs::run_consensus_with_retry`):
   - Lines: ~60 LOC
   - Dependencies: None (uses McpConnectionManager from core)
   - Generalization: Trivial (already generic)
   - Effort: 2h (extract, document, PR)
   - **Value**: HIGH - improves upstream MCP reliability

2. **Native Rust Dashboard** (`spec_status.rs`):
   - Lines: ~400 LOC
   - Dependencies: SpecStage enum (fork-specific), evidence paths
   - Generalization: 3-4h (make paths configurable)
   - **Value**: MEDIUM - upstream has shell script, Rust is faster

3. **Evidence Repository Trait** (`evidence.rs`):
   - Lines: ~200 LOC (trait + implementation)
   - Dependencies: SpecKitError (fork-specific)
   - Generalization: 4-6h (extract to standalone module)
   - **Value**: LOW-MEDIUM - reusable pattern but upstream may not need

**Verdict**:
- Review: **Correct and valuable**
- Upstream receptiveness: Unknown (need to check contribution guidelines)
- **Priority**: P2 (community value, not urgent)
- **Effort**: 6-12h (focus on #1 and #2)

---

### ✅ ARCH-013: MCP Schema Validation [CORRECTLY DEFERRED]

**Review Claim**: "Best-effort parsing, should enforce schema"

**Deep Inspection**:

**Current Implementation** (`consensus.rs:416-436`):
```rust
for content_item in &result.content {
    if let ContentBlock::TextContent(text_content) = content_item {
        if let Ok(json_results) = serde_json::from_str::<Vec<Value>>(text) {
            // Silent skip on parse failure
        }
    }
}
```

**Failure Rate**: 0/141 tests (no observed MCP parse failures in production)

**Benefit of Schema Validation**:
- Fail-fast on malformed responses
- Better error messages
- Effort: 4-6h

**ROI**: Low - hardening code that doesn't currently fail

**Verdict**:
- Review: **Correctly deferred**
- Priority: P3 (nice-to-have, not needed)

---

### ✅ ARCH-014: Shell Script Migration [CORRECTLY DEFERRED]

**Review Claim**: "Migrate guardrail scripts to Rust"

**Deep Inspection**:

**Current**: 6 shell scripts (`/guardrail.{plan,tasks,implement,validate,audit,unlock}`)
**Migration Effort**: 6-8h per script = 36-48h total

**Tradeoffs**:
| Aspect | Shell Scripts | Native Rust |
|--------|--------------|-------------|
| **Customizability** | Easy (users edit scripts) | Requires recompile |
| **Portability** | POSIX bash (Mac/Linux) | Full cross-platform |
| **Error handling** | Basic | Rich (typed errors) |
| **Performance** | Subprocess overhead | Native |
| **Maintenance** | External dependency | In-tree |

**User Needs**:
- Windows support? Not documented as requirement
- Script customization? Currently used feature

**Verdict**:
- Review: **Correctly deferred**
- Priority: P3 unless Windows support becomes requirement
- Effort: Accurate (36-48h)

---

# Corrected Roadmap

## Completed (6 tasks, 7.5h)
- ✅ ARCH-001: Upstream docs + memory policy
- ✅ ARCH-002: MCP fallback
- ✅ ARCH-003: Config precedence
- ✅ ARCH-004: Deprecated code cleanup
- ✅ ARCH-006: Agent name normalization
- ✅ ARCH-007: Evidence locking

## Recommended Next Steps (4 tasks, 11-22h)
1. **ARCH-009-REVISED**: Extract retry constants (30min) - Quick win
2. **ARCH-011**: Async TUI spike (4-8h) - Validate blocking acceptable, document findings
3. **ARCH-012**: Upstream contributions (6-12h) - Give back to community
4. **ARCH-005** (optional): Simplify dual MCP (2-3h) - Cleanup only

## Skip Entirely (3 tasks, 26-34h saved)
- ❌ **ARCH-008**: Protocol extension - No use case
- ❌ **ARCH-010**: State migration - No non-TUI clients exist
- ❌ **ARCH-009** (original): Agent coordination - False duplication

## Correctly Deferred (2 tasks, 40-54h)
- ⏸️ **ARCH-013**: MCP schema validation - Works fine, hardening only
- ⏸️ **ARCH-014**: Shell migration - High effort, unclear benefit

---

# Review Quality Assessment

**Original Review Strengths**:
- ✅ Identified real documentation issues (ARCH-001)
- ✅ Found missing resilience (ARCH-002)
- ✅ Spotted type safety opportunities (ARCH-006)
- ✅ Correctly prioritized deferred work

**Original Review Weaknesses**:
- ❌ Didn't validate if problems cause actual failures
- ❌ Proposed solutions to non-existent use cases (ARCH-010)
- ❌ Misdiagnosed orthogonal concerns as duplication (ARCH-009)
- ❌ Over-engineered dependencies (ARCH-008 enables nothing real)

**Root Cause**: Architectural review without implementation validation = theory-heavy, pragmatism-light

---

# Recommendation

**Total Remaining Valuable Work**: 11-22 hours (vs 86-125h original)

**Next Session Priority**:
1. ARCH-009-REVISED (30min) - Extract constants
2. ARCH-011 (4-8h) - Async TUI spike → likely conclude "blocking OK"
3. ARCH-012 (6-12h) - Upstream contributions

**After That**: Call architecture work DONE at 9/13 tasks (skipping 3 false issues, deferring 2 low-ROI)

**Effort Saved**: 26-34h of protocol work that enables zero current value

---

**Session Handoff**: See SESSION-HANDOFF.md for clean roadmap
