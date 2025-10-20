# Specification: Native Tool Migration for Spec-Kit Commands

**SPEC-ID**: SPEC-KIT-066
**PRD**: [PRD.md](./PRD.md)
**Status**: Backlog
**Created**: 2025-10-20

---

## Overview

Migrate spec-kit command orchestrator instructions from bash/python scripts to native Codex tools (Glob, Read, Write, Edit).

---

## Requirements

### R1: SPEC-ID Generation (Native)
**Priority**: P0
**Current**: Python script `generate_spec_id.py`
**Target**: Glob tool + parsing logic

**Acceptance**:
- Find all SPEC-KIT-* directories via Glob
- Parse numbers, find max, increment by 1
- Generate slug from description
- Return SPEC-KIT-### format

### R2: Directory Creation (Native)
**Priority**: P0
**Current**: `mkdir -p docs/${SPEC_ID}/`
**Target**: Write tool (creates parent dirs automatically)

**Acceptance**:
- Write tool creates non-existent parent directories
- No explicit mkdir needed

### R3: SPEC.md Table Updates (Native)
**Priority**: P0
**Current**: Python/bash script appends rows
**Target**: Edit tool

**Acceptance**:
- Read SPEC.md to find insertion point
- Edit tool adds properly formatted table row
- Table structure preserved

### R4: Template Rendering (Native)
**Priority**: P1
**Current**: Template files + bash/python substitution
**Target**: Read tool + string replacement

**Acceptance**:
- Read template files
- Replace placeholders with values
- Write filled templates to SPEC directory

### R5: Guardrail Scripts (Keep Bash)
**Priority**: P2
**Current**: bash scripts in `scripts/spec_ops_004/commands/*.sh`
**Target**: NO CHANGE (legitimate bash use)

**Rationale**:
- Complex validation logic
- Cargo/clippy/fmt integration
- Telemetry JSON parsing
- Well-tested and stable
- Rewriting would be high-risk, low-reward

---

## Implementation Plan

See PRD.md Phase 1-3 for detailed breakdown.

**Summary**:
1. Audit current bash/python usage
2. Design native replacements
3. Update config.toml orchestrator-instructions
4. Test with real execution
5. Validate with full test suite

---

## Testing Strategy

**Unit Tests**: Native SPEC-ID generation logic
**Integration Tests**: Full `/speckit.new` execution
**Regression Tests**: 604-test suite must pass
**Real-World Test**: Create actual feature SPEC and verify

---

## References

- Session findings: 2025-10-20 (routing bug discovery + fix)
- Config: ~/.code/config.toml (orchestrator instructions)
- ARCH-004: Native MCP migration precedent
- Templates: ~/.code/templates/*.md
