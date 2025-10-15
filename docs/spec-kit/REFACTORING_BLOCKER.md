# Refactoring Blocker: Private Field Access

**Date:** 2025-10-15
**Issue:** SpecKitHandler cannot access private ChatWidget fields/methods
**Impact:** Method extraction blocked until visibility resolved

---

## Problem

When extracting methods to `spec_kit::SpecKitHandler`, compilation fails:

```
error[E0624]: method `request_redraw` is private
error[E0616]: field `config` of struct `ChatWidget` is private
error[E0616]: field `spec_auto_state` of struct `ChatWidget` is private
```

**Root cause:** SpecKitHandler is in module `spec_kit`, ChatWidget fields are not `pub(crate)`

---

## Solution Options

### Option A: Make ChatWidget fields pub(crate) (Quick Fix)

**Change:**
```rust
pub(crate) struct ChatWidget {
    pub(crate) config: Config,  // was private
    pub(crate) spec_auto_state: Option<SpecAutoState>,  // was private
    // ... make all accessed fields pub(crate)
}
```

**Pros:** Simple, minimal code changes
**Cons:** Breaks encapsulation (any TUI module can access internals)

---

### Option B: Keep Methods in ChatWidget Impl (Abandon Extraction)

**Revert extraction, keep everything inline**

**Pros:** No visibility issues
**Cons:** Defeats purpose (rebase conflicts remain)

---

### Option C: Create Public Accessor Methods

**Add to ChatWidget:**
```rust
impl ChatWidget {
    pub(crate) fn config(&self) -> &Config { &self.config }
    pub(crate) fn spec_auto_state(&self) -> &Option<SpecAutoState> { &self.spec_auto_state }
    pub(crate) fn spec_auto_state_mut(&mut self) -> &mut Option<SpecAutoState> { &mut self.spec_auto_state }
    pub(crate) fn request_redraw_public(&mut self) { self.request_redraw(); }
    // ... ~20 accessor methods needed
}
```

**Pros:** Maintains encapsulation
**Cons:** Verbose, ~20 accessor methods needed

---

### Option D: Friend Module Pattern (Best Practice)

**Make spec_kit a submodule of chatwidget:**
```
chatwidget/
├── mod.rs (ChatWidget struct)
├── spec_kit.rs (SpecKitHandler with access to super::ChatWidget private fields)
```

**In chatwidget/mod.rs:**
```rust
mod spec_kit;
pub(crate) use spec_kit::SpecKitHandler;

pub(crate) struct ChatWidget { ... } // fields stay private
```

**In chatwidget/spec_kit.rs:**
```rust
use super::ChatWidget; // Can access private fields (same module)

pub(crate) struct SpecKitHandler { ... }

impl SpecKitHandler {
    pub fn handle_status(&mut self, widget: &mut ChatWidget) {
        widget.config.cwd // Works! (same parent module)
    }
}
```

**Pros:** Proper encapsulation, no accessor boilerplate
**Cons:** Requires moving chatwidget.rs → chatwidget/mod.rs

---

## Recommended: Option D (Friend Module)

**Why:** Clean Rust idiom for extracting related functionality while preserving encapsulation

**Implementation:**
1. `mkdir codex-rs/tui/src/chatwidget`
2. `mv codex-rs/tui/src/chatwidget.rs codex-rs/tui/src/chatwidget/mod.rs`
3. `mv codex-rs/tui/src/spec_kit codex-rs/tui/src/chatwidget/spec_kit`
4. Update `lib.rs`: `mod chatwidget;` (no change needed)
5. SpecKitHandler can now access ChatWidget private fields (same parent module)

**Estimated time:** 15-30 minutes restructuring + retest

---

## Alternative: Option A (Pragmatic)

**If speed is priority:** Make fields `pub(crate)`, document why, proceed with extraction

**Trade-off:** Less clean but functional, gets refactoring done faster

---

## Decision Required

Which approach to take?

**Option A:** Make fields pub(crate) (30 min)
**Option D:** Restructure as chatwidget/mod.rs (30 min)

Both take similar time. Option D is cleaner long-term.

---

**Document Version:** 1.0
**Status:** Blocking Step 1.4 execution
**Resolution needed before continuing**
