# Migration Guide: /spec-* → /speckit.* Command Namespace

**Version:** 1.0
**Date:** 2025-10-15
**Status:** Phase 3 standardization complete

---

## Overview

As of Phase 3 (October 2025), all spec-kit commands have been standardized under the `/speckit.*` namespace. This migration guide helps users transition from legacy `/spec-*` commands to the new standardized naming.

**Good news:** All legacy commands still work! This is a non-breaking change with backward compatibility maintained.

---

## Quick Reference Table

### Core Commands

| Old Command | New Command | Status | Notes |
|-------------|-------------|--------|-------|
| `/new-spec` | `/speckit.new` | ✅ Both work | Creates SPEC with templates |
| `/spec-plan` | `/speckit.plan` | ✅ Both work | Multi-agent work breakdown |
| `/spec-tasks` | `/speckit.tasks` | ✅ Both work | Task decomposition |
| `/spec-implement` | `/speckit.implement` | ✅ Both work | Code generation |
| `/spec-validate` | `/speckit.validate` | ✅ Both work | Test strategy |
| `/spec-audit` | `/speckit.audit` | ✅ Both work | Compliance checking |
| `/spec-unlock` | `/speckit.unlock` | ✅ Both work | Final approval |
| `/spec-auto` | `/speckit.auto` | ✅ Both work | Full 6-stage pipeline |
| `/spec-status` | `/speckit.status` | ✅ Both work | Native TUI dashboard |

### New Commands (No Legacy Equivalent)

| Command | Purpose | Tier |
|---------|---------|------|
| `/speckit.specify` | PRD drafting/updates | Tier 2 (3 agents) |
| `/speckit.clarify` | Ambiguity resolution | Tier 2 (3 agents) |
| `/speckit.analyze` | Consistency checking | Tier 2 (3 agents) |
| `/speckit.checklist` | Quality scoring | Tier 2-lite (2 agents) |

### Guardrail Commands (Unchanged)

| Command | Purpose | Notes |
|---------|---------|-------|
| `/spec-ops-plan` | Plan validation | Shell wrapper |
| `/spec-ops-tasks` | Tasks validation | Shell wrapper |
| `/spec-ops-implement` | Implementation checks | Shell wrapper |
| `/spec-ops-validate` | Test execution | Shell wrapper |
| `/spec-ops-audit` | Compliance scan | Shell wrapper |
| `/spec-ops-unlock` | Final validation | Shell wrapper |
| `/spec-ops-auto` | Full pipeline wrapper | Shell wrapper |
| `/spec-evidence-stats` | Evidence monitoring | Utility |
| `/spec-consensus` | Consensus inspection | Utility |

**Note:** Guardrail commands (`/spec-ops-*`) will be renamed to `/guardrail.*` in Phase 3 Week 2.

---

## Migration Path

### Option 1: Gradual Migration (Recommended)

Start using `/speckit.*` commands in new workflows while legacy commands continue working:

```bash
# Old workflow (still works)
/new-spec Add user authentication
/spec-auto SPEC-KIT-###

# New workflow (recommended)
/speckit.new Add user authentication
/speckit.auto SPEC-KIT-###
```

**Advantage:** No forced changes, migrate at your own pace.

### Option 2: Immediate Switch

Update all documentation, scripts, and workflows to use `/speckit.*` immediately:

1. Search codebase for `/new-spec`, `/spec-plan`, etc.
2. Replace with `/speckit.*` equivalents
3. Update CLAUDE.md and team documentation
4. Test workflows end-to-end

**Advantage:** Clean break, consistent naming from day one.

### Option 3: Wait for Deprecation

Continue using legacy commands until they are formally deprecated (future release):

**Advantage:** Zero effort now, migrate when required.

---

## Command-by-Command Migration

### Creating a New SPEC

**Old:**
```bash
/new-spec Add webhook notification system for task completion
```

**New:**
```bash
/speckit.new Add webhook notification system for task completion
```

