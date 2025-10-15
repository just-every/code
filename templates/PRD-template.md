# PRD: [FEATURE_NAME]

**SPEC-ID**: [SPEC_ID]
**Status**: Draft
**Created**: [DATE]
**Author**: Multi-agent consensus (gemini, claude, code)

---

## Problem Statement

**Current State**: [WHAT_EXISTS_TODAY]

**Pain Points**:
- [USER_PAIN_1]
- [INEFFICIENCY_1]
- [GAP_1]

**Impact**: [WHY_THIS_MATTERS]

---

## Target Users & Use Cases

### Primary User: [USER_TYPE_1]

**Profile**: [USER_DESCRIPTION]

**Current Workflow**: [HOW_THEY_WORK_TODAY]

**Pain Points**: [WHAT_FRUSTRATES_THEM]

**Desired Outcome**: [WHAT_THEY_WANT]

### Secondary User: [USER_TYPE_2]

**Profile**: [DESCRIPTION]

**Use Case**: [HOW_THEY_USE_THE_SYSTEM]

---

## Goals

### Primary Goals

1. **[GOAL_1]**: [DESCRIPTION]
   **Success Metric**: [HOW_TO_MEASURE]

2. **[GOAL_2]**: [DESCRIPTION]
   **Success Metric**: [METRIC]

### Secondary Goals

1. **[SECONDARY_GOAL]**: [DESCRIPTION]

---

## Non-Goals

**Explicitly Out of Scope**:
- [WHAT_WE_WONT_DO_1]
- [FUTURE_ENHANCEMENT_1]
- [RELATED_BUT_SEPARATE_CONCERN]

**Rationale**: [WHY_THESE_ARE_NON_GOALS]

---

## Scope & Assumptions

**In Scope**:
- [INCLUDED_FEATURE_1]
- [INCLUDED_CAPABILITY_2]

**Assumptions**:
- [ASSUMPTION_1]
- [DEPENDENCY_ASSUMPTION_2]

**Constraints**:
- [TECHNICAL_CONSTRAINT]
- [RESOURCE_CONSTRAINT]
- [TIME_CONSTRAINT]

---

## Functional Requirements

| ID | Requirement | Acceptance Criteria | Priority |
|----|-------------|---------------------|----------|
| FR1 | [REQUIREMENT_DESCRIPTION] | [HOW_TO_VERIFY] | P1 |
| FR2 | [REQUIREMENT] | [CRITERIA] | P1 |
| FR3 | [REQUIREMENT] | [CRITERIA] | P2 |

---

## Non-Functional Requirements

| ID | Requirement | Target Metric | Validation Method |
|----|-------------|---------------|-------------------|
| NFR1 | Performance | [LATENCY_TARGET] | [LOAD_TEST_COMMAND] |
| NFR2 | Reliability | [UPTIME_TARGET] | [MONITORING_DASHBOARD] |
| NFR3 | Security | [SECURITY_STANDARD] | [AUDIT_PROCESS] |
| NFR4 | Scalability | [SCALE_TARGET] | [STRESS_TEST] |

---

## User Experience

**Key Workflows**:

### Workflow 1: [PRIMARY_USER_FLOW]

**Steps**:
1. User [ACTION_1]
2. System [RESPONSE_1]
3. User [ACTION_2]
4. System [RESPONSE_2]

**Success Path**: [HAPPY_PATH_OUTCOME]

**Error Paths**:
- If [ERROR_CONDITION], then [HANDLING]

### Workflow 2: [SECONDARY_FLOW]

...

---

## Dependencies

**Technical**:
- [LIBRARY_OR_FRAMEWORK_1]
- [SERVICE_DEPENDENCY_1]

**Organizational**:
- [TEAM_DEPENDENCY]
- [APPROVAL_REQUIREMENT]

**Data**:
- [DATA_SOURCE_1]
- [MIGRATION_REQUIREMENT]

---

## Risks & Mitigations

| Risk | Impact | Probability | Mitigation | Owner |
|------|--------|-------------|------------|-------|
| [RISK_1] | High | Medium | [MITIGATION_STRATEGY] | [OWNER] |
| [RISK_2] | Medium | Low | [STRATEGY] | [OWNER] |

---

## Success Metrics

**Launch Criteria**:
- [CRITERION_1]
- [CRITERION_2]

**Post-Launch Metrics** (measure 30 days after release):
- [KPI_1]: Target [VALUE]
- [KPI_2]: Target [VALUE]
- [USER_SATISFACTION]: Target [SCORE]

---

## Validation Plan

### Testing Strategy

1. **Unit Tests**: [COVERAGE_TARGET]
2. **Integration Tests**: [TEST_SCENARIOS]
3. **E2E Tests**: [CRITICAL_PATHS]
4. **Performance Tests**: [LOAD_PROFILE]

### Review Process

1. **PRD Review**: [STAKEHOLDERS]
2. **Design Review**: [REVIEWERS]
3. **Code Review**: [PROCESS]
4. **Security Review**: [IF_APPLICABLE]

---

## Multi-Agent Consensus

### PRD Quality Assessment

**Completeness**: [SCORE_OR_ASSESSMENT]

**Clarity**: [ALL_REQUIREMENTS_UNAMBIGUOUS]

**Testability**: [ALL_CRITERIA_MEASURABLE]

### Conflicts Resolved

**Issue**: [IF_AGENTS_DISAGREED_ON_SCOPE_OR_APPROACH]

**Resolution**: [HOW_CONSENSUS_REACHED]

---

## Evidence & Telemetry

**PRD Creation Evidence**: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/[SPEC_ID]/prd-consensus.json`

**Agent Outputs**: Multi-agent PRD proposals stored in `.code/agents/` with UUIDs

**Validation**: Use `/speckit.analyze [SPEC_ID]` to check PRD ↔ spec consistency

---

## Open Questions

1. **[QUESTION_1]**: [WHAT_NEEDS_CLARIFICATION]
   **Impact**: [HIGH|MEDIUM|LOW]
   **Blocker**: [YES|NO]

2. **[QUESTION_2]**: [UNRESOLVED_DECISION]
   **Impact**: [LEVEL]
   **Resolution Path**: [HOW_TO_RESOLVE]

**Use `/speckit.clarify [SPEC_ID]` to resolve systematically.**

---

## Changelog

### [DATE] - Initial PRD
- Created by multi-agent consensus
- [MAJOR_DECISIONS_MADE]

### [DATE] - Revision
- Updated [SECTION] based on [FEEDBACK_OR_CLARIFICATION]
- Resolved: [WHAT_CHANGED]
