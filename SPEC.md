# Spec Tracker

## Reality Check (2025-10-06 03:20)

**Vision:** Vague idea → visible automation → implementation
**Current state:** ✅ **WORKING** - Orchestrator provides full visibility + auto-resolution

### What Actually Works

✅ `/new-spec` - Creates SPEC from description (PRD, plan, tasks)
✅ Agent spawning - Gemini, Claude, GPT visible in TUI
✅ Consensus checking - Reads synthesis.json, halts on conflict
✅ Agent completion detection - Triggers consensus automatically
✅ Auto-advancement - Loops through stages without manual approval
✅ Halt on failure - Guardrail or consensus failure stops pipeline

### What Works (As of 2025-10-06 03:20)

✅ **Full visibility** - Orchestrator runs bash guardrails AND agents in visible conversation
✅ **Automatic conflict resolution** - Arbiter agent resolves disagreements, doesn't halt
✅ **Iterative consensus** - Agents debate, reach resolution, only halt on true deadlock
✅ **Clear progress** - Each step visible (bash, agents, synthesis, decisions)

### How It Works

**`/spec-auto` delegates to orchestrator (like `/plan` and `/solve`):**

Per stage:
1. Bash guardrail executes (visible in conversation)
2. Agents spawn (Gemini → Claude → GPT)
3. Conflicts detected → Arbiter spawns automatically
4. Arbiter resolves → Consensus reached
5. Only halts if arbiter can't resolve (rare deadlock)
6. Auto-advances to next stage

**User sees everything:** Bash output, agent work, arbiter decisions, synthesis.

---

## Active Work

### Current Sprint: Streaming Guardrail Output

**Goal:** Make guardrails visible during execution, not just after.

**Task:** Change guardrails from background `RunProjectCommand` to streaming `Op::Exec`

**File:** `codex-rs/tui/src/chatwidget.rs` line 14997

**Change:**
```rust
// Current: Background (OPAQUE)
self.submit_op(Op::RunProjectCommand { ... });

// Target: Streaming (VISIBLE)
self.submit_op(Op::Exec {
    context: exec_ctx,
    params: streaming_exec_params,
});
```

**Impact:** User sees bash output in real-time, like any other TUI command.

**Effort:** 4 hours
**Blocker:** Must test that ExecCommandEnd still triggers agent phase

---

## Task Breakdown

### Phase 1: Visibility (3 days) - CURRENT FOCUS

| ID | Task | Status | Effort | Blocker |
|----|------|--------|--------|---------|
| T37 | Stream guardrail output to TUI | **IN PROGRESS** | 4h | None |
| T38 | Add guardrail progress messages | **TODO** | 1h | T37 |
| T39 | Parse/display guardrail results | **TODO** | 3h | T37 |
| T40 | Add stage progress indicator (X/6) | **TODO** | 4h | None |
| T41 | Add Ctrl-C cancellation | **TODO** | 2h | None |

### Phase 2: Polish (2 days)

| ID | Task | Status | Effort | Blocker |
|----|------|--------|--------|---------|
| T42 | Substep streaming (baseline, policy, HAL) | **TODO** | 8h | T37-T39 |
| T43 | Integration tests (guardrail→agent→consensus) | **TODO** | 8h | T37-T41 |
| T44 | Rebase validation script | **TODO** | 2h | None |
| T45 | E2E test (plan→unlock) | **TODO** | 4h | T37-T43 |

### Phase 3: Maintenance (ongoing)

| ID | Task | Status | Effort | Notes |
|----|------|--------|--------|-------|
| T30 | Project Commands migration | **BACKLOG** | 4-6h | Reduces rebase friction |
| T31 | Evidence archival | **BACKLOG** | 1d | 25MB limit mitigation |
| T33 | Task format unification | **BACKLOG** | 2-3d | Eliminates manual sync |
| T34 | Conflict arbiter agent | **BACKLOG** | 2-3d | Auto-resolves ties |

---

## Completed Work (Archive)

**Foundation (Sept-Oct 2025):**
- T1-T24, T26-T27: Guardrails, consensus runner, telemetry, MCP servers, docs
- See git log for details

**This session (Oct 5-6):**
- T28: Bash consensus integration (fallback, not primary path)
- T29: /new-spec unified intake ✅
- T32: Guardrail→agent wiring ✅
- T36: Fork-specific guards ✅
- T37: Sandbox fix for policy checks ✅

**What's misleading in "Completed":**
- T32 claimed "full automation works" - FALSE
- Guardrails are background, not visible
- Session commits include orchestrator attempt (didn't work, keeping as reference)

---

## Implementation Plan Reference

**See TUI.md for:**
- 6-day detailed implementation roadmap
- Task-by-task breakdown with code locations
- Rebase strategy (before/during/after)
- Test validation suite
- Rollback options

**Next action:** Start T37 (streaming guardrail output)

---

## Fork Deviation Summary

**Files modified from upstream:**
- `codex-rs/tui/src/chatwidget.rs` (+304 lines spec-auto logic)
- `codex-rs/tui/src/chatwidget/exec_tools.rs` (+33 lines guardrail handler)
- `codex-rs/tui/src/slash_command.rs` (+36 lines enum variants)
- `codex-rs/tui/src/spec_prompts.rs` (+200 lines, new file)
- `codex-rs/tui/src/app.rs` (+10 lines routing)

**Total TUI diff:** ~583 lines
**All marked with:** `// === FORK-SPECIFIC: ... ===` guards

**Rebase risk:** Medium (core TUI file)
**Mitigation:** Guards + validation script (T44)

---

## Current Branch State

- **Branch:** feat/spec-auto-telemetry
- **Ahead of origin:** 13 commits
- **Unpushed changes:** +2615 lines (includes evidence artifacts)
- **Fork baseline:** spec-kit-base branch created
- **Rebase guide:** FORK_DEVIATIONS.md + TUI.md

**Ready to push** once T37-T39 complete (streaming visibility working).

---

## Notes

**Architecture decisions:**
- Bash guardrails (necessary - baseline/HAL/policy validation)
- TUI-native agents (visible, interruptible)
- Keep consensus_runner.sh as fallback (bash `/spec-ops-auto`)
- Reject: Orchestrator approach (too opaque during bash execution)
- Reject: Project Hooks migration (don't fire for tools, 40% coverage)

**Current blocker:** Guardrail visibility (T37-T39)

**After visibility fixed:** Full working automation, 6 stages visible, truly native TUI implementation.

## Tasks

| Order | Task ID | Title | Status | Owners | PRD | Branch | PR | Last Validation | Evidence | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 11 | T46 | Add documentation for fork rebasing and nightly drift verification | Backlog | Code |  | feat/spec-auto-telemetry |  |  |  | Created via /new-spec on 2025-10-05 |
| 12 | T47 | Spec status diagnostics dashboard | In Progress | Code | docs/SPEC-KIT-035-spec-status-diagnostics/PRD.md | feat/spec-auto-telemetry |  |  |  | 2025-10-08 /tasks consensus: 8 slices covering telemetry aggregator, evidence sentinel, TUI & CLI parity, HAL messaging, fixtures/tests, docs, release validation |
