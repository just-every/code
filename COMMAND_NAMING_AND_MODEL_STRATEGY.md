# Command Naming & Model Strategy - Standardization Plan

## Problem Statement

**Command naming is inconsistent:**
- Mix of `/spec-*`, `/speckit.*`, bare commands (`/plan`, `/tasks`)
- Unclear which commands are spec-kit vs general Codex
- Confusing `/spec-plan` (multi-agent) vs `/spec-ops-plan` (guardrail)

**Model assignments are arbitrary:**
- Some commands: 1 agent
- Some commands: 3 agents
- Some commands: 5 agents
- No documented rationale

**Result:** Confusion, inconsistency, unclear cost/quality tradeoffs.

---

## Proposed Naming Convention

### Namespace: `/speckit.*`

**Rationale:** Align with GitHub spec-kit, clear namespace separation from Codex commands.

### Categories:

**1. Intake Commands** (generate new SPECs)
- `/speckit.new <description>` - Create SPEC from idea
- `/speckit.clarify <spec-id>` - Resolve ambiguities (port from GitHub)

**2. Stage Commands** (individual stages, multi-agent)
- `/speckit.specify <spec-id>` - Create PRD
- `/speckit.plan <spec-id>` - Create work breakdown
- `/speckit.tasks <spec-id>` - Create task list
- `/speckit.implement <spec-id>` - Write code
- `/speckit.validate <spec-id>` - Run tests
- `/speckit.audit <spec-id>` - Compliance review
- `/speckit.unlock <spec-id>` - Approve for merge

**3. Automation Commands** (orchestrated execution)
- `/speckit.auto <spec-id>` - Full 6-stage pipeline
- `/speckit.resume <spec-id> --from <stage>` - Resume from stage

**4. Quality Commands** (validation, not generation)
- `/speckit.analyze <spec-id>` - Consistency check (port from GitHub)
- `/speckit.checklist <spec-id>` - Requirement quality tests (port from GitHub)

**5. Diagnostic Commands** (read-only status)
- `/speckit.status <spec-id>` - Progress dashboard
- `/speckit.evidence <spec-id>` - Evidence statistics

**6. Guardrail Commands** (bash script execution, keep separate namespace)
- `/guardrail.plan <spec-id>` - Run plan validation
- `/guardrail.tasks <spec-id>` - Run tasks validation
- `/guardrail.implement <spec-id>` - Run implement validation
- `/guardrail.validate <spec-id>` - Run validate harness
- `/guardrail.audit <spec-id>` - Run audit checks
- `/guardrail.unlock <spec-id>` - Run unlock validation

**Deprecated** (maintain backward compat for 1 release):
- `/new-spec` → `/speckit.new`
- `/spec-auto` → `/speckit.auto`
- `/spec-plan` → `/speckit.plan`
- `/spec-ops-*` → `/guardrail.*`

---

## Proposed Model Strategy

### Principle: Match complexity to task

**Single Agent (code):**
- **When:** Simple, deterministic tasks with clear right answer
- **Examples:** `/speckit.new` initial setup, `/speckit.status` reporting
- **Cost:** Low
- **Time:** Fast

**Triple Agent (gemini, claude, code):**
- **When:** Need multiple perspectives but bounded scope
- **Examples:** `/speckit.specify` (PRD), `/speckit.clarify`, `/speckit.analyze`
- **Cost:** Medium
- **Time:** Medium
- **Why 3:** Research (gemini) + Synthesis (claude) + Validation (code)

**Quad Agent (gemini, claude, gpt_pro, gpt_codex):**
- **When:** Complex planning or implementation
- **Examples:** `/speckit.plan`, `/speckit.tasks`, `/speckit.implement`
- **Cost:** High
- **Time:** Slow
- **Why 4:** Research + Synthesis + Code Generation + High-reasoning Validation

**Penta Agent (all 5):**
- **When:** Full pipeline requiring all capabilities
- **Examples:** `/speckit.auto` only
- **Cost:** Highest
- **Time:** Slowest
- **Why 5:** Maximum coverage, arbiter available for conflicts

---

## Detailed Command Matrix

| Command | Agents | Rationale | Est Time | Est Cost |
|---------|--------|-----------|----------|----------|
| **Intake** ||||
| `/speckit.new` | code | Simple scaffolding, SPEC-ID generation | 2-5 min | $0.10 |
| `/speckit.clarify` | gemini, claude, code | Ambiguity detection needs multiple perspectives | 5-10 min | $0.50 |
| **Stages** ||||
| `/speckit.specify` | gemini, claude, code | PRD quality benefits from multi-perspective | 8-12 min | $0.80 |
| `/speckit.plan` | gemini, claude, gpt_pro, gpt_codex | Work breakdown is complex, needs code expertise | 10-15 min | $1.50 |
| `/speckit.tasks` | gemini, claude, gpt_pro, gpt_codex | Task decomposition needs implementation perspective | 10-15 min | $1.50 |
| `/speckit.implement` | gemini, claude, gpt_pro, gpt_codex | Code generation + validation critical | 15-20 min | $2.00 |
| `/speckit.validate` | gemini, claude, gpt_pro | Test strategy needs multiple views | 8-12 min | $1.00 |
| `/speckit.audit` | gemini, claude, gpt_pro | Compliance review needs thoroughness | 8-12 min | $1.00 |
| `/speckit.unlock` | gemini, claude, gpt_pro | Final approval needs consensus | 6-10 min | $0.80 |
| **Automation** ||||
| `/speckit.auto` | all 5 | Full pipeline, arbiter needed for conflicts | 60-90 min | $10-15 |
| `/speckit.resume` | all 5 | Same as auto (partial pipeline) | Varies | Varies |
| **Quality** ||||
| `/speckit.analyze` | gemini, claude, code | Cross-artifact analysis needs synthesis | 5-8 min | $0.50 |
| `/speckit.checklist` | claude, code | Quality testing needs precision | 3-5 min | $0.30 |
| **Diagnostic** ||||
| `/speckit.status` | none | Native TUI, no agents | <1 sec | $0 |
| `/speckit.evidence` | none | File system query | <1 sec | $0 |
| **Guardrails** ||||
| `/guardrail.*` | varies | Policy checks (gpt-5-codex, gpt-5) per guardrail | 6-10 min | $0.80 |

