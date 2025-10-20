# PRD: Migrate Spec-Kit Commands to Native Codex Tools

**SPEC-ID**: SPEC-KIT-066
**Created**: 2025-10-20
**Status**: Backlog
**Priority**: P1 (High - Production Readiness)

---

## Problem Statement

**Current State**:
- `/speckit.new` orchestrator instructions reference Python scripts (`generate_spec_id.py`)
- `/speckit.auto` orchestrator instructions reference bash scripts (`spec_ops_{stage}.sh`)
- These external dependencies:
  - Reduce portability
  - Add failure modes
  - Complicate debugging
  - Violate "native-first" principle established in ARCH-004

**Discovered During**: 2025-10-20 real-world testing session
- Attempted to run `/speckit.new` for first non-meta feature
- Found routing.rs bug (FIXED)
- Found orchestrator doesn't execute tools (uses bash/python instead)

---

## Proposed Solution

### Phase 1: Research & Inventory (1-2 hours)

**Audit all subagent commands in `~/.code/config.toml`**:
1. List every `[[subagents.commands]]` entry
2. Identify bash/python/script references
3. Categorize by complexity:
   - **Simple** (can replace with Glob/Read/Write/Edit immediately)
   - **Medium** (requires logic but doable natively)
   - **Complex** (bash scripts legitimately needed - e.g., guardrails)

**Deliverable**: Markdown inventory table with replacement strategy

### Phase 2: Implement Native Replacements (3-6 hours)

**For each SIMPLE/MEDIUM command**:
1. Rewrite orchestrator-instructions to use native Codex tools
2. Test with real execution
3. Verify output matches bash/python version
4. Document any limitations

**Priority Order**:
1. `speckit.new` (HIGH - blocks feature creation)
2. `speckit.specify` (HIGH - blocks PRD generation)
3. `speckit.clarify/analyze/checklist` (MEDIUM - quality gates)
4. `speckit.auto` guardrails (LOW - complex, defer if possible)

### Phase 3: Test & Validate (1-2 hours)

**Validation Criteria**:
- Run `/speckit.new` on test feature → SPEC created
- Run `/speckit.auto SPEC-ID` → All stages execute
- Compare output to bash/python versions
- Verify 604 tests still pass

---

## Acceptance Criteria

**Must Have**:
- ✅ `speckit.new` works without Python scripts
- ✅ SPEC-ID generation native (Glob + parse + increment)
- ✅ Directory creation native (Write tool implicit)
- ✅ SPEC.md updates native (Edit tool)
- ✅ Template rendering native (Read tool + fill)

**Should Have**:
- ✅ All non-guardrail commands use native tools only
- ✅ Documentation updated (config.toml comments explain approach)
- ✅ Testing plan for each migrated command

**Nice to Have**:
- ⏸️ Guardrail bash scripts replaced (complex, defer to Phase 4)
- ⏸️ Git commit automation via git_workflow agent

---

## Technical Requirements

### Native Tool Capabilities

**Available Codex Tools**:
- **Glob**: Find files by pattern (replacement for `ls docs/SPEC-KIT-*`)
- **Read**: Read file contents (replacement for `cat`)
- **Write**: Create files with parent dirs (replacement for `mkdir -p` + `echo >`)
- **Edit**: Modify existing files (replacement for `sed`/`awk`)
- **Bash**: Execute commands (use ONLY when native tools insufficient)

### SPEC-ID Generation (Native)

**Algorithm**:
```
1. Use Glob: pattern="SPEC-KIT-*" path="docs/"
2. Parse numbers from results (e.g., "SPEC-KIT-060" → 60)
3. Find max: 60
4. Increment: 61
5. Create slug: slugify(feature_description)
6. Return: "SPEC-KIT-061-<slug>"
```

**Implementation**: Pure Rust logic in orchestrator instructions

### Template Rendering (Native)

**Current**:
```bash
# Python template engine or bash substitution
cat template.md | sed "s/\[FEATURE_NAME\]/$feature/g"
```

**Native**:
```
1. Read tool: templates/PRD-template.md
2. String replacement in orchestrator (built-in capability)
3. Write tool: docs/SPEC-KIT-###/PRD.md
```

### SPEC.md Updates (Native)

**Current**:
```python
# Python script parses SPEC.md, finds insertion point, writes new row
```

**Native**:
```
1. Read tool: SPEC.md
2. Find insertion point (search for "## Active Tasks" or last row)
3. Edit tool: Add new table row
4. Format: | Order | SPEC-ID | Title | Status | ... |
```

---

## Out of Scope

**Explicitly NOT included**:
- ❌ Guardrail bash script replacement (too complex, legitimate bash use)
- ❌ HAL validation (MCP server, works correctly)
- ❌ Evidence archival scripts (operational tooling, not core workflow)
- ❌ Git hook scripts (development tooling, not core workflow)

**Guardrails remain bash** because they:
- Execute complex validation logic
- Interface with cargo/clippy/fmt
- Parse JSON telemetry
- Handle multi-step scenarios
- Are well-tested and stable

---

## Testing Plan

**For Each Migrated Command**:

1. **Before**: Run with bash/python version, capture output
2. **After**: Run with native version, capture output
3. **Compare**: Verify identical outcomes
4. **Regression**: Run full test suite (604 tests)

**Test Cases**:
- `speckit.new` with simple description → SPEC created
- `speckit.new` with complex description → PRD comprehensive
- `speckit.new` edge cases (special chars, very long, empty)
- SPEC-ID increment works correctly
- SPEC.md updates don't corrupt table

---

## Success Metrics

**Completion Criteria**:
- ✅ Zero Python dependencies for core workflows
- ✅ Bash only for guardrails (documented exception)
- ✅ All commands executable without external scripts
- ✅ Config.toml orchestrator-instructions use native tools
- ✅ First real-world test passes (`/speckit.new` → `/speckit.auto`)

**Performance**:
- ⚡ Faster execution (no process spawning overhead)
- 📦 Simpler deployment (fewer dependencies)
- 🐛 Easier debugging (pure Rust stack traces)

---

## Implementation Notes

**Discovered Bugs** (2025-10-20):

1. **Routing Bug** (FIXED):
   - SpecKitCommand registry wasn't passing config to format_subagent_command
   - Result: Commands showed metadata but didn't execute
   - Fix: routing.rs now passes widget.config.agents and widget.config.subagent_commands
   - Commit: Pending

2. **Orchestrator Instructions** (PENDING):
   - Instructions reference bash/python that orchestrator can't/shouldn't execute
   - Result: Orchestrator creates plans instead of executing tools
   - Fix: Rewrite to be imperative with native tools
   - This SPEC documents the work needed

---

## Next Steps

**Immediate** (This SPEC):
1. Research all subagent commands (inventory bash/python usage)
2. Design native replacements
3. Implement and test
4. Update config.toml
5. Validate with real-world test

**Future** (Optional):
- Guardrail migration (if bash scripts become maintenance burden)
- Performance benchmarking (native vs bash)
- Error handling improvements

---

## References

- ARCH-004: Native MCP migration (completed 2025-10-18)
- MAINT-1: Subprocess migration completion
- ~/.code/config.toml: Current orchestrator instructions (lines 214-523)
- Session findings: 2025-10-20 real-world testing
