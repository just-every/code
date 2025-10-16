# Audit: [FEATURE_NAME]

**SPEC-ID**: [SPEC_ID]
**Audit Version**: [VERSION]
**Created**: [DATE]

---

## Inputs

**Validation Results**: docs/[SPEC_ID]/validation.md
**Implementation**: docs/[SPEC_ID]/implementation.md
**Spec**: docs/[SPEC_ID]/spec.md
**Prompt Version**: [PROMPT_VERSION]

---

## Compliance Review

### Code Quality

**Metric**: [QUALITY_ASPECT]
**Assessment**: [PASS|FAIL|CONCERN]
**Evidence**: [WHAT_WAS_CHECKED]
**Notes**: [FINDINGS]

### Security

**Review Area**: [SECURITY_ASPECT]
**Status**: [SECURE|VULNERABLE|NEEDS_REVIEW]
**Findings**: [SECURITY_ISSUES_OR_CLEARANCE]
**Remediation**: [FIX_REQUIRED_IF_ANY]

### Performance

**Concern**: [PERFORMANCE_ASPECT]
**Assessment**: [ACCEPTABLE|ISSUE]
**Metrics**: [MEASUREMENTS]

---

## Requirement Compliance

| Requirement | Implemented | Tested | Compliant | Notes |
|-------------|-------------|--------|-----------|-------|
| R1: [REQ] | [Y|N] | [Y|N] | [Y|N] | [DETAILS] |
| R2: [REQ] | [Y|N] | [Y|N] | [Y|N] | [NOTES] |
| R3: [REQ] | [Y|N] | [Y|N] | [Y|N] | [OBSERVATIONS] |

**Compliance Rate**: [X]% ([Y] of [Z] requirements fully compliant)

---

## Risk Assessment

### Identified Risks

**Risk**: [PRODUCTION_RISK]
**Severity**: [HIGH|MEDIUM|LOW]
**Mitigation Status**: [ADDRESSED|PENDING|ACCEPTED]
**Notes**: [DETAILS]

---

## Quality Gates

- [ ] All tests passing
- [ ] No security vulnerabilities
- [ ] Performance acceptable
- [ ] Documentation complete
- [ ] Acceptance criteria met
- [ ] Edge cases handled
- [ ] Error handling robust

---

## Go/No-Go Decision

### Recommendation

**Decision**: [GO|NO-GO|CONDITIONAL]

**Rationale**: [WHY_THIS_DECISION]

**Conditions** (if conditional):
- [CONDITION_1_MUST_BE_MET]
- [CONDITION_2_REQUIRED]

### Blocker Issues

**Blocker 1**: [CRITICAL_ISSUE]
**Impact**: [WHY_THIS_BLOCKS_RELEASE]
**Resolution Required**: [WHAT_MUST_BE_FIXED]

---

## Multi-Agent Consensus

### Audit Agreements

**All agents confirm:**
- [CONSENSUS_FINDING_1]
- [AGREED_ASSESSMENT_2]

### Conflicts

**Issue**: [DISAGREEMENT]
**Resolution**: [FINAL_DECISION]

---

## Evidence References

**Audit Telemetry**: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/[SPEC_ID]/spec-audit_[TIMESTAMP].json`

**Consensus**: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/[SPEC_ID]/spec-audit_synthesis.json`