**Improvements in new version:**
- Uses GitHub-inspired templates (55% faster)
- Tier 2 agents (gemini, claude, code)
- ~13 min, ~$0.60

---

### Running Individual Stages

**Old:**
```bash
/spec-plan SPEC-KIT-065
/spec-tasks SPEC-KIT-065
/spec-implement SPEC-KIT-065
/spec-validate SPEC-KIT-065
/spec-audit SPEC-KIT-065
/spec-unlock SPEC-KIT-065
```

**New:**
```bash
/speckit.plan SPEC-KIT-065
/speckit.tasks SPEC-KIT-065
/speckit.implement SPEC-KIT-065
/speckit.validate SPEC-KIT-065
/speckit.audit SPEC-KIT-065
/speckit.unlock SPEC-KIT-065
```

**Improvements:**
- Tiered model strategy (right-sized agents)
- 40% cost reduction ($15→$11 per pipeline)
- Clearer namespace separation

---

### Full Automation

**Old:**
```bash
/spec-auto SPEC-KIT-065
```

**New:**
```bash
/speckit.auto SPEC-KIT-065
```

**Same functionality:**
- Full 6-stage pipeline
- Automatic conflict resolution
- ~60 min, ~$11
- Dynamic 3-5 agents

---

### Status Checking

**Old:**
```bash
/spec-status SPEC-KIT-065
```

**New:**
```bash
/speckit.status SPEC-KIT-065
```

**Improvements:**
- Native Rust implementation (Tier 0)
- <1s response time
- $0 cost (no agents)

---

## New Quality Commands

These commands have **no legacy equivalent** and are new in Phase 3:

### Clarify Ambiguities

```bash
/speckit.clarify SPEC-KIT-065
```

**Purpose:** Identify and resolve requirement ambiguities
**Agents:** 3 (gemini, claude, code)
**Time:** ~8 min
**Cost:** ~$0.80

### Analyze Consistency

```bash
/speckit.analyze SPEC-KIT-065
```

**Purpose:** Check cross-artifact consistency (PRD ↔ plan ↔ tasks), auto-fix issues
**Agents:** 3 (gemini, claude, code)
**Time:** ~8 min
**Cost:** ~$0.80

### Check Quality

```bash
/speckit.checklist SPEC-KIT-065
```

**Purpose:** Score requirement quality (testability, clarity, completeness)
**Agents:** 2 (claude, code)
**Time:** ~5 min
**Cost:** ~$0.35

### Draft/Update PRD

```bash
/speckit.specify SPEC-KIT-065 Updated requirements
```

**Purpose:** Draft or update PRD with multi-agent analysis
**Agents:** 3 (gemini, claude, code)
**Time:** ~10 min
**Cost:** ~$0.80

---

## Example Workflows

### Old Workflow

```bash
# Create SPEC
/new-spec Add search autocomplete with fuzzy matching

# Run full automation
/spec-auto SPEC-KIT-070

# Check status
/spec-status SPEC-KIT-070

# Evidence footprint
/spec-evidence-stats --spec SPEC-KIT-070
```

### New Workflow (Recommended)

```bash
# Create SPEC
/speckit.new Add search autocomplete with fuzzy matching

# Quality checks (new!)
/speckit.clarify SPEC-KIT-070
/speckit.analyze SPEC-KIT-070
/speckit.checklist SPEC-KIT-070

# Run full automation
/speckit.auto SPEC-KIT-070

# Check status (instant)
/speckit.status SPEC-KIT-070

# Evidence footprint
/spec-evidence-stats --spec SPEC-KIT-070
```

**Benefits of new workflow:**
- Proactive quality checks before automation
- Catch ambiguities and inconsistencies early
- Reduce rework in later stages
- Same total cost, better outcomes

---

## Benefits of Migration

### 1. Consistent Naming
- All spec-kit commands under `/speckit.*` namespace
- Clear separation from general Codex commands
- Follows GitHub spec-kit convention

