# Spec-Kit Automation Agents - codex-rs

**Project**: codex-rs (theturtlecsz/code)
**Last Updated**: 2025-10-18 (Post-ARCH improvements)
**Architecture Status**: Production Ready (Phase 3 Complete)

---

## 📋 PROJECT CONTEXT

**This Repository**: https://github.com/theturtlecsz/code (FORK)
**Upstream**: https://github.com/just-every/code (community fork of OpenAI Codex)
**Origin**: OpenAI Codex CLI (community-maintained)

**NOT RELATED TO**: Anthropic's Claude Code (different product entirely)

**Fork-Specific Features**:
- **Spec-Kit Automation**: Multi-agent PRD workflows (Plan→Tasks→Implement→Validate→Audit→Unlock)
- **Consensus Synthesis**: Multi-model result aggregation via local-memory MCP
- **Quality Gates**: Automated requirement validation framework
- **Native MCP Integration**: 5.3x faster consensus checks (measured vs subprocess baseline)

---

## 🎯 MEMORY SYSTEM POLICY

### MANDATORY: Local-Memory MCP Only

**Policy Effective**: 2025-10-18

**Use**:
- ✅ **local-memory MCP** - ONLY memory system for knowledge persistence
- ✅ Query before tasks, store during work (importance ≥7), persist outcomes

**Do NOT Use**:
- ❌ byterover-mcp (deprecated, migration complete 2025-10-18)
- ❌ Any other memory MCP servers

**Rationale**:
1. Native MCP integration validated (5.3x faster than subprocess, ARCH-002)
2. Spec-kit consensus framework requires local-memory
3. Single source of truth eliminates conflicts
4. 178 passing tests validate reliability (135 unit + 19 integration + 21 E2E + 3 MCP)

**Detailed Policy**: See `codex-rs/MEMORY-POLICY.md`

---

## 🤖 SPEC-KIT AGENTS (Multi-Model Consensus)

### Agent Roster

These are **AI models**, not agent tools. They work in parallel to provide multi-perspective analysis.

| Agent | Model | Role | Used In | Type-Safe |
|-------|-------|------|---------|-----------|
| **gemini** | gemini-2.0-flash-thinking-exp | Research, broad analysis, exploratory implementation | All stages | `SpecAgent::Gemini` |
| **claude** | claude-sonnet-4-5 | Detailed reasoning, edge cases, implementation | All stages | `SpecAgent::Claude` |
| **code** | claude-sonnet-4-5 (Claude Code CLI) | General-purpose, orchestration | All stages | `SpecAgent::Code` |
| **gpt_codex** | gpt-5-codex | Code generation specialist | Implement stage only | `SpecAgent::GptCodex` |
| **gpt_pro** | gpt-5 | **Synthesis & aggregation** (authoritative consensus) | Dev stages | `SpecAgent::GptPro` |

**Key Distinctions**:
- `gpt_pro` is the **aggregator**—synthesizes other agents' outputs with `agreements[]` and `conflicts[]`
- `code` agent added in ARCH-009 (type-safe enum migration)
- All agents normalized via `SpecAgent::from_string()` (ARCH-006)

---

## 🎚️ MULTI-AGENT TIERS

### Tier 0: Native TUI (0 agents, $0, <1s)
**Command**: `/speckit.status SPEC-ID`
**Purpose**: Pure Rust dashboard, no AI needed
**Implementation**: `codex-rs/tui/src/spec_status.rs`

### Tier 2-lite: Dual Agent (2 agents, ~$0.35, 5-8 min)
**Agents**: claude + code
**Command**: `/speckit.checklist SPEC-ID`
**Purpose**: Quality evaluation without research overhead

### Tier 2: Triple Agent (3 agents, ~$0.80-1.00, 8-12 min)
**Agents**: gemini + claude + gpt_pro (or code for simpler stages)
**Commands**:
- `/speckit.new`: Create SPEC
- `/speckit.specify`: Draft/update PRD
- `/speckit.clarify`: Ambiguity resolution
- `/speckit.analyze`: Consistency checking
- `/speckit.plan`: Work breakdown
- `/speckit.tasks`: Task decomposition
- `/speckit.validate`: Test strategy
- `/speckit.audit`: Compliance checking
- `/speckit.unlock`: Final approval

**Use For**: Analysis, planning, consensus (no code generation)

### Tier 3: Quad Agent (4 agents, ~$2.00, 15-20 min)
**Agents**: gemini + claude + gpt_codex + gpt_pro
**Command**: `/speckit.implement SPEC-ID`
**Purpose**: Code generation with multiple implementation approaches + synthesis

