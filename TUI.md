# TUI-Native Spec-Kit Implementation & Fork Management

## Executive Summary

**Current state:** Hybrid - guardrails run in background (opaque), agents spawn natively (visible).
**Goal:** Pure TUI - streaming guardrail output, visible agents, full control, zero orchestrator dependency.

**Effort:** 3-4 days implementation + 1 day testing
**Rebase risk:** Medium (500+ lines in core TUI file)
**Mitigation:** Comprehensive guards, test suite, clear documentation

---

## Part 1: Current Implementation Analysis

### What Works (Keep)

**Files: `codex-rs/tui/src/chatwidget.rs`, `exec_tools.rs`, `slash_command.rs`, `spec_prompts.rs`**

✅ **State machine structures** (lines 16612-17136):
- `SpecAutoPhase` enum (Guardrail, ExecutingAgents, CheckingConsensus)
- `SpecAutoState` struct (tracks pipeline progress)
- `GuardrailWait` struct (tracks which guardrail is running)

✅ **Agent spawning** (line 17458):
- `auto_submit_spec_stage_prompt()` - builds prompts from prompts.json
- Creates UserMessage, submits to orchestrator
- Agents spawn visibly with correct models/reasoning

✅ **Agent completion detection** (line 6776, 17544):
- `on_spec_auto_agents_complete()` - detects when all agents finish
- Transitions to CheckingConsensus phase
- Collects agent results

✅ **Consensus checking** (line 17600):
- `check_consensus_and_advance_spec_auto()` - reads synthesis.json
- Validates status (ok/conflict/degraded)
- Advances to next stage or halts

✅ **Guardrail completion trigger** (exec_tools.rs:771):
- Detects guardrail ExecCommandEnd
- Calls `auto_submit_spec_stage_prompt()`
- Wired correctly as of 2025-10-06

### What's Broken/Missing

❌ **Guardrail visibility** (line 17232):
- `handle_spec_ops_command()` submits as `RunProjectCommand`
- Runs in background with NO streaming output
- User sees nothing until completion
- **Impact:** Opaque 2-3 minute black box per stage

❌ **Error surfacing**:
- Guardrail failures happen in background
- Error message shown but no context
- Can't see which policy check failed or why

❌ **Progress indicators**:
- No "Running baseline check..."
- No "Validating HAL endpoints..."
- No stage X/6 progress bar

❌ **Cancellation**:
- Can interrupt TUI session but not individual guardrail
- No way to skip failing stage and continue

❌ **Guardrail output capture**:
- Bash stdout/stderr go to log files only
- Not streamed to TUI history
- User can't see what's happening

---

## Part 2: TUI-Native Implementation Plan

### Phase 1: Streaming Guardrail Output (2 days)

**Problem:** Guardrails run as background `RunProjectCommand` - output hidden until completion.

**Solution:** Stream guardrail output to TUI history in real-time.

#### Task 1.1: Add guardrail streaming support

**File:** `codex-rs/tui/src/chatwidget.rs`

**Current** (line 14997):
```rust
self.submit_op(Op::RunProjectCommand {
    name: format!("spec_ops_{}", meta.display),
    command: Some(wrapped),
    display: Some(command_line),
    env,
});
```

**Change to:**
```rust
// For spec-auto guardrails, use direct exec with streaming instead of RunProjectCommand
if self.spec_auto_state.is_some() {
    // Create exec context for streaming output
    let call_id = format!("guardrail_{}_{}",  meta.display, timestamp);
    let exec_ctx = ExecCommandContext {
        sub_id: self.current_sub_id(),
        call_id: call_id.clone(),
        command_for_display: wrapped.clone(),
        cwd: self.config.cwd.clone(),
        apply_patch: None,
    };

    // Submit as streaming exec (NOT background project command)
    self.submit_op(Op::Exec {
        context: exec_ctx,
        params: ExecParams {
            command: wrapped,
            cwd: self.config.cwd.clone(),
            timeout_ms: Some(600_000), // 10 min timeout
            env,
            with_escalated_permissions: Some(false),
            justification: None,
        },
    });
} else {
    // Non-spec-auto: use background project command
    self.submit_op(Op::RunProjectCommand { /* ... */ });
}
```

