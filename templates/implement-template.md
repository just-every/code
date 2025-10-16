# Implementation: [FEATURE_NAME]

**SPEC-ID**: [SPEC_ID]
**Implementation Version**: [VERSION]
**Created**: [DATE]

---

## Inputs

**Plan**: docs/[SPEC_ID]/plan.md (hash: [SHA256])
**Tasks**: docs/[SPEC_ID]/tasks.md (hash: [SHA256])
**Spec**: docs/[SPEC_ID]/spec.md
**Prompt Version**: [PROMPT_VERSION]

---

## Implementation Strategy

### Approach Overview

**Primary Strategy**: [INCREMENTAL|BIG_BANG|FEATURE_FLAG|PARALLEL]

**Rationale**: [WHY_THIS_APPROACH]

**Phases**:
1. [PHASE_1_NAME]: [WHAT_GETS_BUILT]
2. [PHASE_2_NAME]: [NEXT_INCREMENT]
3. [PHASE_3_NAME]: [FINAL_STEPS]

---

## Code Changes

### New Files

**File**: [PATH/TO/NEW/FILE]
**Purpose**: [WHAT_IT_DOES]
**Key Components**:
- [STRUCT/FUNCTION_1]: [RESPONSIBILITY]
- [STRUCT/FUNCTION_2]: [BEHAVIOR]

**File**: [ANOTHER_NEW_FILE]
**Purpose**: [DESCRIPTION]

### Modified Files

**File**: [EXISTING/FILE/PATH]
**Changes**:
- Add: [NEW_FUNCTION_OR_FIELD]
- Modify: [CHANGED_BEHAVIOR]
- Remove: [DEPRECATED_CODE]

**Rationale**: [WHY_THESE_CHANGES]

**File**: [ANOTHER_MODIFIED_FILE]
**Changes**: [DETAILS]

---

## Diff Proposals

### Change 1: [COMPONENT_NAME]

**File**: [PATH]
**Type**: [ADD|MODIFY|DELETE]

```diff
[DIFF_CONTENT_OR_SUMMARY]
```

**Rationale**: [WHY_THIS_CHANGE]
**Confidence**: [HIGH|MEDIUM|LOW]

### Change 2: [ANOTHER_CHANGE]

**File**: [PATH]
**Summary**: [WHAT_CHANGES]

---

## Test Strategy

### Unit Tests

**File**: [TEST_FILE_PATH]
**Coverage**:
- [FUNCTION_1]: [TEST_SCENARIO]
- [FUNCTION_2]: [EDGE_CASE]

**Commands**:
```bash
cargo test [TEST_NAME]
```

### Integration Tests

**Scenario**: [INTEGRATION_TEST_NAME]
**Steps**:
1. [SETUP]
2. [ACTION]
3. [VERIFICATION]

**Expected**: [OUTCOME]

---

## Validation Checklist

- [ ] Code compiles (`cargo build`)
- [ ] Tests pass (`cargo test`)
- [ ] Lint clean (`cargo clippy`)
- [ ] Format clean (`cargo fmt --check`)
- [ ] No new warnings
- [ ] Acceptance criteria met (see tasks.md)
- [ ] Edge cases handled
- [ ] Error paths tested

---

## Tool Calls Required

**During Implementation**:
```bash
[COMMAND_1]  # Purpose: [WHY]
[COMMAND_2]  # Purpose: [WHY]
```

**For Validation**:
```bash
[TEST_COMMAND]
[LINT_COMMAND]
```

---

## Risks & Edge Cases

### Risk 1: [IMPLEMENTATION_RISK]

**Likelihood**: [HIGH|MEDIUM|LOW]
**Impact**: [SEVERITY]
**Mitigation**: [HOW_TO_HANDLE]

### Edge Case 1: [SCENARIO]

**Handling**: [HOW_CODE_ADDRESSES_IT]
**Test**: [VERIFICATION_METHOD]

---

## Multi-Agent Consensus

### Implementation Agreements

**All agents aligned on:**
- [TECHNICAL_DECISION_1]
- [APPROACH_2]
- [CONSTRAINT_3]

### Conflicts Resolved

**Issue**: [DISAGREEMENT]
**Resolution**: [FINAL_DECISION]

---

## Rollback Plan

**If implementation fails validation:**
1. [REVERT_STEP_1]
2. [RESTORE_STEP_2]  
3. [CLEANUP_STEP_3]

**Backup locations**: [WHERE_BACKUPS_ARE]

---

## Evidence References

**Implementation Consensus**: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/[SPEC_ID]/spec-implement_synthesis.json`

**Telemetry**: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/[SPEC_ID]/spec-implement_[TIMESTAMP].json`

**Agent Outputs**: Gemini analysis, Claude strategy, GPT-Codex diffs, GPT-Pro validation
