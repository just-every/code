**SPEC-ID**: [SPEC_ID]
**Feature**: [FEATURE_NAME]
**Status**: Backlog
**Created**: [CREATION_DATE]
**Branch**: [BRANCH_NAME]
**Owner**: [OWNER]

**Context**: [BACKGROUND_PROBLEM_STATEMENT]

---

## User Scenarios

### P1: [HIGH_PRIORITY_USER_STORY_TITLE]

**Story**: As a [USER_TYPE], I want [GOAL] so that [BENEFIT]

**Priority Rationale**: [WHY_THIS_IS_P1]

**Testability**: [HOW_TO_VERIFY_INDEPENDENTLY]

**Acceptance Scenarios**:
- Given [CONTEXT], when [ACTION], then [OUTCOME]
- Given [CONTEXT], when [ACTION], then [OUTCOME]
- Given [ERROR_CONDITION], when [ACTION], then [ERROR_HANDLING]

### P2: [MEDIUM_PRIORITY_STORY_TITLE]

**Story**: As a [USER_TYPE], I want [GOAL] so that [BENEFIT]

**Priority Rationale**: [WHY_P2_NOT_P1]

**Testability**: [VERIFICATION_METHOD]

**Acceptance Scenarios**:
- Given [CONTEXT], when [ACTION], then [OUTCOME]

### P3: [LOW_PRIORITY_STORY_TITLE]

**Story**: As a [USER_TYPE], I want [GOAL] so that [BENEFIT]

**Priority Rationale**: [WHY_P3]

**Testability**: [VERIFICATION]

**Acceptance Scenarios**:
- Given [CONTEXT], when [ACTION], then [OUTCOME]

---

## Edge Cases

- [BOUNDARY_CONDITION_1]
- [NULL_OR_EMPTY_INPUT_HANDLING]
- [CONCURRENT_ACCESS_SCENARIO]
- [ERROR_RECOVERY_CASE]
- [PERFORMANCE_LIMIT_CASE]

---

## Requirements

### Functional Requirements

- **FR1**: [REQUIREMENT_WITH_ACCEPTANCE_CRITERIA]
- **FR2**: [REQUIREMENT]
- **FR3**: [REQUIREMENT]

### Non-Functional Requirements

- **Performance**: [METRIC_OR_CONSTRAINT]
- **Security**: [SECURITY_REQUIREMENT]
- **Scalability**: [SCALE_REQUIREMENT]
- **Reliability**: [UPTIME_OR_ERROR_RATE]

---

## Success Criteria

- [MEASURABLE_OUTCOME_1]
- [QUANTIFIABLE_METRIC_2]
- [OBJECTIVE_SUCCESS_INDICATOR_3]

---

## Evidence & Validation

**Acceptance Tests**: See tasks.md for detailed test mapping

**Telemetry Path**: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/[SPEC_ID]/`

**Consensus Evidence**: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/[SPEC_ID]/`

**Validation Commands**:
```bash
# Run individual stages
/speckit.plan [SPEC_ID]
/speckit.tasks [SPEC_ID]
/speckit.implement [SPEC_ID]

# Run full pipeline
/speckit.auto [SPEC_ID]

# Check status
/speckit.status [SPEC_ID]
```

---

## Clarifications

### [DATE] - Initial Spec Creation

**Clarification needed**: [QUESTION_OR_AMBIGUITY]

**Resolution**: [ANSWER_OR_DECISION]

**Updated sections**: [WHICH_PARTS_CHANGED]

---

## Dependencies

- [UPSTREAM_DEPENDENCY_1]
- [SERVICE_REQUIREMENT_1]
- [PREREQUISITE_SPEC_1]

---

## Notes

[ANY_ADDITIONAL_CONTEXT_OR_WARNINGS]
