# Memory System Policy

**Effective Date**: 2025-10-18
**Status**: MANDATORY

---

## Single Memory System: local-memory MCP

**Policy**: Use **local-memory MCP** exclusively for all knowledge persistence and retrieval.

**Deprecated**:
- ~~byterover-mcp~~ (DO NOT USE)
- ~~Any other memory MCP servers~~

---

## Rationale

**Why local-memory only**:
1. **Native MCP Integration**: Validated 5.3x faster than subprocess baseline
2. **Spec-Kit Dependency**: Consensus framework requires local-memory for multi-agent synthesis
3. **Single Source of Truth**: Eliminates memory conflicts and divergence
4. **Tested & Reliable**: 141 passing tests validate MCP integration path

**Why not byterover**:
- Adds unnecessary complexity
- Potential for memory conflicts between systems
- Not integrated with spec-kit automation
- Unclear sync/merge semantics

---

## Usage Guidelines

### Store Knowledge
```bash
# Via MCP tool (in code)
mcp_manager.call_tool("local-memory", "store_memory", {
  "content": "...",
  "domain": "spec-kit",
  "tags": ["spec:SPEC-ID", "stage:plan"],
  "importance": 8
})

# Via CLI (manual)
local-memory remember "knowledge here" \
  --importance 8 \
  --domain spec-kit \
  --tags spec:SPEC-123
```

### Retrieve Knowledge
```bash
# Via MCP tool (in code)
mcp_manager.call_tool("local-memory", "search", {
  "query": "consensus plan",
  "limit": 20,
  "tags": ["spec:SPEC-ID", "stage:plan"],
  "search_type": "hybrid"
})

# Via CLI (manual)
local-memory search "consensus" --tags spec:SPEC-123
```

### Query Best Practices
1. **Before** any task: Query local-memory for relevant context
2. **During** work: Store decisions with importance ≥7
3. **After** completion: Store outcomes, evidence paths, validation results
4. **Tag Structure**: Use `spec:SPEC-ID`, `stage:STAGE`, `consensus-verdict` for spec-kit artifacts

---

## Integration Points

**Spec-Kit Consensus** (`tui/spec_kit/consensus.rs`):
- `fetch_memory_entries()`: Searches local-memory for agent artifacts
- `remember_consensus_verdict()`: Stores synthesis results
- **Fallback**: File-based evidence if MCP unavailable (see ARCH-002)

**Session Context** (`tui/spec_prompts.rs`):
- `gather_local_memory_context()`: Retrieves historical context for agents
- Used by all 6 spec-kit stages (plan, tasks, implement, validate, audit, unlock)

**Evidence Repository** (`tui/spec_kit/evidence.rs`):
- Writes artifacts to filesystem: `docs/SPEC-OPS-004.../evidence/`
- Local-memory stores metadata pointing to evidence files

---

## Migration Complete

**Status**: Byterover migration to local-memory **COMPLETE** as of 2025-10-18

**What Changed**:
- Subprocess `Command::new("local-memory")` → Native MCP
- Performance: 5.3x faster (46ms → 8.7ms measured)
- Reliability: 3-retry logic with exponential backoff
- Testing: 3 integration tests validate MCP path

**Removed**:
- ~~Byterover MCP tool calls~~
- ~~Byterover fallback logic~~
- ~~Subprocess local-memory wrapper~~ (deprecated, pending deletion)

---

## Do Not Use

**Forbidden MCP Servers** (for memory):
- ❌ `byterover-mcp`
- ❌ Any memory system other than `local-memory`

**Exception**: MCP servers for **tools** (not memory) are allowed:
- ✅ `git` (version control operations)
- ✅ `serena` (code search)
- ✅ `ide` (editor integrations)
- ✅ etc.

**Distinction**:
- **Memory MCP**: Stores/retrieves knowledge (local-memory ONLY)
- **Tool MCP**: Provides functionality (git, serena, etc. - allowed)

---

## Enforcement

**Code Reviews**: Flag any byterover references
**Documentation**: This policy referenced in CLAUDE.md, REVIEW.md
**Validation**: `grep -r "byterover" . --include="*.rs"` should return 0 matches

**Last Verified**: 2025-10-18 (no byterover in active codebase)

---

## Questions?

If uncertain about memory system usage:
1. Default to local-memory MCP
2. Check this policy document
3. Ask maintainer if edge case arises

**Maintainer**: theturtlecsz
**Repository**: https://github.com/theturtlecsz/code
