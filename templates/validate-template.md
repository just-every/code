# Validation: [FEATURE_NAME]

**SPEC-ID**: [SPEC_ID]
**Validation Version**: [VERSION]
**Created**: [DATE]

---

## Inputs

**Implementation**: docs/[SPEC_ID]/implementation.md (hash: [SHA256])
**Tasks**: docs/[SPEC_ID]/tasks.md
**Spec**: docs/[SPEC_ID]/spec.md
**Prompt Version**: [PROMPT_VERSION]

---

## Test Strategy

### Test Scenarios

#### Scenario 1: [PRIMARY_HAPPY_PATH]

**Given**: [PRECONDITIONS]
**When**: [ACTION]
**Then**: [EXPECTED_RESULT]

**Command**:
```bash
[TEST_COMMAND]
```

**Expected Output**: [WHAT_SUCCESS_LOOKS_LIKE]

#### Scenario 2: [ERROR_HANDLING]

**Given**: [ERROR_CONDITION]
**When**: [TRIGGER]
**Then**: [ERROR_BEHAVIOR]

**Validation**: [HOW_TO_VERIFY]

#### Scenario 3: [EDGE_CASE]

**Description**: [SCENARIO]
**Test**: [COMMAND]
**Pass Criteria**: [CRITERIA]

---

## Validation Results

### Scenario Outcomes

| Scenario | Status | Evidence | Notes |
|----------|--------|----------|-------|
| [SCENARIO_1] | [PASSED|FAILED|SKIPPED] | [LOG_FILE_OR_OUTPUT] | [DETAILS] |
| [SCENARIO_2] | [STATUS] | [EVIDENCE] | [NOTES] |
| [SCENARIO_3] | [STATUS] | [EVIDENCE] | [NOTES] |

**Summary**: [X] of [Y] scenarios passed

---

## Acceptance Criteria Validation

| Requirement (from spec.md) | Validation Method | Result | Evidence |
|----------------------------|-------------------|--------|----------|
| R1: [REQUIREMENT] | [HOW_TESTED] | [PASS|FAIL] | [ARTIFACT] |
| R2: [REQUIREMENT] | [METHOD] | [RESULT] | [EVIDENCE] |
| R3: [REQUIREMENT] | [VALIDATION] | [OUTCOME] | [PROOF] |

---

## Edge Cases Tested

### Edge Case 1: [SCENARIO]

**Test**: [WHAT_WAS_TESTED]
**Result**: [PASS|FAIL]
**Notes**: [OBSERVATIONS]

### Edge Case 2: [ANOTHER_CASE]

**Test**: [METHOD]
**Result**: [OUTCOME]

---

## Failure Analysis

### Failed Tests

**Test**: [FAILED_TEST_NAME]
**Failure Mode**: [WHAT_WENT_WRONG]
**Root Cause**: [WHY]
**Remediation**: [FIX_REQUIRED]

---

## Multi-Agent Consensus

### Validation Agreements

**All agents confirm:**
- [VALIDATION_RESULT_1]
- [TEST_OUTCOME_2]
- [QUALITY_ASSESSMENT_3]

### Conflicts

**Issue**: [DISAGREEMENT_ABOUT_RESULTS]
**Resolution**: [CONSENSUS_DECISION]

---

## Decision

**Outcome**: [PASS|FAIL|PARTIAL]

**Rationale**: [WHY_THIS_DECISION]

**Next Actions**:
- If PASS: Proceed to /speckit.audit
- If FAIL: Return to /speckit.implement with findings
- If PARTIAL: [SPECIFIC_ACTIONS]

---

## Evidence References

**Validation Telemetry**: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/[SPEC_ID]/spec-validate_[TIMESTAMP].json`

**Test Logs**: [PATHS_TO_LOG_FILES]

**Consensus**: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/[SPEC_ID]/spec-validate_synthesis.json`
