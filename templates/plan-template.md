# Plan: [FEATURE_NAME]

**SPEC-ID**: [SPEC_ID]
**Plan Version**: [VERSION]
**Created**: [DATE]

---

## Inputs

**Spec**: docs/[SPEC_ID]/spec.md (hash: [SHA256])
**Constitution**: memory/constitution.md (v[CONSTITUTION_VERSION])
**PRD**: docs/[SPEC_ID]/PRD.md
**Prompt Version**: [PROMPT_VERSION]

---

## Work Breakdown

### Step 1: [FIRST_MAJOR_STEP]

**Description**: [WHAT_NEEDS_TO_BE_DONE]

**Dependencies**: [PREREQUISITES]

**Success Signal**: [HOW_TO_KNOW_ITS_DONE]

**Owner**: [OWNER]

**Estimated Effort**: [TIME_ESTIMATE]

### Step 2: [SECOND_STEP]

**Description**: [DETAILS]

**Dependencies**: Step 1

**Success Signal**: [COMPLETION_CRITERIA]

**Owner**: [OWNER]

**Estimated Effort**: [TIME]

### Step 3: [CONTINUING_STEPS]

...

---

## Technical Design

### Data Model Changes

**Entity**: [ENTITY_NAME]
**Changes**:
- Add field: [FIELD_NAME] ([TYPE])
- Modify: [EXISTING_FIELD] → [NEW_BEHAVIOR]
- Remove: [DEPRECATED_FIELD]

**Migration**: [HOW_TO_MIGRATE_EXISTING_DATA]

### API Contracts

**Endpoint**: [PATH]
**Method**: [GET|POST|PUT|DELETE]
**Request**:
```json
{
  "field": "type"
}
```
**Response**:
```json
{
  "result": "type"
}
```

**Error Cases**:
- 400: [INVALID_INPUT_DESCRIPTION]
- 404: [NOT_FOUND_SCENARIO]
- 500: [SERVER_ERROR_HANDLING]

### Component Architecture

**New Components**:
- [COMPONENT_1]: [PURPOSE]
- [COMPONENT_2]: [RESPONSIBILITY]

**Modified Components**:
- [EXISTING_COMPONENT]: [WHAT_CHANGES]

**Interactions**:
```
[COMPONENT_A] → [COMPONENT_B] → [COMPONENT_C]
```

---

## Acceptance Mapping

| Requirement (from Spec) | Validation Step | Test/Check Artifact |
|-------------------------|-----------------|---------------------|
| R1: [REQUIREMENT] | [COMMAND_TO_RUN] | [EXPECTED_OUTPUT_OR_FILE] |
| R2: [REQUIREMENT] | [TEST_COMMAND] | [ARTIFACT_PATH] |
| R3: [REQUIREMENT] | [VALIDATION_METHOD] | [EVIDENCE_LOCATION] |

---

## Risks & Unknowns

### Risk 1: [RISK_DESCRIPTION]

**Impact**: [HIGH|MEDIUM|LOW]

**Probability**: [HIGH|MEDIUM|LOW]

**Mitigation**: [HOW_TO_ADDRESS]

**Owner**: [WHO_HANDLES_THIS]

### Risk 2: [ANOTHER_RISK]

**Impact**: [LEVEL]

**Mitigation**: [STRATEGY]

---

## Multi-Agent Consensus

### Agreements

**All agents aligned on:**
- [CONSENSUS_POINT_1]
- [CONSENSUS_POINT_2]
- [CONSENSUS_POINT_3]

### Conflicts Resolved

**Issue**: [WHAT_AGENTS_DISAGREED_ABOUT]

**Positions**:
- Gemini: [GEMINI_POSITION]
- Claude: [CLAUDE_POSITION]
- GPT-Pro: [GPT_POSITION]

**Resolution**: [HOW_CONFLICT_WAS_RESOLVED]

**Arbiter Decision** (if applicable): [ARBITER_REASONING]

---

## Exit Criteria

- [ ] All work breakdown steps completed
- [ ] Acceptance mapping validated (all tests pass)
- [ ] Technical design documented
- [ ] Risks mitigated or accepted
- [ ] Evidence artifacts created
- [ ] Consensus synthesis recorded
- [ ] Ready for /speckit.tasks stage

---

## Evidence References

**Plan Consensus**: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/[SPEC_ID]/spec-plan_synthesis.json`

**Telemetry**: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/[SPEC_ID]/spec-plan_[TIMESTAMP].json`

**Agent Outputs**: `.code/agents/[UUID]/result.txt` (gemini, claude, gpt_pro)
