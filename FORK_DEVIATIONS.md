# Fork-Specific Deviations from Upstream

This document tracks intentional deviations from `anthropics/claude-code` (upstream) to enable spec-kit automation workflow.

## Branch Strategy

```
upstream/main (anthropics/claude-code)
  ↓
upstream-merge (staging branch for upstream syncs)
  ↓
main (your stable fork)
  ↓
spec-kit-base (stable spec-kit features)
  ↓
feat/spec-auto-telemetry (active development)
```

**Rebase workflow:**
1. Merge upstream → upstream-merge
2. Selectively merge upstream-merge → main ("by-bucket" strategy)
3. Rebase feat/* branches, preserve FORK-SPECIFIC sections
4. Keep spec-kit-base synchronized with stable spec-kit features

---

## Code Deviations

### File: `codex-rs/tui/src/chatwidget.rs`

**Lines ~17063-17500:** Spec-kit automation state machine

**Sections marked with:**
```rust
// === FORK-SPECIFIC: spec-kit automation ===
// Upstream: Does not have /spec-auto pipeline
// Preserve: During rebases, keep all code in this section
```

**Changes:**
- SpecAutoPhase enum (ExecutingAgents, CheckingConsensus states)
- SpecAutoState struct (pipeline state tracking)
- handle_spec_auto_command() (pipeline orchestration)
- Auto-submit mechanism (bypass manual approval)
- Consensus checking integration
- Agent completion hooks

**Why:** Enable automated multi-stage pipeline with consensus validation

**Upstream impact:** None - additive only, doesn't modify existing features

---

### File: `codex-rs/tui/src/slash_command.rs`

**Lines 122-152:** Spec-ops slash commands enum

**Marked as:**
```rust
// === FORK-SPECIFIC: spec-ops commands ===
// Upstream: Basic slash commands only
// Preserve: SpecOpsPlan, SpecOpsAuto, SpecEvidenceStats, etc.
```

**Why:** Integrate spec-kit guardrail scripts with TUI

**Upstream impact:** None - additive enum variants

**Migration plan:** Move to Project Commands (T30) to eliminate this deviation

---

### File: `codex-rs/tui/src/spec_prompts.rs`

**Entire file:** Fork-specific

**Why:** Parse docs/spec-kit/prompts.json for multi-agent consensus

**Upstream impact:** None - isolated module

---

## Non-Code Deviations

### Directory: `scripts/spec_ops_004/`

**Status:** Fork-only, no upstream equivalent

**Contents:**
- Guardrail scripts (plan, tasks, implement, validate, audit, unlock)
- Consensus runner
- Telemetry utils
- SPEC-ID generator
- Synthesis checker

**Upstream impact:** None - completely separate tooling

---

### Directory: `docs/spec-kit/`

**Status:** Fork-only documentation

**Upstream impact:** None

---

### File: `.github/codex/home/config.toml`

**Additions:**
- `/new-spec` subagent command
- `/spec-auto` subagent command (pending TUI implementation)
- Custom agent configurations

**Marked in file:**
```toml
# === FORK-SPECIFIC: spec-kit subagent commands ===
[[subagents.commands]]
name = "new-spec"
# ...
# === END FORK-SPECIFIC ===
```

**Upstream impact:** None - config is user-specific

---

## Rebase Checklist

When syncing from upstream:

**Before rebase:**
1. ✅ Tag current state: `git tag pre-rebase-$(date +%Y%m%d)`
2. ✅ Update spec-kit-base: `git checkout spec-kit-base && git merge feat/spec-auto-telemetry`
3. ✅ Review upstream changes: `git log upstream/main ^main`

**During rebase:**
1. ✅ Preserve all FORK-SPECIFIC sections
2. ✅ Accept upstream for unmarked code
3. ✅ Test after each conflict resolution

**After rebase:**
1. ✅ Verify spec-kit works: `./scripts/spec_ops_004/spec_auto.sh --help`
2. ✅ Run tests: `cd codex-rs && cargo test spec_auto`
3. ✅ Test TUI /spec-auto command

---

## Migration Goals

**Reduce deviations over time:**

- **T30:** Migrate slash commands → Project Commands (-357 lines from slash_command.rs)
- **Future:** Upstream /spec-auto if pattern generalizes
- **Future:** Contribute consensus patterns upstream if useful

**Keep as fork-only:**
- spec_ops_004/ scripts (domain-specific)
- docs/spec-kit/ (project-specific)
- Telemetry schema (Kavedarr-specific)

---

## Conflict Resolution Guide

**Scenario 1: Upstream modifies chatwidget.rs**

```bash
# During rebase conflict:
git show :1:codex-rs/tui/src/chatwidget.rs > base.rs
git show :2:codex-rs/tui/src/chatwidget.rs > ours.rs  # Fork
git show :3:codex-rs/tui/src/chatwidget.rs > theirs.rs  # Upstream

# Extract FORK-SPECIFIC sections from ours.rs
grep -A 9999 "FORK-SPECIFIC" ours.rs > fork-sections.rs

# Accept upstream base
git checkout --theirs codex-rs/tui/src/chatwidget.rs

# Re-inject fork sections at marked locations
# (manual or via script)

git add codex-rs/tui/src/chatwidget.rs
```

**Scenario 2: Upstream adds features that interact with spec-kit**

- Review upstream changes for compatibility
- Adapt FORK-SPECIFIC sections if needed
- Test spec-kit pipeline after rebase
- Document adaptations in this file

---

## Version Tracking

**Current fork basis:** Commit dbbcb5d52 (feat(project-hooks): add project hooks and commands)

**Last upstream sync:** [Check git log for latest upstream-merge]

**Spec-kit version:** 1.0 (T28 + T29 complete)

**Update this section after each upstream sync.**

---

## Maintenance

**Monthly:**
- Sync upstream-merge from upstream/main
- Review upstream changelog for conflicts
- Test rebase on throwaway branch first

**Before major changes:**
- Update spec-kit-base branch
- Tag stable states
- Document new deviations here

**Long-term goal:**
- Minimize TUI deviations via Project Commands migration
- Keep spec-kit tooling isolated in scripts/
- Contribute generalizable patterns upstream
