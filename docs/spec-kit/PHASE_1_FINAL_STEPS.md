# Phase 1 Final Steps - Method Extraction Remaining

**Date:** 2025-10-15
**Status:** Architecture validated, 2/10 methods extracted
**Remaining:** 8 methods, ~1,500 lines, 2-3 hours

---

## Current State

**Branch:** refactor/spec-kit-module-extraction
**Commits:** 8 total (last: 57976196c)
**Architecture:** ✅ chatwidget/spec_kit friend module with free functions
**Compilation:** ✅ Successful
**Progress:** 52 lines removed so far (22,801 lines current)

---

## Extraction Pattern (PROVEN)

```bash
# 1. Read method from chatwidget/mod.rs
sed -n 'START,ENDp' codex-rs/tui/src/chatwidget/mod.rs

# 2. Add to chatwidget/spec_kit/handler.rs
pub fn method_name(widget: &mut ChatWidget, args: ArgType) {
    // Paste body
    // Change self.field → widget.field
    // Change self.method() → widget.method()
}

# 3. Replace in chatwidget/mod.rs
pub(crate) fn original_method_name(&mut self, args: ArgType) {
    spec_kit::method_name(self, args);
}

# 4. Update spec_kit/mod.rs exports
pub use handler::method_name;

# 5. Test
cd codex-rs && cargo build -p codex-tui --profile dev-fast

# 6. Commit
git add -A && git commit -m "refactor(spec-kit): extract METHOD_NAME"
```

---

## Remaining Methods (Prioritized)

### Batch 2: Consensus + Guardrail (515 lines)

**1. handle_spec_consensus_command (lines 15086-15400, 315 lines)**
- Includes helpers:
  - parse_consensus_stage (line 14014)
  - load_latest_consensus_synthesis (line 15136)
  - run_spec_consensus (line 15228)
- Extract all as free functions in handler.rs

**2. handle_spec_ops_command → handle_guardrail (lines 14857-15076, 200 lines)**
- Rename during extraction for clarity
- Handles both /guardrail.* and /spec-ops-* (legacy)

---

### Batch 3: Pipeline Methods (6 methods, ~800 lines)

**3. handle_spec_auto_command (line 17005, ~30 lines)**
**4. advance_spec_auto (line 17036, ~150 lines)**
**5. on_spec_auto_task_started (line 17184, ~10 lines)**
**6. on_spec_auto_task_complete (line 17194, ~220 lines)**
**7. on_spec_auto_agents_complete (line 17446, ~65 lines)**
**8. check_consensus_and_advance_spec_auto (line 17513, ~170 lines)**

**Dependencies:** Methods call each other, extract in dependency order

---

## Helper Method Locations

```bash
# Find all helpers
grep -n "fn.*consensus\|fn.*guardrail\|fn.*spec_auto" codex-rs/tui/src/chatwidget/mod.rs | grep -v "pub(crate)"
```

**Found:**
- parse_consensus_stage (14014)
- collect_consensus_artifacts (14026)
- persist_consensus_verdict (15521)
- persist_consensus_telemetry_bundle (15560)
- remember_consensus_verdict (15753)
- queue_consensus_runner (16610)

**Extract with main methods that use them**

---

## Execution Commands

### Extract Consensus Section (315 lines)

```bash
# Read the section
sed -n '15086,15400p' codex-rs/tui/src/chatwidget/mod.rs > /tmp/consensus.rs

# Add to handler.rs manually
# Then delete from chatwidget/mod.rs
# Replace with delegation
```

### Extract Guardrail Handler (200 lines)

```bash
sed -n '14857,15076p' codex-rs/tui/src/chatwidget/mod.rs > /tmp/guardrail.rs
# Add to handler.rs as handle_guardrail
# Delete from chatwidget
# Replace with delegation
```

---

## Expected Final State

**chatwidget/mod.rs:**
- Current: 22,801 lines
- After Batch 2: ~22,300 lines (-500)
- After Batch 3: ~21,000 lines (-1,800 total)

**chatwidget/spec_kit/handler.rs:**
- Current: 73 lines
- After complete: ~1,900 lines

**Net reduction in upstream file:** 14,112 → ~100 insertions (delegation only)

---

## Commit Strategy

**After Batch 2 (consensus + guardrail):**
```bash
git add -A
git commit -m "refactor(spec-kit): extract consensus and guardrail handlers (Batch 2)

Extracted:
- handle_spec_consensus_command + 3 helpers (315 lines)
- handle_spec_ops_command → handle_guardrail (200 lines)

chatwidget/mod.rs: 22,801 → ~22,300 lines
Compilation: ✅ Successful
"
```

**After Batch 3 (pipeline methods):**
```bash
git commit -m "refactor(spec-kit): extract pipeline automation methods (Batch 3)

Extracted 6 methods:
- handle_spec_auto_command
- advance_spec_auto
- on_spec_auto_task_started
- on_spec_auto_task_complete
- on_spec_auto_agents_complete
- check_consensus_and_advance_spec_auto

chatwidget/mod.rs: ~22,300 → ~21,000 lines
Total reduction: 1,800+ lines

Phase 1 COMPLETE: Upstream modifications reduced 99%
"
```

---

## Success Metrics

**Phase 1 Complete When:**
- [x] Module structure created
- [x] State extracted (245 lines)
- [x] Inline definitions removed (223 lines)
- [x] Architecture restructured (friend module)
- [x] 2 simple methods extracted (52 lines)
- [ ] Consensus section extracted (~315 lines)
- [ ] Guardrail handler extracted (~200 lines)
- [ ] Pipeline methods extracted (~800 lines)
- [ ] Final compilation successful
- [ ] chatwidget/mod.rs ~21,000 lines (1,800 line reduction)

**Then:** Proceed to Phase 2 (enum isolation) and Phase 3 (routing extraction)

---

## Resume Command

```
Continue Phase 1 extraction - Batch 2 (consensus + guardrail handlers).

Branch: refactor/spec-kit-module-extraction
Last commit: 57976196c
Progress: 2/10 methods done, 52 lines removed

Next: Extract handle_spec_consensus section (lines 15086-15400, 315 lines).
Pattern: Read → Add to handler.rs → Delete from chatwidget → Delegation → Test

Target: Remove ~515 more lines in Batch 2.
```

---

**Document Version:** 1.0
**Status:** Ready for Batch 2 execution
**Estimated Time:** 1-2 hours for Batch 2, 1-2 hours for Batch 3