**Why:** `Op::Exec` creates visible ExecCell with streaming output. User sees bash execution in real-time.

**Effort:** 4 hours
**Risk:** Medium - need to handle exec vs project command correctly

---

#### Task 1.2: Add guardrail progress messages

**File:** `codex-rs/tui/src/chatwidget.rs`

**Location:** Before calling `handle_spec_ops_command()` (line 17232)

**Add:**
```rust
NextAction::RunGuardrail { command, args } => {
    // Show what's happening
    let stage_name = state.stages[state.current_index].display_name();
    self.history_push(PlainHistoryCell::new(
        vec![
            Line::from(format!("Running {} guardrail validation...", stage_name)),
            Line::from("  ⏳ Baseline checks"),
            Line::from("  ⏳ Policy validation"),
            Line::from("  ⏳ HAL endpoint tests"),
        ],
        HistoryCellType::Notice,
    ));

    self.handle_spec_ops_command(command, args);
    return;
}
```

**Why:** User sees what guardrail is doing, not just "running..."

**Effort:** 1 hour
**Risk:** Low

---

#### Task 1.3: Parse and display guardrail results

**File:** `codex-rs/tui/src/chatwidget/exec_tools.rs`

**Location:** After guardrail completion (line 789, after auto_submit call)

**Add:**
```rust
// Read and display guardrail results
let telemetry_path = format!(
    "docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/{}/spec-{}_*",
    spec_id, stage.key()
);

if let Ok(telemetry) = read_latest_telemetry(&telemetry_path) {
    let mut result_lines = vec![
        Line::from(format!("✓ {} guardrail complete:", stage.display_name()))
    ];

    if let Some(baseline) = telemetry.get("baseline") {
        let status = baseline["status"].as_str().unwrap_or("unknown");
        result_lines.push(Line::from(format!("  Baseline: {}", status)));
    }

    if let Some(policy) = telemetry.get("policy") {
        result_lines.push(Line::from(format!(
            "  Policy: prefilter={}, final={}",
            policy["prefilter"]["status"],
            policy["final"]["status"]
        )));
    }

    chat.history_push(PlainHistoryCell::new(
        result_lines,
        HistoryCellType::Notice,
    ));
}
```

**Why:** User sees what guardrail validated, not just "completed"

**Effort:** 3 hours
**Risk:** Low - read-only telemetry parsing

---

### Phase 2: Progress & Control (1 day)

#### Task 2.1: Add pipeline progress indicator

**File:** `codex-rs/tui/src/chatwidget.rs`

**Location:** Status bar or header during spec-auto

**Add:** Display current stage in status bar:
```
[spec-auto] Stage 2/6: tasks | Agents: 2/3 complete | ✓ plan
```

**Implementation:**
- Add to `SpecAutoState`: `completed_stages: Vec<SpecStage>`
- Update status bar in `render()` when `spec_auto_state.is_some()`
- Show: current stage, agent progress, completed stages

**Effort:** 4 hours
**Risk:** Low - display only, no logic changes

---

#### Task 2.2: Add stage cancellation support

**File:** `codex-rs/tui/src/chatwidget.rs`

**Add keyboard shortcut:** `Ctrl-C` during spec-auto stage

**Implementation:**
```rust
// In keyboard handler:
if spec_auto_state.is_some() {
    if key == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
        self.halt_spec_auto_with_error("Cancelled by user".to_string());
        // Cancel running guardrail/agents
        self.interrupt_current_turn();
        return;
    }
}
```

**Effort:** 2 hours
**Risk:** Low

---

### Phase 3: Enhanced Visibility (1 day)

#### Task 3.1: Stream guardrail substeps

**Current:** Guardrail is monolithic bash execution
**Goal:** Show individual checks as they complete

