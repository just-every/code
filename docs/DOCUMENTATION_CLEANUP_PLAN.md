# Documentation Cleanup Plan

**Status**: Action plan created 2025-10-19
**Problem**: 250 markdown files with significant sprawl (30+ stale session docs)
**Goal**: Consolidate to ~50 essential docs, archive the rest

---

## Current State

**Total Project Markdown Files**: 250 (excluding dependencies/evidence)

**Breakdown**:
- Root level: 30 files (HIGH SPRAWL)
- docs/: 24 core docs
- docs/spec-kit/: 45 files (SEVERE SPRAWL - many session notes)
- docs/SPEC-KIT-*/: ~150 spec directories (feature specs, keep)
- codex-rs/: 2 files (REVIEW.md, ARCHITECTURE-TASKS.md)

---

## Cleanup Strategy

### Phase 1: Archive Stale Session Documents (IMMEDIATE)

**Create Archive Structure**:
```bash
mkdir -p docs/archive/2025-sessions
mkdir -p docs/archive/design-docs
mkdir -p docs/archive/completed-specs
```

**Root Level → Archive** (12 files to move):
```bash
# Session summaries and handoffs
mv SESSION-HANDOFF-2025-10-18.md docs/archive/2025-sessions/
mv agent_execution_log_*.md docs/archive/2025-sessions/  # 5 files
mv telemetry-tasks.md docs/archive/2025-sessions/
mv CONFIG_FIX_SUMMARY.md docs/archive/2025-sessions/
mv RESTART.md docs/archive/2025-sessions/
mv output.md docs/archive/2025-sessions/
mv plan.md docs/archive/2025-sessions/  # Nearly empty (27 bytes)

# Design docs (completed features)
mv COMMAND_NAMING_AND_MODEL_STRATEGY.md docs/archive/design-docs/
mv PHASE_3_STANDARDIZATION_PLAN.md docs/archive/design-docs/
mv SPEC_AUTO_ORCHESTRATOR_DESIGN.md docs/archive/design-docs/
mv OPTIMIZATION_ANALYSIS.md docs/archive/design-docs/
mv model.md docs/archive/design-docs/
```

**docs/spec-kit/ → Archive** (16+ files to move):
```bash
# Session summaries
mv docs/spec-kit/SESSION_SUMMARY_2025-10-16.md docs/archive/2025-sessions/
mv docs/spec-kit/SESSION_RESUME_T78.md docs/archive/2025-sessions/
mv docs/spec-kit/EPIC_SESSION_SUMMARY_2025-10-16.md docs/archive/2025-sessions/
mv docs/spec-kit/ARCHITECTURE_COMPLETE_2025-10-16.md docs/archive/2025-sessions/
mv docs/spec-kit/REVIEW_COMPLETION_ANALYSIS.md docs/archive/2025-sessions/

# Refactoring session notes
mv docs/spec-kit/REFACTORING_SESSION_NOTES.md docs/archive/2025-sessions/
mv docs/spec-kit/REFACTORING_BLOCKER.md docs/archive/2025-sessions/
mv docs/spec-kit/REFACTORING_BLOCKER_RESOLUTION.md docs/archive/2025-sessions/
mv docs/spec-kit/REFACTORING_CONTINUATION.md docs/archive/2025-sessions/
mv docs/spec-kit/REFACTORING_COMPLETE_SUMMARY.md docs/archive/2025-sessions/
mv docs/spec-kit/REFACTORING_FINAL_STATUS.md docs/archive/2025-sessions/

# Completed phase docs
mv docs/spec-kit/PHASE_1_COMPLETE.md docs/archive/2025-sessions/
mv docs/spec-kit/PHASE_1_FINAL_STEPS.md docs/archive/2025-sessions/
mv docs/spec-kit/PHASE_3_DAY_4_TESTING_PLAN.md docs/archive/2025-sessions/

# Design docs (completed)
mv docs/spec-kit/REFACTORING_PLAN.md docs/archive/design-docs/
mv docs/spec-kit/PHASE_2_EXTRACTION_PLAN.md docs/archive/design-docs/
mv docs/spec-kit/STEP_1.4_HANDLER_EXTRACTION.md docs/archive/design-docs/
mv docs/spec-kit/SERVICE_TRAITS_DEEP_ANALYSIS.md docs/archive/design-docs/
mv docs/spec-kit/REMAINING_OPPORTUNITIES.md docs/archive/design-docs/
```