### Tier 4: Dynamic (3-5 agents adaptively, ~$11, 60 min)
**Command**: `/speckit.auto SPEC-ID`
**Behavior**:
- Uses Tier 2 for most stages (plan, tasks, validate, audit, unlock)
- Uses Tier 3 for implement stage
- Adds arbiter agent if conflicts detected
- Handles degradation (continues with 2/3 agents if one fails)

---

## 📋 CONSENSUS WORKFLOW

### How Multi-Agent Consensus Works

**Step 1: Agent Execution** (parallel)
```
/speckit.plan SPEC-KIT-065
  → Spawns 3 agents simultaneously
     - gemini analyzes requirements
     - claude identifies edge cases
     - gpt_pro synthesizes and provides consensus
```

**Step 2: Local-Memory Storage** (each agent)
```rust
// Each agent stores via local-memory MCP
{
  "agent": "claude",
  "stage": "plan",
  "spec_id": "SPEC-KIT-065",
  "prompt_version": "20251002-plan-a",
  "analysis": {
    "work_breakdown": [...],
    "risks": [...]
  }
}
```

**Tags**: `spec:SPEC-KIT-065`, `stage:plan`, `consensus-artifact`
**Importance**: 8

**Step 3: Consensus Synthesis** (automatic)
```
check_consensus_and_advance_spec_auto()
  → fetch_memory_entries() via native MCP (8.7ms)
     → Validates all agents stored results
     → Extracts gpt_pro's consensus section
     → Checks for:
        - Missing agents (degraded if <100%)
        - Conflicts (from gpt_pro.consensus.conflicts)
        - Required fields (agent, stage, spec_id)
```

**Step 4: Verdict Persistence**
```json
// Stored to filesystem + local-memory
{
  "consensus_ok": true,
  "degraded": false,
  "missing_agents": [],
  "agreements": ["All agents agree on 3-phase implementation"],
  "conflicts": [],
  "aggregator_agent": "gpt_pro",
  "artifacts": [...]
}
```

**Step 5: Advance or Retry**
- If consensus OK → Advance to next stage
- If degraded/conflict → Retry (max 3x) or escalate to human
- If empty results → Auto-retry with enhanced prompt context

---

## 🔄 RETRY & RECOVERY LOGIC

### Agent Execution Retries
**Trigger**: Empty results, invalid JSON, or explicit failure
**Max Attempts**: 3
**Backoff**: 100ms → 200ms → 400ms (exponential)
**Location**: `codex-rs/tui/src/chatwidget/spec_kit/handler.rs`

**Enhanced Context on Retry**:
```
state.agent_retry_context = Some(format!(
  "Previous attempt returned invalid/empty results (retry {}/3).
   Store ALL analysis in local-memory with remember command.",
  retry_count + 1
));
```

### MCP Connection Retries
**Trigger**: "MCP manager not initialized yet"
**Max Attempts**: 3
**Backoff**: 100ms → 200ms → 400ms
**Location**: `handler.rs::run_consensus_with_retry()`

**Purpose**: Handles race condition between MCP async initialization and consensus checks

