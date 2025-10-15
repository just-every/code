# Tasks: [FEATURE_NAME]

**SPEC-ID**: [SPEC_ID]
**Plan Reference**: docs/[SPEC_ID]/plan.md
**Prompt Version**: [PROMPT_VERSION]
**Created**: [DATE]

---

## Phase 1: Setup & Prerequisites

- [ ] T001 [P] [PARALLEL_TASK] Initialize project structure
  **Validation**: `[TEST_COMMAND]`
  **Artifact**: `[FILE_OR_EVIDENCE_PATH]`

- [ ] T002 Setup development environment
  **Validation**: `[VERIFICATION_COMMAND]`
  **Artifact**: `[CONFIG_FILE]`

- [ ] T003 [P] Install dependencies
  **Validation**: `[DEPENDENCY_CHECK]`
  **Artifact**: `[LOCK_FILE]`

---

## Phase 2: Foundations

- [ ] T004 [Story: P1] Implement core data model
  **Validation**: `[UNIT_TEST_COMMAND]`
  **Artifact**: `[TEST_OUTPUT]`

- [ ] T005 [Story: P1] Create database schema/migrations
  **Validation**: `[MIGRATION_TEST]`
  **Artifact**: `[MIGRATION_FILES]`

- [ ] T006 [P] Setup testing infrastructure
  **Validation**: `[TEST_HARNESS_CHECK]`
  **Artifact**: `[TEST_CONFIG]`

---

## Phase 3: User Stories

### P1: High Priority

- [ ] T007 [Story: P1] Implement [PRIMARY_FEATURE]
  **User Story**: As a [USER], I want [GOAL]
  **Validation**: `[ACCEPTANCE_TEST]`
  **Artifact**: `[FEATURE_CODE_PATH]`
  **Evidence**: `[TELEMETRY_OR_TEST_RESULTS]`

- [ ] T008 [Story: P1] Add [CRITICAL_CAPABILITY]
  **User Story**: As a [USER], I want [CAPABILITY]
  **Validation**: `[INTEGRATION_TEST]`
  **Artifact**: `[CODE_FILES]`
  **Evidence**: `[TEST_EVIDENCE]`

### P2: Medium Priority

- [ ] T010 [Story: P2] Implement [SECONDARY_FEATURE]
  **User Story**: As a [USER], I want [FEATURE]
  **Validation**: `[TEST_COMMAND]`
  **Artifact**: `[FILE_PATH]`

### P3: Low Priority

- [ ] T015 [Story: P3] Add [NICE_TO_HAVE]
  **User Story**: As a [USER], I want [ENHANCEMENT]
  **Validation**: `[VERIFICATION]`
  **Artifact**: `[PATH]`

---

## Phase 4: Validation & Testing

- [ ] T020 Unit tests for core logic
  **Validation**: `cargo test [MODULE]`
  **Artifact**: Test output showing ≥80% coverage

- [ ] T021 Integration tests for API
  **Validation**: `[INTEGRATION_TEST_SUITE]`
  **Artifact**: `[TEST_RESULTS]`

- [ ] T022 End-to-end testing
  **Validation**: `[E2E_TEST_COMMAND]`
  **Artifact**: `[E2E_EVIDENCE]`

---

## Phase 5: Documentation & Polish

- [ ] T025 Update user documentation
  **Validation**: Doc review + `/speckit.analyze [SPEC_ID]`
  **Artifact**: `docs/[FEATURE_DOCS].md`

- [ ] T026 Update API documentation
  **Validation**: OpenAPI spec validation
  **Artifact**: `[API_SPEC_FILE]`

- [ ] T027 Create troubleshooting guide
  **Validation**: Manual review
  **Artifact**: `docs/troubleshooting/[FEATURE].md`

- [ ] T028 Evidence archival
  **Validation**: `/speckit.status [SPEC_ID]`
  **Artifact**: Evidence checksums, size report

---

## Validation Table

| Task ID | Description | Status | Evidence | Notes |
|---------|-------------|--------|----------|-------|
| T001 | [SHORT_DESC] | Pending | - | - |
| T002 | [SHORT_DESC] | Pending | - | - |
| T003 | [SHORT_DESC] | Pending | - | - |

**Auto-generated from checkboxes above. Update as tasks complete.**

---

## Multi-Agent Consensus

### Task Coverage Analysis

**Acceptance Criteria Coverage**: [PERCENTAGE]% of spec requirements have corresponding tasks

**Gaps Identified**:
- [MISSING_REQUIREMENT_1]
- [UNCOVERED_EDGE_CASE_1]

### Conflicts Resolved

**Issue**: [DISAGREEMENT_ABOUT_APPROACH]

**Positions**:
- Gemini: [RESEARCH_PERSPECTIVE]
- Claude: [SYNTHESIS_PERSPECTIVE]
- GPT-Pro: [VALIDATION_PERSPECTIVE]

**Resolution**: [HOW_RESOLVED]

---

## Dependencies

**Internal**:
- Task [T###] depends on Task [T###]
- [COMPONENT_A] must be complete before [COMPONENT_B]

**External**:
- [LIBRARY_OR_SERVICE_1]
- [API_OR_INTEGRATION_2]

**Blockers**:
- [CURRENT_BLOCKER_1]
- [WAITING_ON_2]

---

## Notes

**Parallel Execution**: Tasks marked [P] can run concurrently

**Story Labels**: [Story: P1] indicates user story implementation from spec.md

**Evidence**: Each task validation produces artifact for audit trail

**Update Frequency**: Check task status daily during /speckit.auto execution