**Approach:** Parse guardrail log in real-time, emit events:

**File:** `scripts/spec_ops_004/common.sh`

**Add logging hook:**
```bash
spec_ops_emit_progress() {
  local stage="$1"
  local message="$2"
  # Write to progress file that TUI can poll
  echo "${message}" >> "${SPEC_OPS_STAGE_DIR}/progress.log"
}

# In guardrail scripts:
spec_ops_emit_progress "plan" "Running baseline audit..."
# ... baseline check ...
spec_ops_emit_progress "plan" "✓ Baseline passed"

spec_ops_emit_progress "plan" "Running policy prefilter..."
# ... policy check ...
spec_ops_emit_progress "plan" "✓ Policy prefilter passed"
```

**File:** `codex-rs/tui/src/chatwidget.rs`

**Add progress polling:**
```rust
// When guardrail running, poll progress.log every 500ms
// Update history cell with latest progress
```

**Effort:** 6-8 hours (bash changes + TUI polling)
**Risk:** Medium - file watching, polling logic

---

### Phase 4: Testing & Validation (1 day)

#### Task 4.1: Add integration tests

**File:** `codex-rs/tui/src/chatwidget/tests.rs`

**Add tests:**
```rust
#[test]
fn spec_auto_completes_single_stage() {
    // Mock guardrail success
    // Trigger /spec-auto
    // Verify agents spawn
    // Verify consensus checked
    // Verify advance to next stage
}

#[test]
fn spec_auto_halts_on_guardrail_failure() {
    // Mock guardrail exit 1
    // Verify pipeline halts
    // Verify error message shown
}

#[test]
fn spec_auto_halts_on_consensus_conflict() {
    // Mock agents with conflicts
    // Verify synthesis shows conflict
    // Verify pipeline halts
}

#[test]
fn spec_auto_cancellation() {
    // Start pipeline
    // Simulate Ctrl-C
    // Verify clean halt
}
```

**Effort:** 6-8 hours
**Risk:** Medium - mocking guardrails/agents

---

#### Task 4.2: Rebase validation test suite

**File:** `scripts/test_fork_deviations.sh`

**Create:**
```bash
#!/usr/bin/env bash
# Validate spec-kit TUI code after upstream rebases

set -euo pipefail

echo "=== Fork Deviation Validation ==="

# 1. Check FORK-SPECIFIC guards present
echo "Checking fork guards..."
if ! grep -q "FORK-SPECIFIC: spec-kit automation" codex-rs/tui/src/chatwidget.rs; then
    echo "ERROR: Fork guards missing in chatwidget.rs"
    exit 1
fi

# 2. Verify spec-auto state machine exists
echo "Checking state machine..."
if ! grep -q "struct SpecAutoState" codex-rs/tui/src/chatwidget.rs; then
    echo "ERROR: SpecAutoState missing"
    exit 1
fi

# 3. Verify guardrail completion handler
echo "Checking guardrail handler..."
if ! grep -q "auto_submit_spec_stage_prompt" codex-rs/tui/src/chatwidget/exec_tools.rs; then
    echo "ERROR: Guardrail completion handler missing"
    exit 1
fi

# 4. Compile check
echo "Compiling TUI..."
cd codex-rs
if ! cargo build --profile dev-fast -p codex-tui 2>&1 | tee /tmp/tui-build.log; then
    echo "ERROR: TUI compilation failed"
    cat /tmp/tui-build.log
    exit 1
fi

# 5. Run spec-auto tests
echo "Running spec-auto tests..."
if ! cargo test -p codex-tui spec_auto 2>&1 | tee /tmp/tui-test.log; then
    echo "ERROR: Tests failed"
    cat /tmp/tui-test.log
    exit 1
fi

echo "✓ All fork deviation validations passed"
```

**Usage:** Run after every upstream rebase

**Effort:** 2 hours
**Risk:** Low

---

## Part 3: Fork Deviation Tracking

