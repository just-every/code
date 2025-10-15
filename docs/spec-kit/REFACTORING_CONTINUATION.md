# Refactoring Phase 1 - Continuation Guide

**Date:** 2025-10-15
**Branch:** refactor/spec-kit-module-extraction
**Last Commit:** 291eb0356
**Status:** Steps 1.1-1.3 complete, Step 1.4 ready for method extraction

---

## Current Progress

### Completed (6 commits)

1. **892d1e4a2** - Module structure created
2. **6674802b3** - Session notes
3. **3448e2bcb** - State.rs with 245 lines extracted
4. **872a9e03c** - Removed 223 duplicate lines from chatwidget
5. **46b143010** - Updated session notes
6. **291eb0356** - Added spec_kit field to ChatWidget

### Achievements

- ✅ chatwidget.rs reduced: 23,028 → 22,847 lines (181 line reduction)
- ✅ spec_kit/state.rs: Complete (245 lines)
- ✅ spec_kit/handler.rs: Skeleton ready
- ✅ spec_kit field added to ChatWidget
- ✅ All compilation tests passing

---

## Next: Method Extraction (3-4 hours)

### 10 Methods to Extract

**Located at these lines in chatwidget.rs:**

1. **handle_spec_ops_command** - Line 14857 (~200 lines)
2. **handle_spec_status_command** - Line 15075 (~30 lines)
3. **handle_spec_consensus_command** - Line 15109 (~75 lines)
4. **handle_spec_auto_command** - Line 17005 (~30 lines)
5. **advance_spec_auto** - Line 17036 (~150 lines)
6. **on_spec_auto_task_started** - Line 17184 (~10 lines)
7. **on_spec_auto_task_complete** - Line 17194 (~220 lines)
8. **halt_spec_auto_with_error** - Line 17418 (~25 lines)
9. **on_spec_auto_agents_complete** - Line 17446 (~65 lines)
10. **check_consensus_and_advance_spec_auto** - Line 17513 (~170 lines)

**Total:** ~975 lines minimum (likely ~1,500 with full impl blocks)

---

## Extraction Process (Per Method)

### Example: handle_spec_status_command

**Step 1: Read the method**
```bash
sed -n '15075,15105p' /home/thetu/code/codex-rs/tui/src/chatwidget.rs
```

**Step 2: Add to handler.rs**
```rust
impl SpecKitHandler {
    pub fn handle_status(&mut self, widget: &mut ChatWidget, raw_args: String) {
        // Paste method body
        // Change self.field → widget.field
        // Change self.method() → widget.method()
    }
}
```

**Step 3: Replace in chatwidget.rs with delegation**
```rust
pub(crate) fn handle_spec_status_command(&mut self, raw_args: String) {
    self.spec_kit.handle_status(self, raw_args);
}
```

**Step 4: Test**
```bash
cd /home/thetu/code/codex-rs
cargo build -p codex-tui --profile dev-fast
```

**Step 5: Fix any field access errors**

**Step 6: Commit**
```bash
git add -A
git commit -m "refactor(spec-kit): extract handle_spec_status to handler module"
```

---

## Batch Strategy

### Batch 1: Simple Methods (Start Here)

**Methods:**
- handle_spec_status_command (line 15075)
- halt_spec_auto_with_error (line 17418)
- handle_spec_consensus_command (line 15109)

**Lines:** ~130 total
**Time:** 30-60 minutes
**Commit:** After all 3 extracted and tested

### Batch 2: Pipeline Methods

**Methods:**
- handle_spec_auto_command (line 17005)
- advance_spec_auto (line 17036)
- on_spec_auto_task_started (line 17184)
- on_spec_auto_task_complete (line 17194)
- on_spec_auto_agents_complete (line 17446)
- check_consensus_and_advance_spec_auto (line 17513)

**Lines:** ~670 total
**Time:** 2-3 hours
**Commit:** After all extracted and tested

### Batch 3: Spec Ops Command

**Methods:**
- handle_spec_ops_command (line 14857)

**Lines:** ~200
**Time:** 30-60 minutes
**Commit:** After extracted and tested

---

## Field Access Pattern Changes

### Common Replacements

| In chatwidget (self.*) | In handler (widget.*) |
|------------------------|----------------------|
| `self.app_event_tx` | `widget.app_event_tx` |
| `self.config` | `widget.config` |
| `self.spec_auto_state` | `self.state` or `widget.spec_auto_state` |
| `self.history_cells` | `widget.history_cells` |
| `self.bottom_pane` | `widget.bottom_pane` |

### Method Calls

| In chatwidget | In handler |
|---------------|-----------|
| `self.insert_history()` | `widget.insert_history()` |
| `self.request_redraw()` | `widget.request_redraw()` |
| `self.mark_needs_redraw()` | `widget.mark_needs_redraw()` |

---

## Compilation Validation

**After each batch:**
```bash
cd /home/thetu/code/codex-rs
cargo build -p codex-tui --profile dev-fast

# Should complete in ~25-35 seconds
# Only warnings acceptable, no errors
```

**Check line count reduction:**
```bash
wc -l codex-rs/tui/src/chatwidget.rs
# Target: ~20,350 after all extractions (from current 22,847)
```

---

## Expected Challenges

### Challenge 1: Circular Dependencies

**Issue:** Method A calls method B which calls method A

**Example:** `advance_spec_auto` ↔ `check_consensus_and_advance_spec_auto`

**Solution:** Both methods are in SpecKitHandler, so they can call each other via `self.method(widget)`

### Challenge 2: spec_auto_state Access

**Current:** `self.spec_auto_state`
**Handler has:** `self.state`
**Widget has:** `widget.spec_auto_state`

**Decision:** Use `self.state` in handler, sync with `widget.spec_auto_state` at method boundaries

### Challenge 3: Large Method Bodies

**Issue:** Methods like `on_spec_auto_task_complete` are 220+ lines

**Solution:** Extract as-is first, refactor internals later if needed

---

## Success Criteria

**Step 1.4 Complete When:**
- [ ] All 10 methods extracted to handler.rs
- [ ] All chatwidget methods replaced with 5-line delegation
- [ ] Compilation successful with no errors
- [ ] chatwidget.rs ~20,350 lines (2,500 line reduction from 22,847)
- [ ] 3 commits (one per batch)

**Then:** Phase 1 complete, proceed to Phase 2 (enum isolation)

---

## Session Resume Prompt

**For next session:**

```
Continue refactoring Step 1.4 - extract handler methods to SpecKitHandler.

Branch: refactor/spec-kit-module-extraction
Last commit: 291eb0356 (spec_kit field added)
Preparation: ✅ Complete

Task: Extract Batch 1 (simple methods) - 3 methods, ~130 lines
- handle_spec_status_command (line 15075)
- halt_spec_auto_with_error (line 17418)
- handle_spec_consensus_command (line 15109)

Reference: docs/spec-kit/REFACTORING_CONTINUATION.md (this file)
Pattern: Read method → Move to handler.rs → Replace with delegation → Test

Start with: sed -n '15075,15105p' codex-rs/tui/src/chatwidget.rs
```

---

**Document Version:** 1.0
**Status:** Ready for method extraction execution
**Owner:** @just-every/automation