**Expected Result**: 28+ files archived, root level reduced from 30 → 18 files

---

## Phase 2: Keep Essential Documents Only

### Root Level (KEEP - 18 files)

**Authoritative**:
- ✅ SPEC.md - Task tracker, single source of truth
- ✅ CLAUDE.md - Operating guide (UPDATED 2025-10-19)
- ✅ AGENTS.md - Agent orchestration
- ✅ PLANNING.md - Architecture
- ✅ product-requirements.md - Product scope
- ✅ README.md - Project overview
- ✅ CHANGELOG.md - Release history

**Specifications**:
- ✅ SPEC_AUTO_FLOW.md - Pipeline flow
- ✅ IMPLEMENTATION_CONSENSUS.md - Implementation details
- ✅ SPEC_KIT_ALIGNMENT_ANALYSIS.md - Alignment analysis
- ✅ SPEC-KIT.md - Spec-kit overview
- ✅ TUI.md - TUI documentation
- ✅ FORK_DEVIATIONS.md - Fork-specific changes

**Analysis** (Could Archive):
- ⚠️ AGENT_ANALYSIS_GUIDE.md
- ⚠️ AGENT_FIX_ANALYSIS.md
- ⚠️ SPEC_OPS_TUI_IMPLEMENTATION.md

### docs/spec-kit/ (KEEP - 15-20 files)

**Current Policies** (KEEP):
- ✅ testing-policy.md
- ✅ evidence-policy.md
- ✅ TESTING_INFRASTRUCTURE.md
- ✅ CONFLICT_RESOLUTION.md
- ✅ PHASE3_TEST_PLAN.md (just created 2025-10-19)
- ✅ PHASE4_TEST_PLAN.md (just created 2025-10-19)
- ✅ MAINT-10-EXECUTION-PLAN.md (just created 2025-10-19)
- ✅ MAINT-10-EXTRACTION-PLAN.md

**Design References** (KEEP):
- ✅ ARCHITECTURE.md
- ✅ QUALITY_GATES_DESIGN.md
- ✅ QUALITY_GATES_SPECIFICATION.md
- ✅ QUALITY_GATES_CONFIGURATION.md
- ✅ consensus-runner-design.md
- ✅ model-strategy.md
- ✅ telemetry-schema-v2.md

**Implementation Guides** (KEEP):
- ✅ spec-auto-automation.md
- ✅ spec-auto-full-automation-plan.md
- ✅ MIGRATION_GUIDE.md
- ✅ ensemble-run-checklist.md
- ✅ new-spec-command.md

**Could Archive**:
- ⚠️ COMMAND_REGISTRY_DESIGN.md
- ⚠️ COMMAND_REGISTRY_TESTS.md
- ⚠️ COMMAND_INVENTORY.md
- ⚠️ QUALITY_GATE_EXPERIMENT.md
- ⚠️ FORK_ISOLATION_AUDIT.md
- ⚠️ TEMPLATE_INTEGRATION.md
- ⚠️ TEMPLATE_VALIDATION_EVIDENCE.md
- ⚠️ evidence-baseline.md

---

## Phase 3: Create Documentation Index

**Create `docs/INDEX.md`** (Navigation Hub):