### Current Deviations (Session: 2025-10-05/06)

**Commit range:** `1a495650e..d2c400beb` (12 commits, +2615 lines)

#### Critical TUI Files

**`codex-rs/tui/src/chatwidget.rs`** (+304 lines)
- Line 548-551: `spec_auto_state: Option<SpecAutoState>` field
- Lines 17138-17700: Spec-auto pipeline methods
- Lines 16612-17136: State machine structures
- **Markers:** FORK-SPECIFIC guards added

**`codex-rs/tui/src/chatwidget/exec_tools.rs`** (+33 lines)
- Lines 771-801: Guardrail completion → agent trigger
- **Markers:** FORK-SPECIFIC guard added

**`codex-rs/tui/src/slash_command.rs`** (+36 lines)
- Lines 122-158: Spec command enum variants
- Lines 188-215: Spec command descriptions
- Lines 265-301: spec_ops() metadata method
- Lines 303-312: spec_stage() mapping
- Lines 428-540: SpecAuto parsing
- **Markers:** FORK-SPECIFIC guards added

**`codex-rs/tui/src/spec_prompts.rs`** (entire file, +~200 lines)
- Prompts.json parser
- Template variable injection
- Agent prompt rendering
- **New file, no upstream equivalent**

**`codex-rs/tui/src/app.rs`** (+10 lines)
- Lines 1596-1610: SpecOps command routing
- Line 4569-4571: SpecAuto command routing
- **Minor additions**

#### Non-TUI Changes (Safe)

**`scripts/spec_ops_004/`** (entirely fork-only)
- All bash guardrail scripts
- consensus_runner.sh
- check_synthesis.py
- generate_spec_id.py
- **Zero upstream conflict risk**

**`docs/spec-kit/`** (entirely fork-only)
- prompts.json
- Documentation
- **Zero upstream conflict risk**

**`.github/codex/home/config.toml`** (+29 lines)
- /new-spec subagent
- /spec-auto subagent (unused, keeping for reference)
- **User config, not in upstream**

---

### Rebase Strategy

#### Before Rebase

1. **Tag current state:**
   ```bash
   git tag spec-kit-pre-rebase-$(date +%Y%m%d)
   git push origin spec-kit-pre-rebase-$(date +%Y%m%d)
   ```

2. **Update spec-kit-base branch:**
   ```bash
   git checkout spec-kit-base
   git merge feat/spec-auto-telemetry
   git push origin spec-kit-base
   ```

3. **Review upstream changes:**
   ```bash
   git fetch upstream
   git log main..upstream/main --oneline
   git diff main..upstream/main -- codex-rs/tui/src/chatwidget.rs
   ```

4. **Identify conflict zones:**
   - If upstream touched chatwidget.rs around lines 16000-18000: HIGH RISK
   - If upstream modified exec_tools.rs: MEDIUM RISK
   - If upstream added new slash commands: LOW RISK (different enum area)

---

#### During Rebase

**Command:**
```bash
git rebase upstream/main
```

**Conflict resolution:**

**Scenario 1: chatwidget.rs conflict**
```bash
# Accept upstream for all non-FORK-SPECIFIC code
git show :3:codex-rs/tui/src/chatwidget.rs > theirs.rs  # Upstream

# Extract our spec-kit sections:
grep -A 9999 "=== FORK-SPECIFIC" codex-rs/tui/src/chatwidget.rs > fork-sections.txt

# Merge:
cp theirs.rs codex-rs/tui/src/chatwidget.rs

# Re-inject fork sections at correct locations:
# 1. spec_auto_state field (around line 548)
# 2. State machine structures (around line 16612)
# 3. Pipeline methods (around line 17138)

git add codex-rs/tui/src/chatwidget.rs
```

**Scenario 2: New upstream features overlap**
- Read upstream change intent
- Adapt FORK-SPECIFIC code to work with new upstream structure
- Test compilation after each resolution

---

#### After Rebase

**Validation checklist:**