**Total pipeline cost:** ~$15 for full /speckit.auto run

---

## Model Assignment Rules

### Rule 1: Research-First Tasks → Include Gemini
- Tasks needing broad exploration
- Finding edge cases, dependencies
- Surveying codebase

**Commands:** specify, plan, tasks, implement, validate, audit, clarify, analyze

### Rule 2: Synthesis-Heavy Tasks → Include Claude
- Consolidating multiple inputs
- Creating structured outputs
- Precise, concise documentation

**Commands:** All multi-agent commands

### Rule 3: Code Generation → Include GPT-Codex
- Writing code, tests, diffs
- Technical implementation

**Commands:** plan, tasks, implement (only stages touching code)

### Rule 4: High-Stakes Decisions → Include GPT-Pro
- Final validation before advancement
- Arbiter for conflict resolution
- Compliance checks

**Commands:** plan, tasks, implement, validate, audit, unlock, auto

### Rule 5: Simple Operations → Code Only
- Deterministic operations
- Scaffolding, ID generation
- File system queries

**Commands:** new (initial setup), status, evidence

---

## Migration Plan

### Phase 1: Add Aliases (Backward Compat)

**Week 1:**
- Add `/speckit.*` as aliases to existing commands
- Keep `/spec-*` and `/new-spec` working
- Update docs to recommend new names
- Add deprecation warnings to old commands

### Phase 2: Model Standardization

**Week 2:**
- Audit each command's current agent assignment
- Apply rules above
- Update all subagent configs
- Test cost/quality impact

### Phase 3: Remove Old Names

**Week 3:**
- Deprecation period complete
- Remove `/spec-*` old names
- Only `/speckit.*` and `/guardrail.*` remain
- Update all documentation

---

## Testing Strategy

**Before migration:**
```bash
# Test each command with current agents
/new-spec Test pre-migration → Measure: time, cost, quality
/spec-plan SPEC-ID → Measure: time, cost, quality
```

**After migration:**
```bash
# Test with standardized agents
/speckit.new Test post-migration
/speckit.plan SPEC-ID
```

**Compare:**
- Time change
- Cost change
- Output quality (subjective but important)
- Agent utilization (were all models needed?)

**Goal:** Validate model assignments improve quality without excessive cost.

---

## Cost Management

**Current (unoptimized):**
- /spec-auto: ~$15-20 (5 agents × 6 stages)
- Individual stages: $0.50-2.00

**Optimized (with rules):**
- /speckit.auto: ~$10-12 (right-sized agents per stage)
- Individual stages: $0.30-1.50

**Savings:** 30-40% by not over-using expensive models

---

## Implementation Checklist

### Config Changes
- [ ] Add `/speckit.*` alias definitions
- [ ] Update model assignments per rules
- [ ] Test each command
- [ ] Document cost per command

### Code Changes
- [ ] Add SlashCommand enum variants (SpecKitNew, SpecKitPlan, etc.)
- [ ] Update routing in app.rs
- [ ] Add deprecation warnings to old commands
- [ ] Update help text

### Documentation
- [ ] Update product-requirements.md with command list
- [ ] Update CLAUDE.md with new names
- [ ] Add migration guide
- [ ] Document model strategy rationale

### Testing
- [ ] Test all `/speckit.*` commands
- [ ] Verify backward compat with `/spec-*`
- [ ] Measure cost/time per command
- [ ] Validate quality maintained

---

## Example: /clarify Command (New)

**Purpose:** Identify and resolve spec ambiguities

**Agents:** gemini (find ambiguities), claude (prioritize questions), code (format output)

**Config:**
```toml
[[subagents.commands]]
name = "clarify"
agents = ["gemini", "claude", "code"]
orchestrator-instructions = """
Scan spec.md for ambiguities in 9 categories:
1. Functional Scope
2. Data Model
3. UX Flow
4. Non-Functionals
5. Integrations
6. Edge Cases
7. Constraints
8. Terminology
9. Completion Signals

Max 5 questions. Present one at a time with:
- Recommended option
- Rationale
- Impact

Update spec.md ## Clarifications section with answers.
"""
```

**Usage:**
```bash
/speckit.clarify SPEC-KIT-040
→ Agents scan spec
→ Present questions
→ User answers
→ Spec updated with clarifications
```

---

## Decision Point

**Option A: Full standardization now**
- Rename all commands
- Reassign models per rules
- Port clarify/analyze/checklist
- **Effort:** 1 week

**Option B: Incremental**
- Port new commands first (clarify, analyze, checklist)
- Test model strategy on those
- Migrate existing commands if successful
- **Effort:** 2 weeks (safer)

**Which approach?**