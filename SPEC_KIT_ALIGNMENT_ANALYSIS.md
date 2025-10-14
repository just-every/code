# Spec-Kit Alignment Analysis: GitHub vs Local Implementation

## Executive Summary

**Your implementation is NOT aligned with GitHub spec-kit.** It's a **heavily customized multi-agent fork** with different philosophy, architecture, and capabilities.

**Similarity:** ~30% (command names, general workflow stages)
**Divergence:** ~70% (multi-agent, guardrails, evidence, automation)

---

## Comparison Matrix

| Aspect | GitHub Spec-Kit | Your Implementation | Alignment |
|--------|-----------------|---------------------|-----------|
| **Philosophy** | Intent-driven, executable specs | Evidence-driven, multi-agent consensus | ❌ Different |
| **Agents** | Single agent (user's choice) | 5 agents (gemini, claude, gpt_pro, gpt_codex, code) | ❌ Multi vs single |
| **Commands** | `/speckit.constitution`, `/speckit.specify`, `/speckit.plan`, `/speckit.tasks`, `/speckit.implement` | `/new-spec`, `/spec-auto`, `/spec-plan`, `/spec-tasks`, `/spec-ops-*` | ⚠️ Partial (different naming) |
| **Templates** | Static markdown templates in `templates/` | Dynamic generation via agent prompts | ❌ Different approach |
| **Workflow** | Manual progression through stages | Automated 6-stage pipeline with auto-advancement | ❌ Manual vs automated |
| **Validation** | None (trust agent output) | Guardrails, policy checks, baseline audits, HAL validation | ❌ Extensive vs none |
| **Evidence** | None | Telemetry JSON, consensus synthesis, audit trails | ❌ Extensive vs none |
| **Conflict Resolution** | None (single agent) | Automatic arbiter, majority voting | ❌ Multi-agent feature |
| **File Structure** | User scenarios, P1/P2/P3 priorities, success criteria | Context, objectives, acceptance criteria, tasks reference | ⚠️ Similar concepts, different format |

---

## Detailed Comparison

### 1. Command Structure

**GitHub:**
```
/speckit.constitution → Define project principles
/speckit.specify → Create feature spec
/speckit.plan → Technical planning
/speckit.tasks → Generate task list
/speckit.implement → Execute tasks
```

**Yours:**
```
/new-spec → Generate SPEC-ID + PRD (multi-agent)
/spec-auto → Run all 6 stages automatically
/spec-plan, /spec-tasks, etc. → Individual stages
/spec-ops-* → Guardrail execution
/spec-status → Progress dashboard
```

**Deviation:** Different naming convention, added automation layer, added guardrails

---

### 2. Template Format

**GitHub spec-template.md:**
```markdown
# Feature: [Name]
Branch: [branch-name]
Created: [date]
Status: [Draft/In Progress/Done]

## User Scenarios
### P1: [Priority 1 Story]
**Story:** As a [user], I want [goal]...
**Priority Rationale:** [why P1]
**Testability:** [how to verify]
**Acceptance Scenarios:**
- Given [context], when [action], then [outcome]

### P2: [Priority 2 Story]
...

## Edge Cases
- [boundary condition 1]
- [error scenario 1]

## Requirements
### Functional Requirements
- FR1: [requirement]
...

## Success Criteria
- [measurable outcome 1]
- [metric 2]
```

**Your spec.md format:**
```markdown
# Spec: [Title] (T##)

## Context
- [Background info]
- [Problem statement]

## Objectives
1. [Objective 1]
2. [Objective 2]

## Scope
- [In scope]
- [Out of scope]

## Acceptance Criteria
- [Criterion 1]
- [Criterion 2]

## Tasks
See tasks.md for breakdown
```

**Deviation:** Simpler format, no user scenarios, no P1/P2/P3 prioritization, tasks externalized

---

### 3. Plan Structure

**GitHub plan-template.md:**
```markdown
# [Feature] Implementation Plan

## Design
[Technical design details]

## Data Model
[Schema/entities]

## API Contracts
[Endpoints, signatures]

## Research
### Unknowns
- [Question 1]
### Technology Choices
- [Decision 1]
```

**Your plan.md format:**
```markdown
# Plan: [Title]
## Inputs
- Spec: [path + hash]
- Constitution: [path + hash]

## Work Breakdown
1. [Step 1]
2. [Step 2]

## Acceptance Mapping
| Requirement | Validation Step | Test Artifact |
|-------------|-----------------|---------------|
| R1: ... | Test command | Expected output |

## Risks & Unknowns
- [Risk 1]

## Consensus & Risks (Multi-AI)
- Agreement: [what agents agreed on]
- Disagreement & resolution: [conflicts + how resolved]

## Exit Criteria (Done)
- [Criterion 1]
```

**Deviation:** Multi-agent consensus section, acceptance mapping table, evidence references vs design docs

---

### 4. Tasks Structure

**GitHub tasks-template.md:**
```markdown
# Tasks

## Phase 1: Setup
- [ ] T001 [P] Initialize project structure (.gitignore, README)
- [ ] T002 Install dependencies
- [ ] T003 Setup dev environment

## Phase 2: Foundations
- [ ] T004 [Story: P1] Implement core data model
- [ ] T005 [Story: P1] Create API endpoints

## Phase 3: User Stories
### P1: High Priority
- [ ] T006 [Story: P1] Feature X implementation
...

## Phase 4: Polish
- [ ] T020 Documentation
- [ ] T021 Tests
```

**Your tasks.md format:**
```markdown
# Tasks: [Title]

| Order | Task | Owner | Status | Validation |
|-------|------|-------|--------|------------|
| 1 | Task description | Code | Pending | Test command |
| 2 | Another task | Code | Done | Evidence path |

Notes:
- [Additional guidance]
```

**Deviation:** Table format vs checkbox list, no phase grouping, no story labels, validation column instead of testability inline

---

### 5. Constitution

**GitHub constitution template:**
```markdown
# Project Constitution

## Core Principles
1. [Principle 1]
2. [Principle 2]

## Coding Standards
- [Standard 1]

## Architecture Decisions
- [Decision 1]

## Quality Standards
- [Metric 1]
```

**Your constitution:**
```markdown
# Code Spec-Kit Constitution

## Core Principles
### Evidence-Driven Templates
- Keep acceptance criteria, task mappings, guardrail docs in sync

### Cross-Repo Separation
- Shared tooling here, project-specific elsewhere

### Tooling Discipline
- Use MCP/LLM tooling first
- All operations via TUI slash commands

## Governance & Workflow
- SPEC.md = canonical tracker
- One In Progress entry per thread
- Dated evidence references
```

**Deviation:** More focused on evidence/tooling/governance, less about coding standards

---

## Fundamental Architectural Differences

### GitHub Spec-Kit: Single-Agent Workflow

```
User → /speckit.specify → Agent writes spec
User → /speckit.plan → Agent writes plan
User → /speckit.tasks → Agent writes tasks
User → /speckit.implement → Agent writes code
```

**No validation, no consensus, trust agent output.**

### Your Spec-Kit: Multi-Agent Consensus with Guardrails

```
User → /new-spec → 3 agents debate PRD → Consensus
User → /spec-auto → Automated pipeline:
  Stage 1: Guardrail validation → 5 agents → Consensus → Arbiter if conflicts
  Stage 2: Guardrail → 5 agents → Consensus → Arbiter
  ... (repeat 4 more times)
  Evidence tracking throughout
```

**Heavy validation, multi-model perspectives, automatic conflict resolution.**

---

## Major Gaps vs GitHub Spec-Kit

**Missing from your implementation:**

1. **User Scenarios (P1/P2/P3 prioritization)**
   - GitHub emphasizes user stories
   - You focus on technical objectives
   - **Impact:** Less user-centric

2. **Static Templates**
   - GitHub has `templates/spec-template.md`, `plan-template.md`, etc.
   - You generate dynamically via agents
   - **Impact:** Less consistency, more flexibility

3. **Checkbox Task Format**
   - GitHub uses `- [ ] T001 Description`
   - You use markdown tables
   - **Impact:** GitHub format better for tracking in markdown editors

4. **Phase-Based Task Organization**
   - GitHub groups: Setup → Foundations → Stories → Polish
   - You use flat numbered lists
   - **Impact:** Less structure for large features

5. **Edge Cases Section**
   - GitHub templates include edge case enumeration
   - You don't have dedicated section
   - **Impact:** Might miss boundary conditions

6. **Success Metrics in Spec**
   - GitHub includes measurable success criteria
   - You have acceptance criteria (different focus)
   - **Impact:** Similar but GitHub more outcome-focused

**Extra in your implementation (not in GitHub):**

1. **Guardrails** - Baseline audits, policy checks, HAL validation
2. **Multi-agent consensus** - 5 models, arbiter resolution
3. **Evidence tracking** - Telemetry JSON, synthesis, audit trails
4. **Automation** - /spec-auto full pipeline
5. **Progress tracking** - /spec-status dashboard
6. **Conflict resolution** - Automatic arbiter agents

---

## Alignment Recommendations

### Option A: Full Alignment (Major Refactor)

**Adopt GitHub structure:**
1. Create `templates/` directory with GitHub templates
2. Rename commands: `/spec-plan` → `/speckit.plan`
3. Change file formats:
   - spec.md → GitHub user scenario format (P1/P2/P3)
   - tasks.md → Checkbox format with phases
   - plan.md → Design/data-model/API format
4. Remove multi-agent consensus (use single agent)
5. Remove guardrails (trust agent output)
6. Remove evidence tracking

**Effort:** 2-3 weeks
**Risk:** HIGH - lose all custom features (multi-agent, validation, automation)
**Result:** Pure GitHub spec-kit replica

---

### Option B: Hybrid (Adopt Format, Keep Features)

**Adopt GitHub templates while keeping multi-agent:**
1. Create `templates/` with GitHub structure
2. Keep multi-agent consensus (non-standard but valuable)
3. Update file generation to follow GitHub format:
   - spec.md includes user scenarios (P1/P2/P3)
   - tasks.md uses checkbox format
   - plan.md adds design/data-model sections
4. Rename commands for clarity: `/speckit.plan` vs `/spec-plan`
5. Keep guardrails as optional enhancement

**Effort:** 1 week
**Risk:** MEDIUM - format changes might break existing SPECs
**Result:** GitHub-compatible with multi-agent enhancement

---

### Option C: Fork Declaration (Stay Divergent)

**Acknowledge this is a different product:**
1. Rename: "Spec-Kit Multi-Agent Framework" (not "spec-kit")
2. Keep all current features
3. Reference GitHub spec-kit as inspiration
4. Document intentional deviations
5. Don't try to align formats

**Effort:** 2 hours (documentation only)
**Risk:** LOW - no code changes
**Result:** Clear positioning as enhanced fork

---

## Recommended Path

**I recommend Option C** for these reasons:

**Your implementation is BETTER in many ways:**
- Multi-agent catches gaps single agent misses
- Guardrails prevent bad specs from advancing
- Evidence tracking enables debugging
- Automation saves manual effort
- Conflict resolution handles disagreements

**GitHub spec-kit is simpler but less robust:**
- Single agent can miss issues
- No validation gates
- No evidence trail
- Manual stage progression
- No conflict handling

**You've built an ENHANCED spec-kit, not a replica.**

**What to do:**
1. Update docs to say "Inspired by GitHub spec-kit"
2. Document intentional enhancements (multi-agent, guardrails, automation)
3. Keep your architecture
4. **Optionally:** Adopt checkbox task format + user scenario format (good ideas from GitHub)

**Quick wins from GitHub format (keep your features):**
- Add P1/P2/P3 user story prioritization to spec.md
- Use checkbox tasks: `- [ ] T001 Description`
- Add "Edge Cases" section to spec.md
- Group tasks by phase (Setup, Stories, Polish)

**Effort:** 4-6 hours to adopt format improvements
**Keep:** Multi-agent, guardrails, automation, evidence

Want me to create templates that merge GitHub format with your multi-agent features?
