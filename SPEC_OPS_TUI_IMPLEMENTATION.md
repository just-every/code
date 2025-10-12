# Spec-Ops TUI Native Implementation Plan

## Goal
Make ALL `/spec-ops-*` and `/spec-status` commands visible and native in TUI (not background execution).

**Target commands:**
- /spec-ops-plan
- /spec-ops-tasks
- /spec-ops-implement
- /spec-ops-validate
- /spec-ops-audit
- /spec-ops-unlock
- /spec-ops-auto
- /spec-status

## Current Problem

**All commands use `RunProjectCommand`:**
```rust
self.submit_op(Op::RunProjectCommand {
    name: format!("spec_ops_{}", meta.display),
    command: Some(wrapped),
    ...
});
```

**Result:**
- Executes in background submission context
- Events don't appear in TUI conversation
- User sees metadata banner only
- Output invisible until completion

## Solution Architecture

**Option A: Stream RunProjectCommand Output (Preferred)**

Modify core to emit ExecCommandBegin/OutputDelta/End events for project commands.

**Files to modify:**
- `codex-rs/core/src/codex.rs` - run_project_command (line 2270)
- Already partially done: stdout_stream added, ExecCommandBegin emission attempted

**Remaining work:**
1. Ensure events route to main conversation (not background context)
2. Fix TUI event filtering (might be suppressing project command events)
3. Test visibility

**Effort:** 4-6 hours
**Risk:** Medium - core changes, event routing

---

**Option B: Native TUI Implementations (Cleaner)**

Replace bash script execution with pure TUI functions.

**For /spec-status:**
- Read files directly in Rust
- Format output
- Display in conversation
- No bash subprocess

**For /spec-ops-plan etc:**
- Call guardrail logic as Rust functions (not bash)
- OR keep bash but capture output differently

**Effort:** 2-3 days (rewrite logic in Rust)
**Risk:** High - reimplementing bash logic in Rust

---

## Recommended Approach: Hybrid

**Phase 1: Fix /spec-status (Native TUI) - 2 hours**

Implement as pure Rust function (already partially exists at line 15073).

**File:** `codex-rs/tui/src/chatwidget.rs`

**Add:**
```rust
pub(crate) fn handle_spec_status_command(&mut self, spec_id: &str) {
    // Read SPEC directory
    // Check file existence (PRD, spec, plan, tasks)
    // Check evidence (telemetry, consensus per stage)
    // Format as table
    // Display via history_push
}
```

**Testing:**
```bash
cargo build --profile dev-fast --bin code
/quit; code
/spec-status SPEC-KIT-045
# Should show formatted output in conversation
```

---

**Phase 2: Fix /spec-ops-* Visibility - 8-12 hours**

**Two sub-approaches:**

### Approach 2A: Make RunProjectCommand Visible (Faster)

**The issue:** Events emit but don't display in conversation.

**Debug steps:**
1. Add logging to run_project_command to confirm events emit
2. Check TUI event handler - find where ExecCommandBegin/End are processed
3. Verify events aren't filtered by sub_id or call_id patterns
4. Test with /spec-ops-plan

**Files:**
- `codex-rs/core/src/codex.rs` (line 2270 - run_project_command)
- `codex-rs/tui/src/chatwidget.rs` (line 6202 - ExecCommandEnd handler)
- `codex-rs/tui/src/chatwidget/exec_tools.rs` (line 579 - handle_exec_end_now)

**Implementation:**
```rust
// In run_project_command (codex.rs):
// Already done: on_exec_command_begin called
// Already done: stdout_stream passed

// In TUI event handler:
// Find where events for project commands are suppressed
// Remove filtering or add spec_ops_* exception
```

**Testing per command:**
```bash
/spec-ops-plan SPEC-KIT-045
# Should see bash execution cell with live output
```

---

### Approach 2B: Replace with Op::UserInput (Simpler)

**Make spec-ops commands submit as user messages** (like /spec-auto does).

**File:** `codex-rs/tui/src/chatwidget.rs` (line 14998)

