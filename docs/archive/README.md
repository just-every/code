# Documentation Archive

**Created**: 2025-10-19
**Purpose**: Preserve historical session notes, completed design docs, and superseded documentation

---

## Structure

### 2025-sessions/
**Session summaries, handoffs, and temporal analysis documents**

- Session summaries from October 2025 development sprints
- Refactoring blockers and resolutions
- Agent execution logs
- Handoff documents
- Temporary output and planning files

**Retention**: Indefinite (historical reference)

### design-docs/
**Completed design documents and strategic analysis**

- Completed design plans (PHASE_*, REFACTORING_PLAN, etc.)
- Strategic analysis documents (OPTIMIZATION_ANALYSIS, model strategy)
- Architecture planning (superseded by current ARCHITECTURE.md)
- Extraction plans (PHASE_2_EXTRACTION_PLAN, etc.)

**Retention**: Indefinite (design rationale preserved)

### completed-specs/
**Spec directories that reached unlock stage**

- SPEC-KIT-* directories for completed/archived features
- Historical feature specifications
- Implementation evidence (moved from active docs/)

**Retention**: Per evidence-policy.md (compress >30d, offload >90d, purge >180d)

---

## What's NOT Archived

**Active Documentation** (keep in main locations):
- Current policies (testing-policy, evidence-policy)
- Architecture docs (ARCHITECTURE.md, SPEC_AUTO_FLOW.md)
- Implementation guides (spec-auto-automation, MIGRATION_GUIDE)
- Test plans for ongoing work (PHASE3/4_TEST_PLAN if status != complete)
- Task tracker (SPEC.md)
- Operating guides (CLAUDE.md, AGENTS.md)

**Rule**: If document is referenced in current work or needed for operations, it stays active.

---

## Accessing Archived Docs

**Via Git History**:
```bash
git log --follow docs/archive/2025-sessions/SESSION_SUMMARY_2025-10-16.md
```

**Via Grep**:
```bash
grep -r "specific topic" docs/archive/
```

**Via Documentation**:
- See docs/INDEX.md for navigation
- Check SPEC.md for task history references

---

## Archive Policy

**When to Archive**:
- Session summaries: 30 days after session
- Design docs: When implemented or superseded
- Spec directories: When unlocked and evidence aged per policy
- Analysis docs: When decisions finalized

**When to Retrieve**:
- Historical context needed
- Design rationale research
- Debugging similar issues
- Upstream sync conflict resolution

---

**Last Archive**: 2025-10-19 (15 files archived, 30-40% doc sprawl reduction)
