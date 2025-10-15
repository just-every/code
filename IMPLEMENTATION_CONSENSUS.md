# Implementation Consensus - Templates, Commands, and Model Strategy

## Decision: Incremental Validation, Not Big Bang

**Do NOT rename existing commands.**
**Do NOT migrate everything at once.**

**Prove value incrementally:**
1. Templates FIRST → Validate consistency improvement
2. /clarify command → Validate GitHub concept fits our workflow
3. THEN decide on broader migration

**Rationale:** 40 commits invested in current system. Don't throw away without proof new approach is better.

---

## Phase 1: Template Scaffolding (Week 1)

### Goal
Prove templates improve spec quality and consistency.

### Implementation

**Create templates/:**
```
templates/
├── spec-template.md         # GitHub format + our enhancements
├── plan-template.md         # Work breakdown + consensus section
├── tasks-template.md        # Checkbox format + validation column
└── PRD-template.md          # Requirements + evidence tracking
```

**spec-template.md structure:**
```markdown
**SPEC-ID**: [SPEC_ID]
**Feature**: [FEATURE_NAME]
**Status**: Backlog
**Created**: [DATE]
**Branch**: [BRANCH]

**Context**: [BACKGROUND_PROBLEM_STATEMENT]

**User Scenarios**:

### P1: [HIGH_PRIORITY_USER_STORY]
**Story**: As a [USER_TYPE], I want [GOAL] so that [BENEFIT]
**Testability**: [HOW_TO_VERIFY_INDEPENDENTLY]
**Acceptance Scenarios**:
- Given [CONTEXT], when [ACTION], then [OUTCOME]
- Given [CONTEXT], when [ACTION], then [OUTCOME]

### P2: [MEDIUM_PRIORITY_STORY]
...

**Edge Cases**:
- [BOUNDARY_CONDITION_1]
- [ERROR_SCENARIO_1]

**Functional Requirements**:
- FR1: [REQUIREMENT_WITH_MEASURABLE_CRITERIA]
- FR2: [REQUIREMENT]

**Non-Functional Requirements**:
- Performance: [METRIC]
- Security: [CONSTRAINT]

**Success Criteria**:
- [MEASURABLE_OUTCOME_1]
- [METRIC_2]

**Evidence & Validation**:
**Acceptance Tests**: See tasks.md for test mapping
**Telemetry Path**: docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/[SPEC_ID]/
```

**Enhancements over GitHub:**
- Markdown-KV metadata (clearer for models)
- Evidence & validation section
- Telemetry path reference

---

**plan-template.md structure:**
```markdown
# Plan: [FEATURE_NAME]

**SPEC-ID**: [SPEC_ID]
**Plan Version**: [VERSION]
**Created**: [DATE]

**Inputs**:
**Spec**: docs/[SPEC_ID]/spec.md (hash: [SHA256])
**Constitution**: memory/constitution.md (v[VERSION])
**Prompt Version**: [PROMPT_VERSION]

**Work Breakdown**:
1. [STEP_1_DESCRIPTION]
   - Dependencies: [DEPENDENCIES]
   - Success signal: [HOW_TO_KNOW_DONE]
   - Owner: [OWNER]

2. [STEP_2]
   ...

**Technical Design**:
**Data Model Changes**:
- [ENTITY_1]: [CHANGES]

**API Contracts** (if applicable):
- Endpoint: [PATH]
- Method: [HTTP_METHOD]
- Contract: [SIGNATURE]

**Acceptance Mapping**:
| Requirement | Validation Step | Test Artifact |
|-------------|-----------------|---------------|
| R1: [REQ] | [COMMAND] | [EXPECTED_OUTPUT] |

**Risks & Unknowns**:
- [RISK_1]: [MITIGATION]

**Multi-Agent Consensus**:
**Agreements**:
- [WHAT_ALL_AGENTS_AGREED_ON]

**Conflicts Resolved**:
- Issue: [DISAGREEMENT]
- Resolution: [HOW_RESOLVED]

**Exit Criteria**:
- [ ] [CRITERION_1]
- [ ] [CRITERION_2]
```

**Hybrid:** GitHub design sections + your consensus tracking

---