### 2. Right-Sized Agents (Tiered Strategy)
- **Tier 0** (Native): 0 agents, instant, $0
- **Tier 2-lite** (Dual): 2 agents, 5 min, $0.35
- **Tier 2** (Triple): 3 agents, 8-12 min, $0.80-1.00
- **Tier 3** (Quad): 4 agents, 15-20 min, $2.00
- **Tier 4** (Dynamic): 3-5 agents, 60 min, $11

**Result:** 40% cost reduction ($15→$11 per pipeline)

### 3. Template System
- GitHub-inspired spec/PRD/plan/tasks templates
- 55% faster generation (13 min vs 30 min)
- Consistent structure across all SPECs

### 4. Quality Commands
- Proactive issue detection (clarify, analyze, checklist)
- Catch problems before they propagate
- Lower rework costs

### 5. Performance
- Native status queries (<1s vs API calls)
- Parallel agent spawning (30% faster)
- Context caching (reduces redundant reads)

---

## Timeline

**Phase 3 Week 1** (October 2025):
- ✅ All `/speckit.*` commands functional
- ✅ Backward compatibility maintained
- ✅ Migration guide published (this document)

**Phase 3 Week 2** (Planned):
- [ ] Guardrail namespace: `/spec-ops-*` → `/guardrail.*`
- [ ] Deprecation warnings added to legacy commands
- [ ] Final testing and release notes

**Future Release:**
- [ ] Remove legacy `/spec-*` enum variants
- [ ] Migration becomes mandatory
- [ ] Breaking change announcement

**Recommendation:** Migrate now to avoid forced migration later.

---

## Frequently Asked Questions

### Q: Do I have to migrate immediately?
**A:** No. Legacy commands continue to work. Migrate at your own pace.

### Q: Will my old commands break?
**A:** No. Backward compatibility is maintained. Both old and new commands work identically.

### Q: What's the benefit of migrating?
**A:** Consistent naming, access to new quality commands, future-proofing against deprecation.

### Q: When will legacy commands be removed?
**A:** Not yet decided. Deprecation warnings will be added first, with advance notice before removal.

### Q: What about my scripts and automation?
**A:** They continue to work. Update at your convenience. Consider switching to `/speckit.*` for new scripts.

### Q: What about `/spec-ops-*` commands?
**A:** They are separate (guardrail layer). Planned rename to `/guardrail.*` in Phase 3 Week 2.

### Q: How do I update my team's documentation?
**A:** Replace `/spec-*` references with `/speckit.*` equivalents. See Quick Reference Table above.

### Q: Are there any functional differences?
**A:** No functional changes. Same orchestration, same agents, same outputs. Only naming changed.

### Q: What about config files?
**A:** Config stays the same. Commands route to same implementations. No config changes needed.

### Q: Can I use both old and new commands in the same workflow?
**A:** Yes. They are interchangeable. Mix and match as needed during migration.

---

## Getting Help

**Documentation:**
- CLAUDE.md - Full command reference
- product-requirements.md - Current features
- PLANNING.md - Architecture details

**Status Check:**
```bash
/speckit.status SPEC-KIT-###
```

**Evidence Review:**
```bash
/spec-evidence-stats --spec SPEC-KIT-###
```

**Issues:** Report at repo issue tracker or ask maintainers

---

## Summary

**Key Takeaways:**
1. ✅ All `/speckit.*` commands functional and ready to use
2. ✅ Legacy `/spec-*` commands still work (backward compatible)
3. ✅ New quality commands available (clarify, analyze, checklist, specify)
4. ✅ 40% cost reduction via tiered model strategy
5. ✅ 55% faster generation via templates
6. ✅ Migrate at your own pace (no forced timeline)

**Recommended Action:**
Start using `/speckit.*` commands in new workflows today. Gradually update existing scripts and documentation. Enjoy the benefits of consistent naming, right-sized agents, and new quality features.

---

**Document Version:** 1.0
**Last Updated:** 2025-10-15
**Status:** Current and authoritative
**Owner:** @just-every/automation
