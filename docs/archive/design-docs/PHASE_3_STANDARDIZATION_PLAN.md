# Phase 3: Command Standardization - Implementation Plan

**Goal**: Consistent /speckit.* naming and right-sized model strategy across ALL commands

**Duration**: 1-2 weeks
**Commits**: ~20-30
**Risk**: Medium (refactoring working system)

---

## Current State (Pre-Standardization)

**Command inventory (11 commands, 3 namespaces):**

| Current Name | Namespace | Agents | Purpose | Status |
|--------------|-----------|--------|---------|--------|
| /new-spec | none | 3 | SPEC creation | Working |
| /spec-auto | spec | 5 | Full pipeline | Working |
| /spec-plan | spec | 4 | Plan stage | Working |
| /spec-tasks | spec | 3 | Tasks stage | Working |
| /spec-ops-plan | spec-ops | n/a | Guardrail | Working |
| /spec-ops-tasks | spec-ops | n/a | Guardrail | Working |
| /spec-status | spec | native | Status | Working |
| /clarify | none | 3 | Ambiguity | Working |
| /analyze | none | 3 | Consistency | Working |
| /checklist | none | 2 | Quality | Working |
| /specify | none | 1 | PRD only | Working |

**Problems:**
- Inconsistent naming (3 different patterns)
- Unclear which are spec-kit vs general Codex
- Model counts arbitrary (1, 2, 3, 4, or 5 agents)
- No clear strategy

---

## Target State (Post-Standardization)

**Two namespaces only:**

### /speckit.* - All spec-kit commands

| New Name | Agents | Model Strategy | Purpose |
|----------|--------|----------------|---------|
| /speckit.new | code (1) | Tier 1: Scaffolding | Generate SPEC-ID, use templates |
| /speckit.specify | gemini, claude, code (3) | Tier 2: Analysis | PRD with multi-perspective |
| /speckit.clarify | gemini, claude, code (3) | Tier 2: Analysis | Resolve ambiguities |
| /speckit.analyze | gemini, claude, code (3) | Tier 2: Analysis | Consistency checking |
| /speckit.checklist | claude, code (2) | Tier 2-lite: Quality | Requirement evaluation |
| /speckit.plan | gemini, claude, gpt_pro (3) | Tier 2: Analysis | Work breakdown |
| /speckit.tasks | gemini, claude, gpt_pro (3) | Tier 2: Analysis | Task decomposition |
| /speckit.implement | gemini, claude, gpt_codex, gpt_pro (4) | Tier 3: Code gen | Implementation |
| /speckit.validate | gemini, claude, gpt_pro (3) | Tier 2: Analysis | Test strategy |
| /speckit.audit | gemini, claude, gpt_pro (3) | Tier 2: Analysis | Compliance |
| /speckit.unlock | gemini, claude, gpt_pro (3) | Tier 2: Analysis | Approval |
| /speckit.auto | Dynamic 3-5 | Adaptive | Full pipeline |
| /speckit.status | none (native) | Tier 0: Native | Dashboard |

### /guardrail.* - Validation scripts

| New Name | Purpose |
|----------|---------|
| /guardrail.plan | Baseline + policy checks for plan |
| /guardrail.tasks | Validation for tasks |
| /guardrail.implement | Pre-implementation checks |
| /guardrail.validate | Test harness execution |
| /guardrail.audit | Compliance scanning |
| /guardrail.unlock | Final validation |

---

## Model Strategy Tiers

### Tier 0: Native TUI (0 agents)
**Use for:** Status queries, file operations
**Commands:** /speckit.status
**Cost:** $0
**Time:** <1s

### Tier 1: Single Agent (code - 1 agent)
**Use for:** Deterministic scaffolding, template filling
**Commands:** /speckit.new (SPEC-ID generation, directory creation)
**Cost:** $0.05-0.10
**Time:** 1-3 min

### Tier 2-lite: Dual Agent (claude, code - 2 agents)
**Use for:** Quality evaluation, no research needed
**Commands:** /speckit.checklist
**Cost:** $0.25-0.35
**Time:** 3-5 min

### Tier 2: Triple Agent (gemini, claude, code/gpt_pro - 3 agents)
**Use for:** Analysis, planning, consensus (no code generation)
**Commands:** /speckit.specify, /speckit.clarify, /speckit.analyze, /speckit.plan, /speckit.tasks, /speckit.validate, /speckit.audit, /speckit.unlock
**Cost:** $0.80-1.50
**Time:** 8-15 min

### Tier 3: Quad Agent (gemini, claude, gpt_codex, gpt_pro - 4 agents)
**Use for:** Code generation + validation
**Commands:** /speckit.implement only
**Cost:** $2.00-2.50
**Time:** 15-20 min

### Tier 4: Penta Agent (all 5 dynamically)
**Use for:** Full pipeline with arbiter capability
**Commands:** /speckit.auto (uses Tier 2 for most stages, Tier 3 for implement, adds arbiter if conflicts)
**Cost:** $8-12
**Time:** 40-60 min