**tasks-template.md structure:**
```markdown
# Tasks: [FEATURE_NAME]

**SPEC-ID**: [SPEC_ID]
**Plan Reference**: docs/[SPEC_ID]/plan.md
**Prompt Version**: [PROMPT_VERSION]

## Phase 1: Setup
- [ ] T001 [P] [PARALLEL_TASK_DESCRIPTION]
- [ ] T002 [SEQUENTIAL_TASK]

## Phase 2: Foundations
- [ ] T003 [Story: P1] [USER_STORY_TASK]
- [ ] T004 [Story: P1] [ANOTHER_STORY_TASK]

## Phase 3: User Stories

### P1: High Priority
- [ ] T005 [Story: P1] Implement [FEATURE_X]
  **Validation**: [TEST_COMMAND]
  **Artifact**: [EVIDENCE_PATH]

### P2: Medium Priority
- [ ] T010 [Story: P2] Implement [FEATURE_Y]
  ...

## Phase 4: Polish
- [ ] T020 Documentation updates
- [ ] T021 Integration tests
- [ ] T022 Evidence archival

**Validation Table** (generated from checkboxes):
| Task ID | Status | Evidence |
|---------|--------|----------|
| T001 | Done | [PATH] |
| T002 | Pending | - |

**Multi-Agent Consensus**:
**Task Coverage**: [COVERAGE_SUMMARY]
**Conflicts Resolved**: [IF_ANY]
```

**Hybrid:** GitHub checkbox format + phase grouping + your validation evidence

---

### Test Plan for Templates

**Baseline (current):**
```bash
/new-spec Add Redis caching
→ Measure: spec.md structure, completeness, clarity
→ Time: ~10 min
```

**With templates:**
```bash
# Update /new-spec to use templates
/new-spec Add Redis caching
→ Compare: structure consistency, completeness
→ Time: Should be same or faster
```

**Success criteria:**
- All required sections present (user scenarios, edge cases, success criteria)
- Consistent structure across SPECs
- Models fill blanks faster than generating from scratch

**If templates DON'T improve quality:** Abort, keep current approach

---

## Phase 2: New Commands (After Templates Validated)

### /clarify Command

**Purpose**: Structured ambiguity resolution (missing from current system)

**Model Strategy**:
```toml
agents = ["gemini", "claude", "code"]
```

**Why 3:**
- **Gemini**: Scan spec, identify ambiguities across 9 categories (breadth)
- **Claude**: Prioritize questions by impact, format clearly (synthesis)
- **Code**: Validate questions are answerable, present to user (execution)

**Not 4-5 agents:** Clarification is bounded scope, doesn't need code generation or high-reasoning arbitration.

---

### /analyze Command

**Purpose**: Cross-artifact consistency validation (missing from current system)

**Model Strategy**:
```toml
agents = ["gemini", "claude", "code"]
```

**Why 3:**
- **Gemini**: Scan spec.md, plan.md, tasks.md for inconsistencies (breadth)
- **Claude**: Synthesize findings, prioritize by severity (analysis)
- **Code**: Format report, suggest fixes (execution)

**Read-only:** No files modified, just reporting.

---

### /checklist Command

**Purpose**: Requirement quality testing (missing from current system)

**Model Strategy**:
```toml
agents = ["claude", "code"]
```

**Why 2:**
- **Claude**: Evaluate requirement quality (completeness, clarity, measurability)
- **Code**: Format checklist, assign CHK IDs

**Not 3:** Gemini unnecessary - no research needed, pure quality evaluation.

---

## Agreed Model Strategy Matrix

