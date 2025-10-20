# Phase 1 Inventory: Spec-Kit Command Analysis

**Created**: 2025-10-20
**SPEC-ID**: SPEC-KIT-066
**Purpose**: Identify bash/python dependencies in orchestrator instructions

---

## Command Inventory

| Command | Line # | Bash/Python References | Complexity | Native Replacement | Priority |
|---------|--------|------------------------|------------|-------------------|----------|
| **speckit.new** | 222-269 | `python3 generate_spec_id.py`<br>`mkdir -p`<br>`git add/commit` | MEDIUM | Glob+parse for SPEC-ID<br>Write (auto-creates dirs)<br>git-workflow agent | **P0** |
| **speckit.specify** | 272-281 | None (already native) | SIMPLE | ✅ No changes needed | P3 |
| **speckit.plan** | 284-288 | None (agent consensus) | SIMPLE | ✅ No changes needed | P3 |
| **speckit.tasks** | 291-299 | None (agent consensus) | SIMPLE | ✅ No changes needed | P3 |
| **speckit.implement** | 302-311 | `scripts/env_run.sh cargo fmt`<br>clippy, builds, tests | COMPLEX | KEEP bash for cargo/clippy<br>Native for file operations | P2 |
| **speckit.auto** | 314-398 | `bash scripts/spec_ops_{stage}.sh` (guardrails) | COMPLEX | **KEEP AS BASH** (legitimate) | P4 |
| **speckit.clarify** | 401-420 | None (agent consensus) | SIMPLE | ✅ No changes needed | P3 |
| **speckit.analyze** | 423-443 | None (agent consensus) | SIMPLE | ✅ No changes needed | P3 |
| **speckit.checklist** | 446-463 | None (agent evaluation) | SIMPLE | ✅ No changes needed | P3 |

---

## Detailed Analysis

### P0: speckit.new (CRITICAL - Blocks feature creation)

**Current Bash/Python Dependencies**:
```bash
# Line 231: SPEC-ID generation
python3 scripts/spec_ops_004/generate_spec_id.py "<feature-description>"

# Line 235: Directory creation
mkdir -p docs/${SPEC_ID}/

# Lines 253-254: Git operations
git add SPEC.md docs/${SPEC_ID}/
git commit -m "feat(${SPEC_ID}): created via /new-spec"
```

**Native Replacement Strategy**:
1. **SPEC-ID Generation**:
   ```
   - Glob tool: pattern="SPEC-KIT-*" in docs/
   - Parse numbers: extract(\d+) from "SPEC-KIT-060-slug"
   - Find max: 60
   - Increment: 61
   - Slugify: lowercase, replace spaces with "-", remove special chars
   - Result: "SPEC-KIT-061-{slug}"
   ```

2. **Directory Creation**:
   ```
   - Write tool automatically creates parent directories
   - Write: docs/SPEC-KIT-061-{slug}/PRD.md
   - No explicit mkdir needed
   ```

3. **Git Operations**:
   ```
   - OPTION A: Use git-workflow agent (recommended, follows RULES.md)
   - OPTION B: Bash tool for git commands (if agent unavailable)
   - OPTION C: Skip git commit (let user commit manually)
   ```

**Recommended Approach**: Native Glob+Write + git-workflow agent

---

### P1: speckit.implement (Build validation required)

**Current Bash Dependencies**:
```bash
scripts/env_run.sh cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build/test commands
```

**Native Replacement Strategy**:
- **Keep bash for cargo operations** (legitimate complexity)
- **Use native tools for file operations** (Read, Write, Edit for code changes)
- **Hybrid approach**: Native for editing, bash for validation

**Rationale**: cargo/clippy are external tools, bash is appropriate wrapper

---

### P2: All Others (Already Native or Legitimate Bash)

**speckit.specify, plan, tasks, clarify, analyze, checklist**:
- ✅ Already use native agent consensus
- ✅ No bash/python dependencies
- ✅ No changes needed

**speckit.auto**:
- Uses bash guardrail scripts: `spec_ops_{stage}.sh`
- **KEEP AS BASH** - legitimate use case:
  - Complex validation logic
  - Cargo/clippy integration
  - JSON telemetry parsing
  - Multi-step scenarios
  - Well-tested and stable

---

## Migration Priority Order

### Phase 2 Focus (P0):
1. ✅ **speckit.new** - Migrate to native tools (2-3 hours)

### Phase 3 Focus (P1-P2):
2. ⏸️ **speckit.implement** - Verify hybrid approach (30 min)
3. ⏸️ **speckit.auto** - Confirm bash retention (15 min)

### No Action Required (P3):
4. ✅ speckit.specify, plan, tasks, clarify, analyze, checklist - Already native

---

## Evidence: Bash/Python References

### speckit.new (Lines 225-269)
```toml
orchestrator-instructions = """
Create SPEC from feature description with multi-agent consensus on PRD.

Execute these steps:

1. Generate SPEC-ID:
   Run: python3 scripts/spec_ops_004/generate_spec_id.py "<feature-description>"
   Store as SPEC_ID

2. Create directory:
   mkdir -p docs/${SPEC_ID}/

...

6. Commit changes (required for clean tree):
   Run: git add SPEC.md docs/${SPEC_ID}/
   Run: git commit -m "feat(${SPEC_ID}): created via /new-spec"
```

**Problem**: Orchestrator interprets "Run: python3..." as description, not command
**Impact**: Creates plan documents instead of executing tools
**Solution**: Rewrite to imperatively use Glob, Read, Write, Edit tools

---

### speckit.implement (Lines 302-311)
```toml
3. Apply the chosen diff locally, then run `scripts/env_run.sh cargo fmt --all -- --check`, clippy, targeted builds/tests, and any spec-specific checks; attach logs to the command output.
```

**Assessment**: Bash appropriate for cargo operations
**Action**: Keep bash, verify instructions are imperative

---

### speckit.auto (Lines 314-398)
```toml
STEP 1: Run guardrail validation
Execute: bash scripts/spec_ops_004/commands/spec_ops_{stage}.sh {SPEC-ID}
If guardrail fails (exit != 0): HALT with error message
```

**Assessment**: Guardrail scripts are complex and well-tested
**Action**: KEEP AS BASH (documented exception per PRD)

---

## Next Steps

**Immediate** (Phase 2):
1. Draft new orchestrator-instructions for speckit.new
2. Use template from SESSION-RESTART-PROMPT.md (lines 155-192)
3. Edit ~/.code/config.toml
4. Test with ./build-fast.sh + /speckit.new

**Store in Local-Memory**:
- This inventory analysis (importance: 8)
- Decision rationale for keeping bash in implement/auto
- Native replacement algorithms

**Success Criteria for Phase 1**:
- ✅ All 9 commands analyzed
- ✅ Bash/python dependencies identified
- ✅ Native replacement strategy documented
- ✅ Priority order determined
- ⏳ Stored in local-memory