**Change:**
```rust
pub(crate) fn handle_spec_ops_command(&mut self, command: SlashCommand, raw_args: String) {
    let Some(meta) = command.spec_ops() else { return; };

    // Build bash command
    let command_line = format!("bash scripts/spec_ops_004/commands/{} {}",
                               meta.script, raw_args);

    // Submit as user message - agent executes visibly
    let user_msg = UserMessage {
        display_text: format!("/spec-ops-{} {}", meta.display, raw_args),
        ordered_items: vec![InputItem::Text {
            text: format!("Run this command:\n{}", command_line)
        }],
    };
    self.submit_user_message(user_msg);
}
```

**Pros:** Definitely visible (agent execution always shows)
**Cons:** Adds message to conversation, uses agent turn

**Effort:** 1 hour
**Risk:** Low

---

## Implementation Timeline

### Day 1: /spec-status + Choose Approach
- **Morning (2h):** Implement native /spec-status
- **Afternoon (4h):** Test Approach 2A vs 2B with /spec-ops-plan
- **Evening (2h):** Apply winning approach to all 7 commands

### Day 2: Testing & Refinement
- **Morning (4h):** Test each command individually
- **Afternoon (4h):** Test /spec-ops-auto full pipeline
- **Evening (2h):** Fix bugs, edge cases

### Day 3: Polish & Validation
- **Morning (2h):** Add progress indicators
- **Afternoon (3h):** Integration tests
- **Evening (1h):** Documentation

**Total: 24 hours across 3 days**

---

## Acceptance Criteria

**Per command must:**
- [ ] Display output in TUI conversation (not background)
- [ ] Show bash execution cell with live output
- [ ] Completion visible (exit code, duration)
- [ ] Can be interrupted (Ctrl-C works)
- [ ] Errors surface immediately (not hidden)

**For /spec-status specifically:**
- [ ] Shows formatted table in conversation
- [ ] No bash subprocess
- [ ] Updates instantly (< 100ms)

**For /spec-ops-auto:**
- [ ] All 6 stages visible
- [ ] Progress indicator (Stage X/6)
- [ ] Can resume from any stage
- [ ] Errors halt with clear message

---

## Quick Start (Do This First)

**Test current state:**
```bash
# Does plan guardrail create evidence?
bash scripts/spec_ops_004/commands/spec_ops_plan.sh SPEC-KIT-045-mini
ls docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-045*/

# If files exist → commands work, just invisible
# If no files → commands broken
```

**If commands work (just invisible):**
→ Use Approach 2B (UserMessage submission) - 1 day

**If commands broken:**
→ Debug bash scripts first, then visibility

---

## Files to Modify (Approach 2B - Fastest)

1. `codex-rs/tui/src/chatwidget.rs` (line 14863)
   - Modify `handle_spec_ops_command`
   - Change from RunProjectCommand to UserMessage
   - ~20 lines

2. `codex-rs/tui/src/chatwidget.rs` (new function)
   - Add native `handle_spec_status_command`
   - ~80 lines

3. `codex-rs/tui/src/app.rs` (line 1607)
   - Route SpecStatus to native handler
   - ~5 lines

**Total changes:** ~105 lines
**Files:** 2
**Build time:** ~30 seconds
**Test cycle:** 2 minutes per iteration

---

## Testing Strategy

**Per command:**
```bash
/spec-ops-plan SPEC-KIT-045-mini
# Verify: See bash output, not just metadata

/spec-ops-tasks SPEC-KIT-045-mini
# Verify: See bash output

# ... repeat for all commands

/spec-status SPEC-KIT-045-mini
# Verify: See formatted status table

/spec-ops-auto SPEC-KIT-045-mini --from plan
# Verify: All stages visible
```

**Success criteria:**
- All commands show output in < 5 seconds
- No "Waiting..." with hidden execution
- Can see what's happening
- Can interrupt execution

---

## Current Blockers for SPEC-KIT-045

From your message: "hitting blockers due to these problems"

**What's blocked:**
1. Can't see /spec-ops-plan output → don't know if it succeeded
2. Can't see /spec-status → don't know current state
3. Can't debug failures → no visible errors

**Once visibility fixed:**
- Can iterate rapidly on SPEC-KIT-045
- See what each stage produces
- Debug issues immediately
- Complete the test framework SPEC

---

## Immediate Next Steps

**Now:**
1. I implement Approach 2B (UserMessage for spec-ops)
2. I implement native spec-status
3. You test in TUI
4. We iterate until working

**Estimated:** 4-6 hours to working state

**Want me to start implementing now?**
