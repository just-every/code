# Step 1.4: Handler Method Extraction - Detailed Plan

**Current State:** Steps 1.1-1.3 complete, chatwidget.rs reduced by 223 lines
**Remaining:** Extract ~2,500 lines of handler methods
**Estimated Time:** 3-4 hours

---

## Methods to Extract (10 total)

### Location and Size Analysis

| Line | Method | Approx Size | Priority |
|------|--------|-------------|----------|
| 14857 | `handle_spec_ops_command` | ~200 lines | High |
| 15075 | `handle_spec_status_command` | ~30 lines | High |
| 15109 | `handle_spec_consensus_command` | ~75 lines | High |
| 17005 | `handle_spec_auto_command` | ~30 lines | Critical |
| 17036 | `advance_spec_auto` | ~150 lines | Critical |
| 17184 | `on_spec_auto_task_started` | ~10 lines | Medium |
| 17194 | `on_spec_auto_task_complete` | ~220 lines | Critical |
| 17418 | `halt_spec_auto_with_error` | ~25 lines | Medium |
| 17446 | `on_spec_auto_agents_complete` | ~65 lines | Critical |
| 17513 | `check_consensus_and_advance_spec_auto` | ~170 lines | Critical |

**Tests start at line 17682 - DO NOT extract**

**Total estimate:** ~975 lines to extract (conservative)

---

## Extraction Strategy

### Option A: Full ChatWidget Access (Recommended)

**Handler signature:**
```rust
impl SpecKitHandler {
    pub fn handle_ops(&mut self, widget: &mut ChatWidget, args: &str, command: SlashCommand) {
        // Original handle_spec_ops_command body here
        // Access widget.config, widget.history, etc. directly
    }
}
```

**Delegation in chatwidget.rs:**
```rust
pub(crate) fn handle_spec_ops_command(
    &mut self,
    command: SlashCommand,
    args: String,
    hal_mode: Option<HalMode>,
) {
    self.spec_kit.handle_ops(self, &args, command);
}
```

**Pros:** Simple, direct, no context struct needed
**Cons:** Handler has full ChatWidget access (less encapsulation)

---

## Extraction Order (By Dependency)

### Phase A: Independent Methods First

1. **handle_spec_status_command** (line 15075, ~30 lines)
   - No dependencies on spec_auto_state
   - Calls native status dashboard
   - Easiest to extract

2. **handle_spec_consensus_command** (line 15109, ~75 lines)
   - Uses local-memory lookups
   - Independent of pipeline state

3. **halt_spec_auto_with_error** (line 17418, ~25 lines)
   - Cleanup method
   - Called by others but doesn't call them

### Phase B: Pipeline Core Methods

4. **handle_spec_auto_command** (line 17005, ~30 lines)
   - Entry point for /speckit.auto
   - Calls advance_spec_auto

5. **advance_spec_auto** (line 17036, ~150 lines)
   - Core state machine
   - Calls multiple helpers

6. **on_spec_auto_task_started** (line 17184, ~10 lines)
   - Simple state update

7. **on_spec_auto_task_complete** (line 17194, ~220 lines)
   - Complex logic
   - Calls check_consensus_and_advance_spec_auto

8. **on_spec_auto_agents_complete** (line 17446, ~65 lines)
   - Agent completion handler
   - Calls check_consensus

9. **check_consensus_and_advance_spec_auto** (line 17513, ~170 lines)
   - Consensus checking logic
   - Calls advance_spec_auto (circular dependency)

### Phase C: Spec Ops Command

10. **handle_spec_ops_command** (line 14857, ~200 lines)
    - Guardrail command handler
    - Complex argument parsing
    - Environment setup

---

## Step-by-Step Execution

### 1. Add SpecKitHandler Field to ChatWidget

**Find ChatWidget struct definition:**
```bash
grep -n "^pub struct ChatWidget" codex-rs/tui/src/chatwidget.rs
```

**Add field:**
```rust
pub struct ChatWidget<'a> {
    // ... existing fields ...

    // === FORK-SPECIFIC: spec-kit automation handler ===
    spec_kit: SpecKitHandler,
}
```

**Update ChatWidget::new() and new_from_existing():**
Add initialization: `spec_kit: SpecKitHandler::new()`

---

### 2. Extract Methods One-by-One

**For each method:**

**A. Read method from chatwidget.rs:**
```bash
# Example for handle_spec_status_command
sed -n '15075,15105p' codex-rs/tui/src/chatwidget.rs
```

**B. Move to handler.rs:**
```rust
impl SpecKitHandler {
    pub fn handle_status(&mut self, widget: &mut ChatWidget, raw_args: String) {
        // Paste method body here
        // Change self.field → widget.field
    }
}
```

**C. Replace in chatwidget.rs:**
```rust
pub(crate) fn handle_spec_status_command(&mut self, raw_args: String) {
    self.spec_kit.handle_status(self, raw_args);
}
```

**D. Test compilation:**
```bash
cargo build -p codex-tui --profile dev-fast
```

**E. Fix errors (field access, etc.)**

**F. Commit when compilation succeeds:**
```bash
git add -A
git commit -m "refactor(spec-kit): extract handle_spec_status to handler module"
```

---

### 3. Iteration Strategy

**Do NOT extract all at once.** Extract in batches:

**Batch 1:** Simple methods (3 methods, ~130 lines)
- handle_spec_status_command
- handle_spec_consensus_command
- halt_spec_auto_with_error

**Batch 2:** Pipeline methods (5 methods, ~645 lines)
- handle_spec_auto_command
- advance_spec_auto
- on_spec_auto_task_started
- on_spec_auto_task_complete
- on_spec_auto_agents_complete
- check_consensus_and_advance_spec_auto

**Batch 3:** Spec ops command (~200 lines)
- handle_spec_ops_command

**Commit after each batch** to maintain granular history.

---

## Compilation Issues to Expect

### Issue 1: Field Access

**Error:**
```
error[E0609]: no field `config` on type `&mut SpecKitHandler`
```

**Fix:** Change `self.config` → `widget.config` in handler method

### Issue 2: Method Calls

**Error:**
```
error[E0599]: no method named `app_event_tx` on type `&mut SpecKitHandler`
```

**Fix:** Change `self.app_event_tx.send(...)` → `widget.app_event_tx.send(...)`

### Issue 3: spec_auto_state Access

**Error:**
```
error[E0609]: no field `spec_auto_state` on type `&mut ChatWidget`
```

**Fix:** Change to use `self.state` (SpecKitHandler field) or `widget.spec_kit.state`

---

## Success Criteria

**After all extractions:**
- [ ] chatwidget.rs has only delegation methods (5-10 lines each)
- [ ] All 10 methods in SpecKitHandler
- [ ] Compilation successful
- [ ] chatwidget.rs reduced from 22,847 to ~20,350 lines (~2,500 line reduction)
- [ ] All functionality preserved (no behavior changes)

---

## Quick Reference Commands

**Find method boundaries:**
```bash
# Show method signature and first 5 lines
sed -n '14857,14862p' codex-rs/tui/src/chatwidget.rs

# Count lines in a method (manual inspection needed for closing brace)
```

**Test compilation:**
```bash
cd codex-rs
cargo build -p codex-tui --profile dev-fast
```

**Check chatwidget line count:**
```bash
wc -l codex-rs/tui/src/chatwidget.rs
```

---

**Document Version:** 1.0
**Status:** Ready for execution
**Owner:** @just-every/automation
