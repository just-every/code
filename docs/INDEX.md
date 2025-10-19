# Documentation Index

**Last Updated**: 2025-10-19
**Total Active Docs**: ~45 essential documents
**Archive**: docs/archive/ (session notes, completed designs)

---

## 🎯 Start Here (Essential Reading)

**New to the project? Read these first:**

| Document | Purpose | Status |
|----------|---------|--------|
| **SPEC.md** | Task tracker, single source of truth | ✅ Current (2025-10-19) |
| **CLAUDE.md** | Operating guide for Claude Code | ✅ Current (2025-10-19) |
| **product-requirements.md** | Product scope and vision | ✅ Current |
| **PLANNING.md** | High-level architecture, goals, constraints | ✅ Current |
| **README.md** | Project overview, quick start | ✅ Current |

---

## 📋 Policies & Standards

**Governance and compliance:**

| Document | Purpose | Status |
|----------|---------|--------|
| **testing-policy.md** | Test coverage strategy (604 tests, 42-48% coverage) | ✅ Current (2025-10-19) |
| **evidence-policy.md** | Evidence retention, archival automation | ✅ Current |
| **UPSTREAM-SYNC.md** | Quarterly sync process, conflict resolution | ✅ Current |
| **MEMORY-POLICY.md** | Memory system policy (local-memory only) | ✅ Current |
| **memory/constitution.md** | Project charter and guardrails | ✅ Current |

**Location**: `/docs/` and `/docs/spec-kit/`

---

## 🏗️ Architecture & Design

**System architecture and design decisions:**

| Document | Purpose | Status |
|----------|---------|--------|
| **spec-kit/ARCHITECTURE.md** | Spec-kit architecture overview | ✅ Current |
| **architecture/async-sync-boundaries.md** | Ratatui (sync) + Tokio (async) patterns | ✅ Current |
| **SPEC_AUTO_FLOW.md** | Pipeline flow (6 stages: Plan→Unlock) | ✅ Current |
| **IMPLEMENTATION_CONSENSUS.md** | Implementation details | ✅ Current |
| **FORK_DEVIATIONS.md** | Fork-specific changes vs upstream | ✅ Current |

**Design Documents**:
- **spec-kit/QUALITY_GATES_DESIGN.md** - Quality gate architecture
- **spec-kit/QUALITY_GATES_SPECIFICATION.md** - Detailed specifications
- **spec-kit/consensus-runner-design.md** - Consensus automation
- **spec-kit/model-strategy.md** - Tiered model strategy (Tier 0-4)

---

## 🧪 Testing Documentation

**Test infrastructure and plans:**

| Document | Purpose | Status |
|----------|---------|--------|
| **TESTING_INFRASTRUCTURE.md** | MockMcpManager, fixtures, tarpaulin setup | ✅ Current |
| **PHASE3_TEST_PLAN.md** | Integration tests (W/E/S/Q/C categories) | ✅ Complete 2025-10-19 |
| **PHASE4_TEST_PLAN.md** | Edge cases + property-based tests (EC/PB) | ✅ Complete 2025-10-19 |
| **testing-policy.md** | Coverage goals, module targets, roadmap | ✅ Current (2025-10-19) |

**Test Results**: 604 tests @ 100% pass rate, 42-48% estimated coverage

---

## 🔧 Implementation & Operation Guides

**How-to guides and operational procedures:**

| Document | Purpose |
|----------|---------|
| **spec-auto-automation.md** | Spec-kit automation workflows |
| **spec-auto-full-automation-plan.md** | Full automation implementation |
| **MIGRATION_GUIDE.md** | Migration patterns and examples |
| **ensemble-run-checklist.md** | Multi-agent run checklist |
| **new-spec-command.md** | Creating new spec commands |
| **COMMAND_REGISTRY_DESIGN.md** | Command registry architecture |
| **telemetry-schema-v2.md** | Telemetry schema specification |
| **CONFLICT_RESOLUTION.md** | Consensus conflict handling |

---

## 📦 Deferred & Archive

### Deferred Tasks