| Command | Agents | Rationale | Time | Cost |
|---------|--------|-----------|------|------|
| **Intake** ||||
| /speckit.new | code | Scaffolding, template filling | 5 min | $0.10 |
| /speckit.clarify | gemini, claude, code | Multi-perspective ambiguity scan | 8 min | $0.60 |
| **Stages** ||||
| /speckit.specify | gemini, claude, code | PRD needs multiple perspectives | 10 min | $0.80 |
| /speckit.plan | gemini, claude, gpt_pro | Work breakdown + validation (no code gen needed) | 12 min | $1.20 |
| /speckit.tasks | gemini, claude, gpt_pro | Task decomp + validation (no code gen) | 12 min | $1.20 |
| /speckit.implement | gemini, claude, gpt_codex, gpt_pro | Code gen requires gpt_codex + validation | 18 min | $2.50 |
| /speckit.validate | gemini, claude, gpt_pro | Test strategy needs consensus | 10 min | $1.00 |
| /speckit.audit | gemini, claude, gpt_pro | Compliance review | 10 min | $1.00 |
| /speckit.unlock | gemini, claude, gpt_pro | Final approval | 8 min | $0.80 |
| **Automation** ||||
| /speckit.auto | all 5 when needed | Uses 3-4 per stage, 5 if conflicts | 60 min | $8-12 |
| **Quality** ||||
| /speckit.analyze | gemini, claude, code | Consistency analysis | 6 min | $0.50 |
| /speckit.checklist | claude, code | Quality testing (no research) | 4 min | $0.30 |
| **Diagnostic** ||||
| /speckit.status | none | Native TUI | <1s | $0 |
| **Guardrails** ||||
| /guardrail.* | gpt-5-codex (prefilter), gpt-5 (final) | Per bash script | 8 min | $0.80 |

**Key change from original strategy:**
- **plan/tasks use 3 agents** (not 4) - don't need gpt_codex unless generating code
- **implement uses 4** - only stage that generates code

**Total /speckit.auto cost:** ~$8-12 (not $15-20)
- Most stages: 3 agents
- Implement stage: 4 agents
- Conflicts: +arbiter (gpt_pro already in the set)

---

## Revised Agent Assignment Rules

**Rule 1: Research + Synthesis + Validation = Baseline (3 agents)**
```
gemini (research/breadth) + claude (synthesis/precision) + code (validation/execution)
```

**Use for:** specify, plan, tasks, validate, audit, unlock, clarify, analyze

**Rationale:** Covers all perspectives without over-provisioning

---

**Rule 2: Add GPT-Codex ONLY for Code Generation (4 agents)**
```
gemini + claude + gpt_codex (code gen) + gpt_pro (high-reasoning validation)
```

**Use for:** implement stage only

**Rationale:** Code generation needs specialized model + high-reasoning review

---

**Rule 3: Add GPT-Pro ONLY for High-Stakes Decisions**
```
Already in baseline for validation stages
Promotes to arbiter if conflicts arise
```

**Use for:** plan, tasks, implement, validate, audit, unlock (anything that gates progression)

**Rationale:** gpt_pro does validation in baseline, escalates to arbiter if needed

---

**Rule 4: Single Agent for Deterministic Operations**
```
code only
```

**Use for:** Scaffolding, status checks, file operations

**Rationale:** No consensus needed for mechanical tasks

---

## Implementation Plan (Corrected)

### Week 1: Templates + Test

**Day 1-2: Create templates**
- Port GitHub templates to `templates/`
- Add markdown-KV metadata
- Add consensus sections
- Add evidence tracking

**Day 3: Update /new-spec**
- Load spec-template.md
- Fill placeholders via agents
- Write filled template

**Day 4: Test**
- Create 3 SPECs with templates
- Compare to 3 SPECs without
- Measure: consistency, completeness, time

**Day 5: Decision**
- If better: Continue
- If worse: Abort template approach

---

### Week 2: New Commands (If Templates Succeed)

**Day 1-2: /clarify**
- Port GitHub clarify workflow
- Adapt to multi-agent (gemini scan, claude prioritize, code present)
- Test on existing SPEC with ambiguities

**Day 3: /analyze**
- Port GitHub analyze workflow
- Cross-artifact consistency checking
- Test on SPEC-KIT-040

**Day 4-5: /checklist**
- Port GitHub checklist workflow
- Requirement quality testing
- Test on SPEC-KIT-045

---

### Week 3: Migration Decision (If Commands Succeed)

**Only if both templates AND new commands prove valuable:**

**Option A: Full migration**
- Rename all to `/speckit.*`
- Hard cutover (no backward compat)
- Update all docs in one pass

**Option B: No migration**
- Keep current names
- Add new commands under `/speckit.*`
- Live with mixed naming

**Defer decision until evidence of value.**

---

## Agreed Model Strategy (Final)

### Complexity-Based Assignment

**Tier 1: Mechanical (1 agent - code)**
- SPEC-ID generation
- Template filling (no synthesis)
- Status queries
- File operations