1. **Compile check:**
   ```bash
   ./scripts/test_fork_deviations.sh
   ```

2. **Manual test:**
   ```bash
   cd /home/thetu/code
   ./codex-rs/target/dev-fast/code

   # In TUI:
   /new-spec Test rebase functionality
   /spec-auto SPEC-KIT-030-test-rebase-functionality
   ```

3. **Verify telemetry:**
   ```bash
   ls docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-030*/
   # Should see: baseline*.md, spec-plan*.json, spec-plan*.log
   ```

4. **Verify consensus:**
   ```bash
   ls docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-030*/
   # Should see: synthesis.json, telemetry.jsonl, per-agent JSON
   ```

5. **Verify agents used config:**
   ```bash
   /agents  # Check which are enabled
   # Run /spec-auto, verify those agents were used
   ```

**If any fail:** Rollback, analyze, re-rebase with fixes.

---

### Rebase Frequency

**Monthly minimum:**
- Check upstream for major changes
- Test rebase on throwaway branch first
- Document new conflicts in this file

**Before major spec-kit changes:**
- Sync with upstream first
- Reduces merge complexity later

---

## Part 4: Implementation Tasks (Detailed)

### Week 1: Streaming & Visibility

| Day | Task | Hours | Risk | Validation |
|-----|------|-------|------|------------|
| Mon | 1.1: Streaming guardrail output | 4 | Med | See bash output in TUI |
| Mon | 1.2: Progress messages | 1 | Low | See "Running X..." messages |
| Tue | 1.3: Parse/display results | 3 | Low | See baseline/policy results |
| Tue | 2.1: Progress indicator | 4 | Low | See "Stage 2/6" in status bar |
| Wed | 2.2: Cancellation support | 2 | Low | Ctrl-C halts cleanly |
| Wed | Testing: Phase 1+2 | 4 | - | Full visibility working |

### Week 2: Advanced Features & Testing

| Day | Task | Hours | Risk | Validation |
|-----|------|-------|------|------------|
| Thu | 3.1: Guardrail substep streaming | 8 | Med | See "✓ Baseline passed" live |
| Fri | 4.1: Integration tests | 8 | Med | `cargo test spec_auto` passes |
| Sat | 4.2: Rebase validation script | 2 | Low | Script validates deviations |
| Sat | Testing: Full pipeline E2E | 4 | - | plan→unlock completes |
| Sun | Documentation & cleanup | 4 | - | FORK_DEVIATIONS.md updated |

**Total:** 44 hours = ~6 working days

---

## Part 5: Test Strategy

### Unit Tests

**`codex-rs/tui/src/chatwidget/tests.rs`**

```rust
mod spec_auto_tests {
    use super::*;

    #[test]
    fn state_machine_transitions() {
        // Guardrail → ExecutingAgents → CheckingConsensus → NextStage
    }

    #[test]
    fn agent_completion_detection() {
        // All agents complete → consensus check triggered
    }

    #[test]
    fn consensus_conflict_halts() {
        // synthesis.json status=conflict → pipeline stops
    }

    #[test]
    fn guardrail_failure_halts() {
        // Exit code 1 → error shown, pipeline stops
    }

    #[test]
    fn six_stage_pipeline() {
        // Mock all stages → verify completion
    }
}
```

### Integration Tests

**`codex-rs/tui/tests/spec_auto_integration.rs`** (new file)

```rust
#[tokio::test]
async fn full_pipeline_with_mocked_guardrails() {
    // Create temp SPEC
    // Mock guardrail success for all stages
    // Mock agent outputs
    // Trigger /spec-auto
    // Verify: 6 stages execute, synthesis written, no halts
}

#[tokio::test]
async fn pipeline_halts_on_first_conflict() {
    // Stage 1 OK, stage 2 conflict
    // Verify: halts at stage 2, doesn't run stage 3
}
```

### Manual Validation Checklist

After implementation complete:

