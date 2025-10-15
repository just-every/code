# Refactoring Blocker Resolution: Rust Borrow Checker

**Date:** 2025-10-15
**Issue:** Cannot delegate to handler pattern due to Rust borrow rules
**Status:** ARCHITECTURAL BLOCKER - Refactoring approach invalid

---

## The Fundamental Problem

**Attempted pattern:**
```rust
struct ChatWidget {
    spec_kit: SpecKitHandler,
    // ... other fields
}

impl ChatWidget {
    fn handle_spec_status_command(&mut self, args: String) {
        self.spec_kit.handle_status(self, args);  // ERROR!
    }
}
```

**Error:**
```
error[E0499]: cannot borrow `self.spec_kit` as mutable because `*self` is also borrowed as mutable
```

**Why:** Rust prevents `self.field.method(self, ...)` - cannot borrow a field and the whole struct simultaneously.

---

## Why All Delegation Patterns Fail

**Pattern 1: Handler as field**
```rust
self.spec_kit.handle(self, args)  // ❌ Borrow checker error
```

**Pattern 2: Static handler**
```rust
SpecKitHandler::handle(&mut self.spec_kit, self, args)  // ❌ Same issue
```

**Pattern 3: Separate state**
```rust
SpecKitHandler::handle(self, args)  // ❌ Still borrowing self + self.field
```

**Conclusion:** Any pattern that passes `&mut ChatWidget` to a method called ON a ChatWidget field violates Rust borrowing rules.

---

## Valid Approaches

### Approach 1: Keep Methods Inline (Accept Original State)

**Abandon extraction, keep all methods in ChatWidget impl**

**Pros:**
- No borrow checker issues
- Works immediately
- Code already functional

**Cons:**
- 14,112 insertions remain in chatwidget.rs
- Rebase conflicts guaranteed
- Original problem unsolved

**Verdict:** Defeats entire purpose of refactoring

---

### Approach 2: Free Functions (Not Methods)

**Instead of:**
```rust
impl SpecKitHandler {
    pub fn handle_status(&mut self, widget: &mut ChatWidget, args: String) { ... }
}
```

**Use:**
```rust
pub fn handle_spec_status(widget: &mut ChatWidget, args: String) {
    // Access widget.spec_auto_state directly
    // No self.spec_kit needed
}
```

**Call from ChatWidget:**
```rust
impl ChatWidget {
    pub(crate) fn handle_spec_status_command(&mut self, args: String) {
        spec_kit::handle_spec_status(self, args);
    }
}
```

**Pros:**
- No borrow checker issues (no self.spec_kit.method pattern)
- Methods isolated in spec_kit module
- Reduces rebase conflicts (separate file)

**Cons:**
- SpecAutoState stored in ChatWidget, not SpecKitHandler
- Handler is stateless (just a namespace for functions)
- Less OOP, more functional

**Verdict:** VIABLE - achieves goal of isolating code

---

### Approach 3: RefCell Interior Mutability

```rust
struct ChatWidget {
    spec_kit: RefCell<SpecKitHandler>,
}

impl ChatWidget {
    fn handle(&mut self, args: String) {
        let mut handler = self.spec_kit.borrow_mut();
        handler.handle_status(self, args);  // Still breaks - self already borrowed
    }
}
```

**Verdict:** Doesn't solve borrow checker issue

---

## Recommended: Approach 2 (Free Functions)

**Implementation:**

**spec_kit/handler.rs:**
```rust
use super::ChatWidget;  // Friend access

/// Handle /speckit.status command
pub fn handle_spec_status(widget: &mut ChatWidget, raw_args: String) {
    // Full implementation here
    // Access widget.config, widget.spec_auto_state, etc.
}

/// Halt /speckit.auto with error
pub fn halt_spec_auto_with_error(widget: &mut ChatWidget, reason: String) {
    // Implementation
}

// ... 8 more functions
```

**chatwidget/mod.rs:**
```rust
mod spec_kit;

impl ChatWidget {
    pub(crate) fn handle_spec_status_command(&mut self, raw_args: String) {
        spec_kit::handle_spec_status(self, raw_args);  // ✅ Works!
    }

    fn halt_spec_auto_with_error(&mut self, reason: String) {
        spec_kit::halt_spec_auto_with_error(self, reason);  // ✅ Works!
    }
}
```

**Result:**
- Methods isolated in spec_kit/handler.rs (separate file from chatwidget/mod.rs)
- No borrow checker issues
- Reduces rebase conflicts (changes in separate file)
- ChatWidget has only delegation (5 lines per method)

**Trade-off:**
- Handler is stateless (SpecAutoState remains in ChatWidget.spec_auto_state)
- Less "object-oriented" (free functions vs methods)
- But achieves the actual goal: isolate code, reduce conflicts

---

## Updated Refactoring Plan

**Change from:**
> Extract to SpecKitHandler with state field, call via self.spec_kit.method(self)

**Change to:**
> Extract to free functions in spec_kit module, call via spec_kit::function(self)

**Implementation:**
1. Remove SpecKitHandler struct (not needed)
2. spec_kit/handler.rs contains free functions
3. spec_kit/state.rs contains state structs (already done)
4. ChatWidget.spec_auto_state field (keep as-is)
5. Delegation methods call spec_kit::handle_*

**Estimated time:** Same as original (3-4 hours for extraction)
**Benefit:** Actually compiles, achieves goal

---

## Decision

Proceed with Approach 2 (free functions)?

**Next steps if yes:**
1. Remove SpecKitHandler struct
2. Convert handler.rs to free functions
3. Extract 10 methods as free functions
4. Test compilation
5. Commit

Time: 3-4 hours for full extraction

---

**Document Version:** 1.0
**Status:** Architectural decision required
**Recommendation:** Proceed with free functions approach
