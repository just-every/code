# Cross-Artifact Analysis: [FEATURE_NAME]

**SPEC-ID**: [SPEC_ID]
**Analysis Version**: [VERSION]
**Created**: [DATE]

---

## Inputs

**Artifacts Analyzed**:
- spec.md (hash: [SHA256])
- PRD.md (hash: [SHA256])
- plan.md (hash: [SHA256])
- tasks.md (hash: [SHA256])

**Prompt Version**: [PROMPT_VERSION]

---

## Consistency Check

### PRD ↔ Spec Alignment

**Issue**: [INCONSISTENCY_OR_OK]
**Details**: [WHAT_MISMATCHES]
**Severity**: [HIGH|MEDIUM|LOW]
**Fix**: [PROPOSED_CORRECTION]

### Plan ↔ Spec Alignment

**Check**: [WHAT_WAS_COMPARED]
**Finding**: [ALIGNED|GAP_FOUND]
**Gap**: [DESCRIPTION_IF_ANY]

### Tasks ↔ Acceptance Criteria

**Coverage**: [PERCENTAGE]% of acceptance criteria have tasks
**Missing Coverage**:
- [UNCOVERED_REQUIREMENT_1]
- [UNCOVERED_REQUIREMENT_2]

**Action**: [CREATE_TASKS_OR_OK]

---

## Conflicts Detected

### Conflict 1: [DESCRIPTION]

**Artifacts**: [WHICH_DOCS_CONFLICT]
**Contradiction**: [WHAT_CONTRADICTS]
**Resolution**: [WHICH_IS_CORRECT]

---

## Auto-Fix Proposals

### Fix 1: [WHAT_TO_FIX]

**File**: [ARTIFACT]
**Change**: [PROPOSED_EDIT]
**Rationale**: [WHY]

**Apply?**: [Y|N|MANUAL_REVIEW]

---

## Quality Scores

**Completeness**: [SCORE]/10
**Consistency**: [SCORE]/10  
**Traceability**: [SCORE]/10
**Clarity**: [SCORE]/10

**Overall**: [AVERAGE]/10

---

## Recommendations

1. [IMPROVEMENT_1]
2. [FIX_2]
3. [ENHANCEMENT_3]

---

## Multi-Agent Consensus

### Analysis Agreements

- [FINDING_ALL_AGENTS_AGREE_ON]

### Divergent Assessments

**Issue**: [WHAT_AGENTS_DISAGREE_ABOUT]
**Resolution**: [CONSENSUS]

---

## Evidence References

**Analysis Consensus**: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/[SPEC_ID]/spec-analyze_synthesis.json`
