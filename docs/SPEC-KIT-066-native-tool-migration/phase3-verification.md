# Phase 3 Verification: Command Review

**Created**: 2025-10-20
**SPEC-ID**: SPEC-KIT-066
**Purpose**: Verify all other commands are already native or legitimately use bash

---

## Verification Results

### ✅ Already Native Commands (No Changes Needed)

| Command | Lines | Analysis | Status |
|---------|-------|----------|--------|
| **speckit.specify** | 287-296 | Uses "Gather", "Draft or update", "Update", "Emit" - all native operations | ✅ NATIVE |
| **speckit.plan** | 299-303 | Agent consensus, uses templates - no bash/python | ✅ NATIVE |
| **speckit.tasks** | 306-314 | "Fan out agents", "Merge outputs", "Update SPEC.md" - all native | ✅ NATIVE |
| **speckit.clarify** | 416-435 | "Extract", "Scan", "Present questions", "update spec.md" - all native | ✅ NATIVE |
| **speckit.analyze** | 438-458 | "Load artifacts", "Fan out agents", "Generate", "Present" - all native | ✅ NATIVE |
| **speckit.checklist** | 461-478 | "Load", "Evaluate", "Generate", "Provide" - all native | ✅ NATIVE |
| **speckit.status** | 481-515 | All Read operations for files and JSON - all native | ✅ NATIVE |

### ✅ Legitimately Uses Bash (Documented Exception)

| Command | Lines | Bash Usage | Rationale | Status |
|---------|-------|------------|-----------|--------|
| **speckit.implement** | 317-326 | Line 324: `scripts/env_run.sh cargo fmt --all -- --check`, clippy, builds/tests | Cargo/clippy are external tools. Bash is appropriate wrapper for validation. | ✅ KEEP BASH |
| **speckit.auto** | 329-413 | Line 342: `bash scripts/spec_ops_004/commands/spec_ops_{stage}.sh`<br>Line 393: Guardrail bash scripts | Complex validation logic, telemetry parsing, multi-step scenarios. Well-tested and stable. | ✅ KEEP BASH |

---

## Phase 3 Summary

**Total Commands Reviewed**: 9 (speckit.* namespace)

**Migration Results**:
- ✅ **1 command migrated**: speckit.new (Phase 2)
- ✅ **7 commands already native**: specify, plan, tasks, clarify, analyze, checklist, status
- ✅ **2 commands keep bash**: implement (cargo/clippy), auto (guardrails)

**Inventory Accuracy**: 100% (all predictions from Phase 1 confirmed)

---

## Evidence: Native Command Verification

### speckit.specify (Lines 287-296)
```toml
orchestrator-instructions = """
Single high-reasoning GPT-5 Codex session with staged context.
1. Gather `memory/constitution.md`, `product-requirements.md`, planning.md, the SPEC.md row, and any existing plan/spec for the task.
2. Draft or update `docs/SPEC-<AREA>-<slug>/PRD.md` using repo templates; capture assumptions, acceptance criteria, and open questions inline.
3. Update the SPEC.md Tasks table atomically with PRD path, concise scope summary, status change, and dated evidence note.
4. Emit a diff preview plus the open-question list for `/plan`; no code or implementation edits permitted.
```
**Analysis**: All operations (Gather, Draft, Update, Emit) use native tools. No bash/python references.

### speckit.plan (Lines 299-303)
```toml
orchestrator-instructions = """
Multi-agent consensus planning. Ingest `docs/SPEC-<AREA>-<slug>/PRD.md` (and existing `docs/SPEC-<AREA>-<slug>/spec.md` if present), collect proposals from all agents, document disagreements, then synthesize `docs/SPEC-<AREA>-<slug>/plan.md` using `templates/plan-template.md`. Record consensus vs. dissent explicitly and leave SPEC.md untouched aside from referenced assumptions.
"""
```
**Analysis**: Pure agent consensus workflow. Uses templates. No bash/python.

### speckit.tasks (Lines 306-314)
```toml
orchestrator-instructions = """
Three-phase synthesis run:
1. Fan out agents to draft task slices tied to the SPEC Task ID, listing dependencies, validation hooks, and required documentation touches.
2. Merge outputs into `docs/SPEC-<AREA>-<slug>/spec.md`, explicitly logging agreements, disagreements, and rationale; include compare/contrast notes for any divergent recommendations.
3. Update SPEC.md's Tasks table atomically (status, branch/PR placeholders, dated notes) and record unresolved risks within the spec.
```
**Analysis**: Agent consensus + file operations. All native.

### speckit.implement (Lines 317-326) - KEEPS BASH
```toml
orchestrator-instructions = """
...
3. Apply the chosen diff locally, then run `scripts/env_run.sh cargo fmt --all -- --check`, clippy, targeted builds/tests, and any spec-specific checks; attach logs to the command output.
```
**Analysis**: Uses bash for cargo/clippy validation. **LEGITIMATE** - these are external tools.
**Decision**: KEEP AS-IS (documented exception in PRD)

### speckit.auto (Lines 329-413) - KEEPS BASH
```toml
orchestrator-instructions = """
...
STEP 1: Run guardrail validation
Execute: bash scripts/spec_ops_004/commands/spec_ops_{stage}.sh {SPEC-ID}
If guardrail fails (exit != 0): HALT with error message
...
CRITICAL EXECUTION RULES:
- Run guardrail bash scripts: bash scripts/spec_ops_004/commands/spec_ops_{stage}.sh
```
**Analysis**: Uses bash for guardrail validation scripts. **LEGITIMATE** - complex logic, telemetry, well-tested.
**Decision**: KEEP AS-IS (documented exception in PRD)

### speckit.clarify, analyze, checklist (Lines 416-478)
All use agent consensus and native file operations. No bash/python references.

### speckit.status (Lines 481-515)
Pure Read operations for files and JSON telemetry. All native.

---

## Conclusion

**Phase 3 Complete**:
- ✅ All commands verified
- ✅ No additional migrations needed
- ✅ Bash usage in implement/auto is appropriate and documented
- ✅ Inventory from Phase 1 was 100% accurate

**Next**: Phase 4 requires USER testing in TUI environment.