```markdown
# Documentation Index

**Last Updated**: 2025-10-19

---

## 🎯 Start Here (Essential Reading)

| Document | Purpose | Location |
|----------|---------|----------|
| **SPEC.md** | Task tracker, single source of truth | `/SPEC.md` |
| **CLAUDE.md** | Operating guide for Claude Code | `/CLAUDE.md` |
| **product-requirements.md** | Product scope and vision | `/product-requirements.md` |
| **PLANNING.md** | High-level architecture | `/PLANNING.md` |
| **README.md** | Project overview | `/README.md` |

---

## 📋 Policies & Standards

| Document | Purpose |
|----------|---------|
| **testing-policy.md** | Test coverage strategy (604 tests, 42-48% coverage) |
| **evidence-policy.md** | Evidence retention and archival |
| **UPSTREAM-SYNC.md** | Quarterly sync process |

---

## 🏗️ Architecture & Design

| Document | Purpose |
|----------|---------|
| **ARCHITECTURE.md** | Spec-kit architecture overview |
| **async-sync-boundaries.md** | Ratatui (sync) + Tokio (async) patterns |
| **SPEC_AUTO_FLOW.md** | Pipeline flow (6 stages) |
| **QUALITY_GATES_DESIGN.md** | Quality gate architecture |

---

## 🧪 Testing

| Document | Purpose |
|----------|---------|
| **TESTING_INFRASTRUCTURE.md** | Test infrastructure guide |
| **PHASE3_TEST_PLAN.md** | Integration tests plan/completion |
| **PHASE4_TEST_PLAN.md** | Edge cases + property-based tests |

---

## 🔧 Implementation Guides

| Document | Purpose |
|----------|---------|
| **spec-auto-automation.md** | Spec-kit automation guide |
| **MIGRATION_GUIDE.md** | Migration patterns |
| **ensemble-run-checklist.md** | Multi-agent checklist |
| **MAINT-10-EXECUTION-PLAN.md** | Crate extraction plan (deferred) |

---

## 📦 Archive

**Session Notes**: `docs/archive/2025-sessions/`
**Design Docs**: `docs/archive/design-docs/`
**Completed Specs**: `docs/archive/completed-specs/`

---

## 🔍 Finding Documentation

**By Topic**:
- Testing → testing-policy.md, TESTING_INFRASTRUCTURE.md, PHASE3/4_TEST_PLAN.md
- Quality Gates → QUALITY_GATES_*.md (3 files)
- Evidence → evidence-policy.md
- Upstream Sync → UPSTREAM-SYNC.md

**By Type**:
- Policies → docs/spec-kit/*-policy.md
- Architecture → docs/architecture/, ARCHITECTURE.md
- Guides → spec-auto-*.md, MIGRATION_GUIDE.md
```

---

## Execution Commands

**Create Archive and Move Files**:
```bash
cd /home/thetu/code

# Create archive structure (DONE ✅)
mkdir -p docs/archive/{2025-sessions,design-docs,completed-specs}

# Archive root-level stale docs (DONE ✅)
# ... (commands above)

# Archive docs/spec-kit/ stale docs
mv docs/spec-kit/SESSION_*.md docs/archive/2025-sessions/
mv docs/spec-kit/EPIC_*.md docs/archive/2025-sessions/
mv docs/spec-kit/REFACTORING_*.md docs/archive/2025-sessions/
mv docs/spec-kit/PHASE_1_*.md docs/archive/2025-sessions/
mv docs/spec-kit/PHASE_2_*.md docs/archive/design-docs/
mv docs/spec-kit/PHASE_3_DAY_*.md docs/archive/2025-sessions/
mv docs/spec-kit/ARCHITECTURE_COMPLETE_*.md docs/archive/2025-sessions/
mv docs/spec-kit/REVIEW_COMPLETION_*.md docs/archive/2025-sessions/
mv docs/spec-kit/STEP_*.md docs/archive/design-docs/
mv docs/spec-kit/SERVICE_TRAITS_*.md docs/archive/design-docs/
mv docs/spec-kit/REMAINING_*.md docs/archive/design-docs/

# Create index
# (see content above)
```

---

## Expected Results

### Before Cleanup
- **Root**: 30 .md files
- **docs/spec-kit/**: 45 files
- **Total sprawl**: 75+ operational docs

### After Cleanup
- **Root**: 18 essential .md files (-12, -40%)
- **docs/spec-kit/**: 20-25 current docs (-20-25, -45%)
- **docs/archive/**: 28+ archived docs
- **docs/INDEX.md**: Navigation hub (NEW)
- **Total active docs**: ~40-45 (-35-40%)

---

## Benefits

1. **Clarity**: Essential docs easy to find
2. **Maintenance**: Fewer docs to keep current
3. **Onboarding**: Clear "Start Here" path
4. **History**: Archived docs preserved for reference
5. **Focus**: Remove obsolete/superseded content

---

## Status

- ✅ **Archive structure created** (docs/archive/)
- ✅ **Root-level cleanup partial** (11 files moved)
- ⏸️ **docs/spec-kit/ cleanup pending** (shell permission issues)
- ⏸️ **docs/INDEX.md creation pending**

**Next Steps**: Execute remaining mv commands manually or via script in next session.