### Validation Stage Retries
**Trigger**: Validation failures (tests don't pass)
**Max Attempts**: 2 (inserts `Implement → Validate` cycle)
**Location**: `handler.rs::on_spec_auto_task_complete()`

---

## 📊 PERFORMANCE METRICS

### Measured Latencies (Debug Build, 2025-10-18)

| Operation | Latency | Notes |
|-----------|---------|-------|
| **MCP Consensus Check** | 8.7ms avg | 5.3x faster than subprocess (46ms) |
| **MCP Connection Init** | ~150ms | 5-second timeout, only once per session |
| **Single Agent Execution** | 30-120s | Model-dependent, includes thinking time |
| **Tier 2 Stage** | 8-12 min | 3 agents parallel |
| **Tier 3 Stage** | 15-20 min | 4 agents parallel |
| **Full Pipeline** | ~60 min | 6 stages, adaptive tiering |

**Benchmark Source**: `codex-rs/tui/tests/mcp_consensus_benchmark.rs`

---

## 🏗️ TECHNICAL ARCHITECTURE

### Consensus Implementation
**File**: `codex-rs/tui/src/chatwidget/spec_kit/consensus.rs` (992 LOC)

**Key Functions**:
```rust
// Main entry point
pub async fn run_spec_consensus(
  cwd: &Path,
  spec_id: &str,
  stage: SpecStage,
  telemetry_enabled: bool,
  mcp_manager: &McpConnectionManager,
) -> Result<(Vec<Line>, bool)>

// MCP search with native protocol
async fn fetch_memory_entries(...) -> Result<Vec<LocalMemorySearchResult>>

// MCP storage with retry
async fn remember_consensus_verdict(...) -> Result<()>

// Parse MCP response (TextContent → JSON)
fn parse_mcp_search_results(result: &CallToolResult) -> Result<Vec<...>>
```

**MCP Tool Calls**:
- Search: `mcp_manager.call_tool("local-memory", "search", args, timeout)`
- Store: `mcp_manager.call_tool("local-memory", "store_memory", args, timeout)`
- Timeout: 30s for search, 10s for store

### State Machine
**File**: `codex-rs/tui/src/chatwidget/spec_kit/state.rs` (414 LOC)

```rust
pub enum SpecAutoPhase {
  Guardrail,                        // Shell script validation
  ExecutingAgents { ... },          // Parallel agent execution
  CheckingConsensus,                // MCP fetch + synthesis
  QualityGateExecuting { ... },     // Optional quality validation
  QualityGateProcessing { ... },    // Issue classification
  QualityGateValidating { ... },    // GPT-5 verification
  QualityGateAwaitingHuman { ... }, // Human escalation
}
```

**State Transitions**:
```
Guardrail → ExecutingAgents → CheckingConsensus → [Next Stage or Retry]
                                     ↓ (if quality gates enabled)
                               QualityGateExecuting → ... → Next Stage
```

### Evidence Repository
**File**: `codex-rs/tui/src/chatwidget/spec_kit/evidence.rs` (499 LOC)

**Filesystem Structure**:
```
docs/SPEC-OPS-004-integrated-coder-hooks/evidence/
├── consensus/
│   └── SPEC-ID/
│       ├── plan_20251018T120000Z_verdict.json
│       └── plan_20251018T120000Z_synthesis.json
└── commands/
    └── SPEC-ID/
        ├── plan_20251018T120000Z_telemetry.json
        └── plan_20251018T120000Z_gemini_artifact.json
```

**Telemetry Schema v1**:
```json
{
  "command": "/speckit.plan",
  "specId": "SPEC-KIT-065",
  "sessionId": "...",
  "schemaVersion": 1,
  "timestamp": "2025-10-18T12:00:00Z",
  "artifacts": [...],
  "baseline": { "mode": "native", "status": "ok" }
}
```

---

## 📚 DOCUMENTATION REFERENCE

**Core Documentation** (codex-rs workspace):
- `CLAUDE.md`: Operational playbook (how to work in this repo)
- `MEMORY-POLICY.md`: Memory system policy (local-memory only)
- `REVIEW.md`: Architecture analysis (comprehensive, 2025-10-18)
- `ARCHITECTURE-TASKS.md`: Improvement tasks (13 tasks, 7 complete)
- `SPEC.md`: Task tracker (single source of truth)
- This file: Spec-kit agent reference

**Policy Documents** (created 2025-10-18):
- `docs/spec-kit/evidence-policy.md`: Evidence growth management (25 MB soft limit, retention, archival)
- `docs/spec-kit/testing-policy.md`: Test coverage roadmap (1.7%→40% by Q1 2026)
- `docs/UPSTREAM-SYNC.md`: Upstream merge strategy (monthly/quarterly, conflict resolution)
- `docs/architecture/async-sync-boundaries.md`: Async/sync design (Ratatui+Tokio architecture)

**Spec-Kit Implementation Docs**:
- `docs/spec-kit/prompts.json`: Agent prompt templates (embedded at compile time)
- `docs/spec-kit/model-strategy.md`: Model selection rules
- `docs/spec-kit/spec-auto-automation.md`: Pipeline details
- `docs/spec-kit/evidence-baseline.md`: Telemetry expectations

**Spec-Kit Module Breakdown** (7,883 LOC total):
- Handler: `tui/src/chatwidget/spec_kit/handler.rs` (2,038 LOC - orchestration)
- Consensus: `tui/src/chatwidget/spec_kit/consensus.rs` (992 LOC - MCP native)
- Quality: `tui/src/chatwidget/spec_kit/quality.rs` (807 LOC - gates)
- Guardrail: `tui/src/chatwidget/spec_kit/guardrail.rs` (589 LOC - validation)
- Evidence: `tui/src/chatwidget/spec_kit/evidence.rs` (499 LOC - persistence)
- + 9 more modules (state, schemas, error, context, etc.)

---

## 🚀 QUICK START GUIDE

### Run Full Automation
```bash
# Create SPEC
/speckit.new Add user authentication with OAuth2 and JWT

# Auto-run all 6 stages
/speckit.auto SPEC-KIT-###

# Monitor progress
/speckit.status SPEC-KIT-###
```

### Manual Stage-by-Stage
```bash
/speckit.plan SPEC-KIT-065       # ~10 min, $1.00
/speckit.tasks SPEC-KIT-065      # ~10 min, $1.00
/speckit.implement SPEC-KIT-065  # ~18 min, $2.00 (4 agents)
/speckit.validate SPEC-KIT-065   # ~10 min, $1.00
/speckit.audit SPEC-KIT-065      # ~10 min, $1.00
/speckit.unlock SPEC-KIT-065     # ~10 min, $1.00
```

### Debugging Commands
```bash
# Check consensus status
/spec-consensus SPEC-KIT-065 plan

# Monitor evidence size
/spec-evidence-stats --spec SPEC-KIT-065

# Check local-memory artifacts
local-memory search "SPEC-KIT-065 stage:plan" --limit 20
```

---

## ⚙️ AGENT CONFIGURATION

### Prompt Versioning
**Location**: `docs/spec-kit/prompts.json`

```json
{
  "plan": {
    "version": "20251002-plan-a",
    "gemini": { "role": "researcher", "prompt": "..." },
    "claude": { "role": "analyst", "prompt": "..." },
    "gpt_pro": { "role": "synthesizer", "prompt": "..." }
  }
}
```

**Version Format**: `YYYYMMDD-{stage}-{revision}`
**Embedded**: Compiled into binary via `include_str!()` macro

### Model Selection Defaults

| Agent | Default Model | Fallback | Reasoning Mode |
|-------|---------------|----------|----------------|
| **gemini** | gemini-2.0-flash-thinking-exp-01-21 | gemini-2.0-flash-exp | high |
| **claude** | claude-sonnet-4-20250514 | claude-sonnet-4 | high |
| **gpt_codex** | gpt-5-codex | gpt-5 | high |
| **gpt_pro** | gpt-5 | gpt-5-codex | high |

**Metadata Resolution**: Prompts can override with `${MODEL_ID}`, `${MODEL_RELEASE}`, `${REASONING_MODE}` placeholders

---

## 🔄 CONSENSUS ALGORITHM

### Classification Rules

**Consensus OK** (advance to next stage):
- ✅ All required agents present (gemini, claude, gpt_pro)
- ✅ gpt_pro provides aggregator summary
- ✅ No conflicts in gpt_pro.consensus.conflicts
- ✅ Required fields validated (agent, stage, spec_id, prompt_version)

**Consensus Degraded** (continue with warning):
- ⚠️ One agent missing (2/3 participation)
- ✅ No conflicts
- ⚠️ Warning logged, but consensus accepted

**Consensus Conflict** (retry or escalate):
- ❌ gpt_pro.consensus.conflicts non-empty
- ❌ Manual resolution required
- Action: Review synthesis file, resolve conflicts, re-run stage

**No Consensus** (retry):
- ❌ <50% agent participation
- ❌ No gpt_pro aggregator
- Action: Retry stage (max 3x)

### Retry Strategy

**Empty/Invalid Results Detection** (regex patterns):
```rust
let results_empty_or_invalid = consensus_lines.iter().any(|line| {
  let text = line.to_string();
  text.contains("No structured local-memory entries") ||
  text.contains("No consensus artifacts") ||
  text.contains("Missing agent artifacts") ||
  text.contains("No local-memory entries found")
});
```

**Retry Logic**:
```
Attempt 1: Normal prompt
Attempt 2: + "Previous attempt failed, ensure you use local-memory remember"
Attempt 3: + Enhanced retry context
Fail: Halt pipeline, human intervention required
```

---

## 🧪 TESTING & VALIDATION

**Test Coverage**: 178 passing (135 unit, 19 integration, 21 E2E, 3 MCP)

**Integration Tests**:
1. **quality_gates_integration.rs** (19 tests):
   - Checkpoint execution, agent JSON parsing
   - Unanimous auto-resolution (High confidence)
   - 2/3 majority validation flow with GPT-5
   - No-consensus escalation, edge cases

2. **spec_auto_e2e.rs** (21 tests):
   - Full pipeline state machine
   - Stage progression and advancement
   - Checkpoint integration
   - Error recovery and retry logic

3. **mcp_consensus_integration.rs** (3 tests):
   - MCP connection initialization (validates 11 local-memory tools)
   - Tool call format validation (search/store succeed)
   - Retry logic for delayed initialization

**Benchmark Tests** (run with `--ignored`):
- **mcp_consensus_benchmark.rs**: Validates 5.3x speedup (46ms → 8.7ms)

---

## ⚠️ KNOWN LIMITATIONS & FUTURE WORK

**Architectural Constraints**:
1. **Spec-kit embedded in TUI**
   - 7,883 LOC in `tui/src/chatwidget/spec_kit/` (should be separate crate)
   - Makes CLI/API usage impossible
   - Future: Extract to `codex-spec-kit` crate (2-4 week effort, deferred)

2. **Async/Sync Boundary**
   - TUI event loop blocks during MCP calls (8.7ms typical, 700ms cold-start)
   - Ratatui is sync, Tokio is async, bridged via `Handle::block_on()`
   - Acceptable for infrequent user-initiated commands
   - See `docs/architecture/async-sync-boundaries.md`

3. **Test Coverage Gap**
   - Current: 1.7% (178 tests / 7,883 LOC)
   - Target: 40% by Q1 2026
   - See `docs/spec-kit/testing-policy.md`

**Resolved via ARCH Improvements** (Oct 2025):
- ✅ MCP fallback (ARCH-002): File-based evidence if MCP unavailable
- ✅ MCP process multiplication (ARCH-005): App-level shared manager
- ✅ Config precedence (ARCH-003): Documented 5-layer hierarchy
- ✅ Agent enum safety (ARCH-006): Type-safe `SpecAgent` enum
- ✅ Evidence corruption (ARCH-007): File locking via fs2

---

## 🔍 DEBUGGING GUIDE

### Common Issues

**1. "MCP manager not initialized yet"**
```
Cause: Consensus ran before MCP connected (async race condition)
Solution: Retry logic auto-handles (3 attempts, 100-400ms backoff)
Verify: Check local-memory running: `local-memory --version`
```

**2. "No consensus artifacts found"**
```
Cause: Agents didn't store to local-memory
Check: /spec-evidence-stats --spec SPEC-ID
Check: local-memory search "SPEC-ID stage:plan"
Fallback: Inspect docs/SPEC-OPS-004.../evidence/*.json
```

**3. "Consensus degraded: missing agents"**
```
Cause: One or more agents failed/timed out
Check: TUI history for agent error messages
Action: Retry stage OR accept degraded consensus
Context: 2/3 agents still valid for degraded mode
```

**4. "Evidence footprint exceeds 25MB"**
```
Check: /spec-evidence-stats
Action: Archive old SPECs, propose offloading strategy
Limit: Soft limit per SPEC (not enforced, monitored)
```

**5. "Validation retry cycle"**
```
Cause: Tests failed after implement
Behavior: Auto-inserts "Implement → Validate" cycle (max 2 retries)
Check: TUI shows "Retrying implementation/validation cycle (attempt N)"
```

---

## 📈 ARCHITECTURE STATUS

See `codex-rs/ARCHITECTURE-TASKS.md` and `codex-rs/REVIEW.md` for full details.

**Completed** (Oct 17-18, 2025):
- ✅ ARCH-001: Fixed upstream docs (just-every/code fork)
- ✅ ARCH-002: MCP fallback + native integration (5.3x faster)
- ✅ ARCH-003: Config precedence documented
- ✅ ARCH-004: Removed deprecated subprocess code
- ✅ ARCH-005: Fixed MCP process multiplication (App-level manager)
- ✅ ARCH-006: Type-safe agent enums (`SpecAgent`)
- ✅ ARCH-007: Evidence file locking (prevents corruption)
- ✅ ARCH-009-REVISED: Extracted retry constants
- ✅ AR-1 through AR-4: Agent resilience (timeout, retry, empty detection, schemas)

**Skipped** (validated as unnecessary):
- ❌ ARCH-008: Protocol extension (YAGNI)
- ❌ ARCH-010: State migration (no non-TUI clients exist)

**Future Considerations** (not prioritized):
- ARCH-011: Async TUI research spike (low ROI for 8.7ms blocking)
- ARCH-012: Upstream contributions (if valuable fixes emerge)
- Spec-kit extraction to separate crate (if reusability need arises)

---

**Maintainer**: theturtlecsz
**Repository**: https://github.com/theturtlecsz/code
**Last Verified**: 2025-10-18