| Task | Status | Location |
|------|--------|----------|
| **MAINT-10** | Deferred indefinitely | MAINT-10-EXECUTION-PLAN.md |

**Rationale**: No CLI/API/library consumers exist (YAGNI principle)

### Archived Documentation

**Session Notes**: `docs/archive/2025-sessions/`
- SESSION_SUMMARY_2025-10-16.md
- EPIC_SESSION_SUMMARY_2025-10-16.md
- REFACTORING_*.md (7 files)
- PHASE_1_*.md (2 files)
- And 15+ more session-specific documents

**Design Docs**: `docs/archive/design-docs/`
- REFACTORING_PLAN.md
- PHASE_2_EXTRACTION_PLAN.md
- SERVICE_TRAITS_DEEP_ANALYSIS.md
- And 10+ completed design documents

**Completed Specs**: `docs/archive/completed-specs/`
- Feature specs that reached unlock stage

---

## 🔍 Finding Documentation

### By Topic

**Testing**:
- Start: testing-policy.md
- Infrastructure: TESTING_INFRASTRUCTURE.md
- Plans: PHASE3_TEST_PLAN.md, PHASE4_TEST_PLAN.md

**Quality Gates**:
- Overview: QUALITY_GATES_DESIGN.md
- Details: QUALITY_GATES_SPECIFICATION.md
- Config: QUALITY_GATES_CONFIGURATION.md

**Evidence**:
- Policy: evidence-policy.md
- Baseline: TESTING_INFRASTRUCTURE.md (fixtures)

**Upstream Sync**:
- Process: UPSTREAM-SYNC.md
- Isolation: FORK_DEVIATIONS.md (80 FORK-SPECIFIC markers)

**Architecture**:
- Overview: ARCHITECTURE.md
- Async/Sync: async-sync-boundaries.md
- Pipeline: SPEC_AUTO_FLOW.md

### By Audience

**New Contributors**:
1. README.md
2. CLAUDE.md
3. product-requirements.md
4. PLANNING.md

**Developers**:
1. SPEC.md (current tasks)
2. ARCHITECTURE.md
3. testing-policy.md
4. CLAUDE.md (operating guide)

**AI Agents**:
1. CLAUDE.md (mandatory)
2. AGENTS.md (orchestration)
3. SPEC.md (task context)
4. Relevant policy docs

---

## 📊 Documentation Statistics

**Total Project Docs**: ~250 .md files

**Active Documentation**:
- Essential (root): 18 files
- Spec-kit: 20-25 files
- Architecture: 3 files
- Specs (SPEC-KIT-*): ~150 directories

**Archived**: 28+ files
- Session notes: 16 files
- Design docs: 12 files

**Reduction**: 30-40% fewer active docs to maintain

---

## 🔄 Maintenance

**Update Frequency**:
- **Daily**: SPEC.md (task updates)
- **Per session**: CLAUDE.md (if prerequisites change)
- **Per release**: CHANGELOG.md
- **Quarterly**: UPSTREAM-SYNC.md (after sync)
- **As needed**: Policy docs (testing-policy, evidence-policy)

**Stale Document Policy**:
- Session summaries → Archive after 30 days
- Design docs → Archive when implemented
- Completed specs → Archive when unlocked
- Analysis docs → Archive after decisions made

---

## 🎓 Quick Reference

**Common Tasks**:
- Run spec-kit command → See CLAUDE.md section 2
- Write tests → See testing-policy.md, TESTING_INFRASTRUCTURE.md
- Handle quality gates → See QUALITY_GATES_DESIGN.md
- Sync upstream → See UPSTREAM-SYNC.md
- Find evidence → See evidence-policy.md

**Common Questions**:
- "How do I...?" → Check CLAUDE.md first
- "What's the status of...?" → Check SPEC.md
- "Why did we...?" → Check relevant policy or design doc
- "Where is...?" → Check this INDEX.md

---

**Navigation**: Return to [README.md](../README.md) | [SPEC.md](../SPEC.md) | [CLAUDE.md](../CLAUDE.md)