- [ ] `/new-spec` creates valid SPEC package
- [ ] `/spec-auto` shows streaming guardrail output
- [ ] Agents spawn visibly with correct models
- [ ] Agent progress visible
- [ ] Consensus results shown clearly
- [ ] Auto-advances on success
- [ ] Halts on guardrail failure with error
- [ ] Halts on consensus conflict with details
- [ ] Can cancel with Ctrl-C
- [ ] Progress indicator shows current stage
- [ ] `/agents` configuration is respected
- [ ] Evidence/telemetry written correctly
- [ ] Rebase validation script passes

---

## Part 6: Rollback/Fallback Plan

**If TUI implementation hits blockers:**

1. **Streaming output issue:**
   - Keep background exec, add progress polling (Task 3.1 only)
   - Accept less-than-perfect visibility

2. **Event wiring too complex:**
   - Simplify to manual advancement (remove auto-advance)
   - User runs `/spec-plan`, then `/spec-tasks`, etc.

3. **Integration test failures:**
   - Ship without tests, add later
   - Manual testing only

4. **Upstream conflicts unresolvable:**
   - Maintain fork as separate repo
   - Stop rebasing, merge upstream selectively

**Nuclear option:** Revert all TUI changes, keep bash `/spec-ops-auto` only
- Evidence: All commits tagged as `spec-kit-*`
- Rollback: `git revert <range>`
- Loss: Visibility, but bash automation still works

---

## Part 7: Success Criteria

**Minimum viable (ship-worthy):**
- [ ] Guardrail output visible during execution (not just at end)
- [ ] Agents spawn and show progress
- [ ] Consensus results displayed
- [ ] Auto-advances through stages
- [ ] Halts correctly on failures
- [ ] Rebase validation script passes

**Nice to have (polish):**
- [ ] Substep progress ("✓ Baseline passed")
- [ ] Stage X/6 progress indicator
- [ ] Ctrl-C cancellation
- [ ] Full integration test suite

**Definition of done:**
Run `/spec-auto SPEC-KIT-025-...` and see:
```
Running plan guardrail validation...
  ⏳ Baseline checks
  ⏳ Policy validation
  ✓ Baseline passed
  ✓ Policy passed
Auto-executing multi-agent plan...
  Starting Gemini... [progress bar]
  Starting Claude... [progress bar]
  Starting GPT Pro... [progress bar]
Checking consensus for plan...
  ✓ 15 agreements, 0 conflicts
  ✓ Plan consensus validated
Advancing to tasks stage (2/6)...
[repeat for all stages]
Pipeline complete. All stages validated.
```

---

## Current Session Commits (Need Review)

```
d2c400beb fix(guardrails): workspace-write sandbox for policy
7cb1b439b docs(spec): core automation complete
f39717adc chore(tui): comment update
7136dcc2f feat(tui): wire guardrail → agent trigger  ← CRITICAL
d215923ad fix(tui): update description
19846109a feat(orchestrator): subagent (REMOVE - not using)
e6bbcafba fix(tui): use configured agents  ← KEEP
0bd04fc6c feat(tui): fork-specific guards  ← KEEP
86de2037b docs(fork): rebase guards  ← KEEP
a0e3340b6 chore(spec-kit): sync artifacts
842232c96 feat(spec-kit): /new-spec command  ← KEEP
1f281c73a feat(spec-auto): bash consensus  ← KEEP (fallback)
1a495650e feat(spec-kit): telemetry metrics
```

**Action:** Squash orchestrator commits (not using), keep TUI wiring.

---

## Immediate Next Steps

**Today:**
1. Implement Task 1.1 (streaming guardrail output) - 4 hours
2. Test visibility improvement
3. If works: continue to Task 1.2-1.3
4. If blocked: document issue, reassess

**This week:**
- Complete Phase 1 (streaming)
- Complete Phase 2 (progress/control)
- Ship visible automation

**Next week:**
- Phase 3 (enhanced visibility)
- Phase 4 (testing)
- Create rebase validation suite

**Ready to start Task 1.1?**