**Cost optimization:** ~40% savings vs current arbitrary assignments

---

## Implementation Phases

### Week 1: Core Command Renaming

**Day 1: Add /speckit.* aliases**
- Add new enum variants to SlashCommand
- Route to existing implementations
- Keep old names working (backward compat)
- Test all commands work with both names

**Day 2: Update config**
- Rename all [[subagents.commands]]
- Apply tiered model strategy
- Test each command

**Day 3: Update orchestrator**
- /spec-auto instructions reference /speckit.* not /spec-*
- Test full pipeline

**Day 4-5: Documentation**
- Update CLAUDE.md, AGENTS.md, product-requirements.md
- Update all SPEC files referencing old commands
- Add migration guide

### Week 2: Guardrail Separation

**Day 1-2: Rename /spec-ops-***
- /spec-ops-plan → /guardrail.plan
- Update routing
- Test visibility

**Day 3: Clean up**
- Remove old enum variants (break backward compat)
- Update all docs
- Final testing

**Day 4-5: Validation**
- Test full workflow with new names
- Run /speckit.auto on test SPEC
- Document any issues

---

## Testing Strategy

**Per command:**
```bash
# Test alias works
/speckit.new Test standardization
/spec-new Test standardization  # Old name still works

# Verify agent count
bash scripts/spec_ops_004/log_agent_runs.sh 10
# Should show correct tier (3 agents for /speckit.specify, etc.)

# Test functionality unchanged
/speckit.clarify SPEC-KIT-065
# Should work identically to old /clarify
```

**Full pipeline:**
```bash
/speckit.auto SPEC-KIT-090-standardization-test
# Verify all stages use correct agent tiers
# Measure cost (should be ~$11, not $15-20)
```

---

## Migration Guide for Users

**Old → New mapping:**
```bash
# Intake
/new-spec → /speckit.new

# Stages
/specify → /speckit.specify
/spec-plan → /speckit.plan
/spec-tasks → /speckit.tasks
/implement → /speckit.implement
/spec-validate → /speckit.validate
/spec-audit → /speckit.audit
/spec-unlock → /speckit.unlock

# Automation
/spec-auto → /speckit.auto

# Quality
/clarify → /speckit.clarify
/analyze → /speckit.analyze
/checklist → /speckit.checklist

# Diagnostic
/spec-status → /speckit.status

# Guardrails
/spec-ops-plan → /guardrail.plan
/spec-ops-tasks → /guardrail.tasks
/spec-ops-implement → /guardrail.implement
/spec-ops-validate → /guardrail.validate
/spec-ops-audit → /guardrail.audit
/spec-ops-unlock → /guardrail.unlock
```

**Backward compatibility:** Old commands work for 1 release cycle (deprecation warnings).

---

## Success Criteria

### Naming
- [ ] All commands use /speckit.* or /guardrail.* prefix
- [ ] No bare commands (/plan, /tasks, etc.)
- [ ] Consistent with GitHub spec-kit convention
- [ ] Clear namespace separation

### Model Strategy
- [ ] Each command uses correct tier
- [ ] No over-provisioning (5 agents when 3 sufficient)
- [ ] No under-provisioning (1 agent when consensus needed)
- [ ] Cost reduced ~40% vs pre-standardization

### Quality
- [ ] All commands still work
- [ ] Output quality maintained or improved
- [ ] Speed maintained or improved
- [ ] No regressions

### Documentation
- [ ] All docs updated
- [ ] Migration guide complete
- [ ] Examples use new names
- [ ] Old names marked deprecated

---

## Rollback Plan

**If standardization breaks things:**

**Week 1 issues:**
- Revert config changes
- Keep old command names
- Document what failed

**Week 2 issues:**
- More serious - code changes committed
- Revert commits
- Restore old enum variants
- Document lessons learned

**Mitigation:** Test each change before committing, small incremental steps.

---

## Cost Analysis

### Current (Arbitrary)
- /new-spec: 3 agents = ~$0.60
- /specify: 1 agent = ~$0.20 (under-provisioned)
- /spec-auto: 5 agents × 6 stages = ~$15

### After Standardization
- /speckit.new: 1 agent = ~$0.10 (right-sized)
- /speckit.specify: 3 agents = ~$0.80 (properly provisioned)
- /speckit.auto: Tier 2 (3) × 5 stages + Tier 3 (4) × 1 = ~$11

**Savings:** ~30-40% ($15 → $11 per pipeline)

---

## Implementation Start

**Tomorrow (Week 1 Day 1):**
1. Add SlashCommand enum variants (SpecKitNew, SpecKitPlan, etc.)
2. Route to existing handlers (aliases)
3. Test each /speckit.* command
4. Keep /spec-* working (backward compat)

**Commit strategy:**
- Small commits per command
- Test after each
- Easy rollback if issues

**Ready to start Week 1 Day 1 implementation?**