**Tier 2: Analytical (3 agents - gemini, claude, code)**
- PRD creation (specify)
- Ambiguity resolution (clarify)
- Consistency analysis (analyze)
- Planning (plan)
- Task decomposition (tasks)
- Test strategy (validate)
- Compliance review (audit)
- Final approval (unlock)

**Tier 3: Generative (4 agents - +gpt_codex, +gpt_pro)**
- Code generation (implement)
- Architecture design (if heavy technical planning)

**Tier 4: Full Pipeline (dynamic 3-5)**
- /speckit.auto uses Tier 2 (3 agents) for most stages
- Escalates to Tier 3 (4 agents) for implement
- Adds arbiter (from existing agents) if conflicts

### Reasoning Modes (Per Agent)

**Gemini**:
- Model: gemini-2.5-pro
- Mode: thinking (extended reasoning)
- Budget: 0.6
- Use: Research, breadth, edge case discovery

**Claude**:
- Model: claude-4.5-sonnet
- Mode: auto (standard)
- Temperature: 0.3 (precise synthesis)
- Use: Consolidation, structured output, tight docs

**GPT-Pro** (when needed):
- Model: gpt-5
- Mode: high reasoning
- Use: Validation, arbitration, high-stakes decisions

**GPT-Codex** (implement only):
- Model: gpt-5-codex
- Mode: high reasoning
- Use: Code generation, diffs, technical implementation

**Code** (baseline):
- Model: gpt-5-codex
- Mode: medium reasoning
- Use: Execution, formatting, mechanical tasks

---

## Cost Analysis (Measured Estimates)

**Based on session observations:**

**Per Stage (Tier 2 - 3 agents):**
- Guardrail: 8 min (policy checks)
- Agents: 8 min (parallel gemini, claude, code)
- **Total: ~16 min, ~$1.20/stage**

**Implement Stage (Tier 3 - 4 agents):**
- Guardrail: 8 min
- Agents: 12 min (parallel gemini, claude, gpt_codex, gpt_pro)
- **Total: ~20 min, ~$2.50**

**Full Pipeline:**
- 5 Tier-2 stages: 5 × $1.20 = $6.00
- 1 Tier-3 stage: 1 × $2.50 = $2.50
- Policy checks (6 stages): 6 × $0.40 = $2.40
- **Total: ~$11/pipeline** (60 min with parallel execution)

**Previous estimate ($15-20) was high** - doesn't account for parallelization.

---

## Validation Metrics

### Template Success Criteria

**Must achieve:**
- [ ] 100% of generated specs include user scenarios
- [ ] 100% of specs include edge cases section
- [ ] 100% of specs include success criteria
- [ ] Spec structure identical across 3 test runs
- [ ] Agent fills template in ≤10 minutes (no slower than current)

**Nice to have:**
- [ ] Spec quality subjectively better (manual review)
- [ ] Fewer clarifications needed downstream

---

### Command Success Criteria

**/clarify:**
- [ ] Identifies real ambiguities (not false positives)
- [ ] Questions are answerable (not vague)
- [ ] Resolves ≥80% of [NEEDS CLARIFICATION] markers
- [ ] Completes in <10 minutes

**/analyze:**
- [ ] Finds real inconsistencies (spec ↔ plan ↔ tasks)
- [ ] Prioritizes findings by severity
- [ ] Report actionable (suggests fixes)
- [ ] Completes in <5 minutes (read-only)

**/checklist:**
- [ ] Evaluates requirement quality accurately
- [ ] Identifies gaps in acceptance criteria
- [ ] Completes in <5 minutes

**Failure threshold:** If any command scores <60% on success criteria, abort that command.

---

## Implementation Start

**Commit to:**
1. Templates (Week 1)
2. /clarify, /analyze, /checklist (Week 2, if templates succeed)
3. Migration decision deferred (Week 3, if commands succeed)

**Model strategy:**
- Tier 2 (3 agents) as baseline
- Tier 3 (4 agents) for implement only
- Proven to work through session testing

**Do NOT:**
- Rename existing commands yet
- Migrate everything at once
- Commit to backward compatibility overhead

**Incremental validation wins. Big bang migrations fail.**

---

## Sign-Off

**Consensus approach:** ✅ Documented above
**Model strategy:** ✅ 3-agent baseline, 4-agent for code gen
**Implementation order:** ✅ Templates → Commands → Migration decision

**Ready to implement Phase 1 (templates)?**
