# Fork Isolation Audit - Rebase Conflict Surface Analysis

**Date:** 2025-10-15
**Branch:** feat/spec-auto-telemetry (79 commits)
**Upstream:** anthropics/claude-code
**Merge Base:** 2822aa525 (Sep 19, 2025)
**Divergence:** 798 files, 78,850 insertions, 12,139 deletions

---

## Executive Summary

**Actual Conflict Risk: 3 Critical Files**

Despite 798 files changed, the real rebase conflict surface is:
1. **chatwidget.rs** - 14,112 insertions (🔴 CRITICAL)
2. **app.rs** - 1,546 insertions (🟠 HIGH)
3. **slash_command.rs** - 632 insertions (🟠 HIGH)

**Total:** ~16,290 lines of inline modifications to upstream code

**Remaining 782 files:**
- 529 new files (templates/, docs/, scripts/) - Zero conflict risk
- ~250 files modified by cargo fmt only - Low conflict risk (auto-merge)
- ~3 files with minor back-compat shims - Low conflict risk

**Refactoring Goal:** Reduce 16,290 lines of inline changes to <100 lines of minimal delegation

---

## Category A: Pure Additions (Zero Conflict Risk)

### New Files (529 total)

**Templates (4 files):**
- `templates/spec-template.md`
- `templates/PRD-template.md`
- `templates/plan-template.md`
- `templates/tasks-template.md`

**Documentation (150+ files):**
- `docs/spec-kit/*.md` (15 files)
- `docs/SPEC-KIT-*/` directories (6 SPECs)
- Root-level strategy docs (20+ files)

**Scripts (30+ files):**
- `scripts/spec_ops_004/*.sh`
- `scripts/spec-kit/*.py`

**Rust Modules (3 files):**
- `codex-rs/tui/src/spec_prompts.rs` - NEW
- `codex-rs/tui/src/spec_status.rs` - NEW
- `codex-rs/tui/src/bin/spec-status-dump.rs` - NEW

**Test Fixtures (300+ files):**
- `codex-rs/tui/tests/fixtures/spec_status/`

**Evidence (100+ files):**
- `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/`

**Rebase Strategy:** Keep all. Zero conflicts (new files never conflict).

---

## Category B: Inline Modifications (HIGH Conflict Risk)

### File 1: codex-rs/tui/src/chatwidget.rs (🔴 CRITICAL)

**Stats:** 14,112 insertions, 3,262 deletions
**Conflict Probability:** 100% on every rebase
**Why Modified:** Embedded all spec-kit command handling inline

**Modifications:**

**State additions (struct ChatWidget):**
```rust
// FORK-SPECIFIC field
spec_auto_state: Option<SpecAutoState>,
```

**Inline structs (should be in separate module):**
```rust
struct SpecAutoState {
    spec_id: String,
    current_phase: SpecAutoPhase,
    // ... 50+ lines
}

enum SpecAutoPhase {
    Plan, Tasks, Implement, Validate, Audit, Unlock, Done
}
```

**Method additions (10+ methods, ~1,000 lines each):**
- `handle_spec_plan_command()` - ~200 lines
- `handle_spec_tasks_command()` - ~200 lines
- `handle_spec_implement_command()` - ~300 lines
- `handle_spec_validate_command()` - ~200 lines
- `handle_spec_audit_command()` - ~200 lines
- `handle_spec_unlock_command()` - ~200 lines
- `handle_spec_ops_command()` - ~400 lines
- `handle_spec_consensus_command()` - ~150 lines
- `handle_spec_status_command()` - ~100 lines
- `advance_spec_auto_phase()` - ~200 lines
- Helper methods - ~300 lines

**Total inline code:** ~2,500 lines of spec-kit logic embedded in upstream file

**Rebase Impact:**
- Every upstream chatwidget.rs change conflicts
- Message handling changes conflict
- State management changes conflict
- Rendering changes conflict

**Refactoring Strategy:**
- Extract all to `spec_kit/handler.rs` module
- Keep single delegation field in ChatWidget
- All methods become SpecKitHandler methods
- **Target:** 14,112 insertions → ~50 insertions

---

### File 2: codex-rs/tui/src/app.rs (🟠 HIGH)

**Stats:** 1,546 insertions, 1,262 deletions
**Conflict Probability:** 80% on rebase
**Why Modified:** Added routing branches for 30 slash commands

**Modifications:**

**Match statement additions:**
```rust
match command {
    // ... upstream variants ...

    // Our additions: 40+ branches
    SlashCommand::SpecKitPlan | SlashCommand::SpecKitTasks | ... => {
        // 20+ lines per branch group
    }
    SlashCommand::GuardrailPlan | ... => {
        // 15+ lines
    }
    SlashCommand::NewSpec => {
        // Legacy redirect, 10 lines
    }
    // ... 10 more branch groups
}
```

**Rebase Impact:**
- Upstream adds new command handling → conflicts
- Upstream refactors match statement → conflicts
- Upstream changes handler signatures → conflicts

**Refactoring Strategy:**
- Use nested enum: `SlashCommand::SpecKit(SpecKitCommand)`
- Single delegation branch calls handler.route(cmd, args)
- **Target:** 1,546 insertions → ~40 insertions

---

### File 3: codex-rs/tui/src/slash_command.rs (🟠 HIGH)

**Stats:** 632 insertions, 143 deletions
**Conflict Probability:** 70% on rebase
**Why Modified:** Added 30 enum variants to upstream SlashCommand enum

**Modifications:**

**Enum additions:**
```rust
pub enum SlashCommand {
    // Upstream variants
    Browser,
    Chrome,
    // ...

    // FORK-SPECIFIC: 30 variants added
    SpecKitNew,
    SpecKitSpecify,
    // ... 11 more SpecKit*
    GuardrailPlan,
    // ... 6 more Guardrail*
    NewSpec,  // legacy
    // ... 12 more legacy
}
```

**Method additions:**
- `is_spec_ops()` - Checks if command is guardrail
- `spec_ops()` - Returns metadata for guardrail commands
- Description text for 30 variants

**Rebase Impact:**
- Upstream adds enum variants → ordering conflicts
- Upstream renames variants → our code breaks
- Upstream changes enum derive macros → conflicts

**Refactoring Strategy:**
- Nested enums: `SpecKit(SpecKitCommand)`, `Guardrail(GuardrailCommand)`
- Separate module: `spec_kit/commands.rs`
- **Target:** 632 insertions → ~30 insertions in upstream enum

---

## Category C: Minor Modifications (Low Conflict Risk)

### 4. codex-rs/apply-patch/src/lib.rs
**Stats:** ~50 insertions
**Conflict Risk:** 🟢 LOW
**Change:** Added FileSystem trait back-compat shim (upstream refactored, we need old API)

**Rebase Strategy:** If upstream changes apply-patch significantly, regenerate shim

---

### 5. Cargo Fmt Noise (~250 files)
**Stats:** Unknown insertions (mostly whitespace)
**Conflict Risk:** 🟢 LOW - Auto-merge or re-fmt after rebase

**Examples:**
- All `bottom_pane/*.rs` files (50+ files)
- Most `codex-rs/core/src/*.rs` files
- Build scripts, lib.rs files across crates

**Validation:** Run `git diff master --stat` after filtering whitespace:
```bash
git diff master -w --stat | wc -l  # Should be much smaller
```

**Rebase Strategy:** Accept upstream formatting, re-run `cargo fmt --all` post-rebase

---

## Refactoring Plan (3-Phase Extraction)

### Phase 1: Extract Handler Module (Highest Impact)

**Goal:** Move chatwidget.rs inline logic to separate module

**Create:** `codex-rs/tui/src/spec_kit/mod.rs`
```rust
mod handler;
mod state;
mod commands;

pub use handler::SpecKitHandler;
pub use state::{SpecAutoState, SpecAutoPhase};
pub use commands::{SpecKitCommand, GuardrailCommand};
```

**Create:** `codex-rs/tui/src/spec_kit/handler.rs`
```rust
pub struct SpecKitHandler {
    state: Option<SpecAutoState>,
    // ... telemetry tracking
}

impl SpecKitHandler {
    pub fn handle_plan(&mut self, args: &str, ctx: &mut ChatContext) {
        // Move handle_spec_plan_command logic here
    }

    pub fn handle_tasks(&mut self, args: &str, ctx: &mut ChatContext) {
        // Move handle_spec_tasks_command logic here
    }

    // ... 10 more methods
}
```

**Modify:** `codex-rs/tui/src/chatwidget.rs`
```rust
// Before: 14,112 insertions
spec_auto_state: Option<SpecAutoState>,  // inline struct def
// ... 10 inline methods, 2,500 lines

// After: 50 insertions
use spec_kit::SpecKitHandler;
spec_kit: Option<SpecKitHandler>,

fn handle_spec_plan_command(&mut self, args: &str) {
    if let Some(handler) = &mut self.spec_kit {
        handler.handle_plan(args, self.make_context());
    }
}
// ... minimal delegation only
```

**Effort:** 4-6 hours
**Impact:** 14,112 → 50 lines (99.6% reduction)

---

### Phase 2: Isolate Enum Variants (High Impact)

**Goal:** Nested enums to group spec-kit commands

**Create:** `codex-rs/tui/src/spec_kit/commands.rs`
```rust
#[derive(Debug, Clone, EnumString, EnumIter)]
#[strum(serialize_all = "kebab-case")]
pub enum SpecKitCommand {
    New, Specify, Clarify, Analyze, Checklist,
    Plan, Tasks, Implement, Validate, Audit, Unlock,
    Auto, Status
}

#[derive(Debug, Clone, EnumString, EnumIter)]
#[strum(serialize_all = "kebab-case")]
pub enum GuardrailCommand {
    Plan, Tasks, Implement, Validate, Audit, Unlock, Auto
}
```

**Modify:** `codex-rs/tui/src/slash_command.rs`
```rust
// Before: 30 variants mixed with upstream
pub enum SlashCommand {
    Browser,  // upstream
    SpecKitNew,  // ours
    Chrome,  // upstream
    SpecKitPlan,  // ours
    // ... chaos
}

// After: 2 variants, clean separation
pub enum SlashCommand {
    // ... all upstream variants unchanged ...
    SpecKit(SpecKitCommand),     // Single variant
    Guardrail(GuardrailCommand), // Single variant
    // ... legacy variants for backward compat
}
```

**Routing update:**
```rust
// Pattern matching becomes nested
match command {
    SlashCommand::SpecKit(SpecKitCommand::Plan) => { /* ... */ }
    SlashCommand::SpecKit(cmd) => handler.route(cmd),  // Simplified
}
```

**Effort:** 3-4 hours
**Impact:** 632 → 30 lines (95% reduction)

---

### Phase 3: Routing Extraction (Medium Impact)

**Goal:** Move routing logic to handler, keep minimal delegation in app.rs

**Modify:** `codex-rs/tui/src/app.rs`
```rust
// Before: 40+ branches, 1,546 insertions
match command {
    SlashCommand::SpecKitPlan => {
        if let AppState::Chat { widget } = &mut self.app_state {
            widget.handle_spec_plan_command(args);
        }
    }
    // ... 39 more similar branches
}

// After: 2 branches, ~40 insertions
match command {
    SlashCommand::SpecKit(cmd) => {
        if let AppState::Chat { widget } = &mut self.app_state {
            widget.spec_kit_handler().route(cmd, args);
        }
    }
    SlashCommand::Guardrail(cmd) => {
        if let AppState::Chat { widget } = &mut self.app_state {
            widget.handle_guardrail_command(cmd, args);
        }
    }
    // ... all upstream branches unchanged
}
```

**Effort:** 2-3 hours
**Impact:** 1,546 → 40 lines (97% reduction)

---

## Total Refactoring Impact

**Before Refactoring:**
- Critical modifications: 16,290 lines in 3 files
- Rebase conflicts: Guaranteed on every file
- Merge effort: 4-8 hours per rebase

**After Refactoring:**
- Critical modifications: ~120 lines in 3 files
- Rebase conflicts: Rare, easy to resolve
- Merge effort: 30 minutes per rebase

**Net reduction:** 99.3% (16,290 → 120 lines)

---

## Post-Refactoring Rebase Protocol

### Step 1: Upstream Sync
```bash
git fetch upstream master
git log master..upstream/master --oneline  # Review changes
```

### Step 2: Automated Pre-Check
```bash
scripts/fork_maintenance/validate_pre_rebase.sh
# Checks: git tree clean, all tests pass, no uncommitted changes
```

### Step 3: Rebase
```bash
git checkout -b rebase-$(date +%Y%m%d)
git rebase upstream/master
```

### Step 4: Conflict Resolution (Minimal Touch Points)

**If chatwidget.rs conflicts (rare):**
- Keep our single field: `spec_kit: Option<SpecKitHandler>`
- Keep our delegation methods (10 lines each)
- Accept upstream for everything else

**If app.rs conflicts (rare):**
- Keep our 2 delegation branches
- Accept upstream for all other routing

**If slash_command.rs conflicts (rare):**
- Keep our 2 nested enum variants
- Accept upstream for all other variants
- Re-run `cargo fmt`

### Step 5: Automated Post-Check
```bash
scripts/fork_maintenance/validate_rebase.sh
# Checks: compilation, spec-kit enums present, routing intact, tests pass
```

### Step 6: Manual Validation
```bash
# Start TUI
codex-rs/target/dev-fast/code

# Test commands
/speckit.status SPEC-KIT-045-mini
/speckit.auto SPEC-KIT-TEST  # Small test SPEC
```

### Step 7: Document
```bash
echo "$(date): Rebased to $(git rev-parse upstream/master)" >> docs/spec-kit/REBASE_LOG.md
# Note conflicts, resolutions, issues
```

---

## Rebase Conflict Patterns & Resolutions

### Pattern 1: chatwidget.rs Field Additions

**Upstream adds field:**
```diff
pub struct ChatWidget {
+   new_upstream_field: SomeType,
    // ... existing fields
}
```

**We have:**
```diff
pub struct ChatWidget {
    // ... upstream fields
+   spec_kit: Option<SpecKitHandler>,  // Our field
}
```

**Resolution:**
1. Accept both fields
2. Place our field last (convention)
3. Run `cargo build` to verify

---

### Pattern 2: app.rs Match Statement

**Upstream adds command:**
```diff
match command {
+   SlashCommand::NewUpstreamCommand => { /* ... */ }
    // ... existing branches
}
```

**We have:**
```diff
match command {
    // ... upstream branches
+   SlashCommand::SpecKit(cmd) => { /* ... */ }
+   SlashCommand::Guardrail(cmd) => { /* ... */ }
}
```

**Resolution:**
1. Keep both sets of branches
2. Our branches go after upstream branches (convention)
3. No functional conflict

---

### Pattern 3: slash_command.rs Enum Variants

**Upstream adds variant:**
```diff
pub enum SlashCommand {
+   NewUpstreamVariant,
    // ... existing variants
}
```

**We have:**
```diff
pub enum SlashCommand {
    // ... upstream variants
+   SpecKit(SpecKitCommand),
+   Guardrail(GuardrailCommand),
}
```

**Resolution:**
1. Place new upstream variant in correct upstream position
2. Keep our variants last (convention)
3. Re-run `cargo fmt`

---

### Pattern 4: Cargo.toml Dependencies

**Upstream updates dependency:**
```diff
[dependencies]
-some-crate = "1.0"
+some-crate = "2.0"
```

**We haven't added dependencies (clean)**

**Resolution:**
1. Accept upstream version
2. Run `cargo build` to verify compatibility
3. Fix any compilation errors from API changes

---

## Files Requiring Investigation

**Needs manual review to determine if changes are substantive or fmt noise:**

1. `codex-rs/core/src/config_types.rs` - Check for subagent config additions
2. `codex-rs/core/src/slash_commands.rs` - Check for spec-kit formatters
3. `codex-rs/core/src/codex.rs` - Check for spec-kit integration
4. `codex-rs/cli/src/main.rs` - Check for CLI command additions

**Action:** Run diff analysis on each file, categorize as:
- **A (Addition):** New code we added, move to module
- **B (Modification):** Changed upstream logic, document why
- **C (Formatting):** Cargo fmt only, ignore

---

## Maintenance Artifacts to Build

### 1. scripts/fork_maintenance/validate_pre_rebase.sh
**Purpose:** Check prerequisites before attempting rebase
**Checks:**
- Git tree clean
- All tests pass
- No uncommitted changes
- Current branch is rebase working branch

### 2. scripts/fork_maintenance/validate_rebase.sh
**Purpose:** Verify fork-specific code survived rebase
**Checks:**
- Compilation successful
- SpecKit* enum variants present
- Guardrail* enum variants present
- spec_kit module exists
- Routing delegation intact
- Template files exist
- Script files exist
- One /speckit.auto test run

### 3. docs/spec-kit/REBASE_LOG.md
**Purpose:** Institutional knowledge of each rebase attempt
**Contents:**
- Date, upstream commit, conflicts encountered
- Resolution strategies used
- Time spent
- Issues discovered
- Lessons learned

### 4. docs/spec-kit/CONFLICT_RESOLUTION_PLAYBOOK.md
**Purpose:** Standard patterns for common conflicts
**Contents:**
- Pattern library (field additions, match branches, enum variants)
- Resolution strategies per pattern
- Code examples
- When to escalate vs auto-resolve

---

## Refactoring Success Criteria

**Before marking refactoring complete:**
- [ ] chatwidget.rs: <100 insertions total
- [ ] app.rs: <50 insertions total
- [ ] slash_command.rs: <50 insertions total
- [ ] All spec-kit logic in `spec_kit/` module
- [ ] Compilation successful
- [ ] All 20 commands functional (tested in TUI)
- [ ] One full /speckit.auto pipeline succeeds
- [ ] Zero behavior changes (identical output)
- [ ] validate_rebase.sh passes

**Timeline:**
- Phase 1 (handler extraction): 4-6 hours
- Phase 2 (enum isolation): 3-4 hours
- Phase 3 (routing extraction): 2-3 hours
- Validation: 1-2 hours
- **Total:** 10-15 hours

**ROI:** First rebase after refactoring takes 30 min vs 4-8 hours

---

## Quarterly Rebase Schedule

**Q1 2026 (January):**
- Sync with upstream (likely 3-4 months of changes)
- Use refactored codebase (minimal conflicts expected)
- Document rebase in REBASE_LOG.md
- Estimated effort: 1-2 hours

**Q2 2026 (April):**
- Sync with upstream
- Validate conflict patterns match predictions
- Update playbook if new patterns emerge
- Estimated effort: 1-2 hours

**Ongoing:**
- Monitor upstream releases
- Cherry-pick critical security fixes as needed
- Full rebase quarterly
- Emergency rebase if critical bug fix needed

---

## Action Items (Immediate)

**This Session:**
1. ✅ Complete this audit document
2. [ ] Create REFACTORING_PLAN.md (detailed extraction steps)
3. [ ] Build validate_rebase.sh (automated checker)
4. [ ] Update FORK_DEVIATIONS.md (conflict patterns)

**Next Session (10-15 hours):**
5. [ ] Execute Phase 1: Extract SpecKitHandler
6. [ ] Execute Phase 2: Isolate enums
7. [ ] Execute Phase 3: Extract routing
8. [ ] Validate: Compilation + functional testing
9. [ ] Commit refactored code

**After Refactoring:**
10. [ ] Attempt test rebase (upstream/master → our branch)
11. [ ] Measure conflict resolution time
12. [ ] Document actual vs predicted conflicts
13. [ ] Update REBASE_LOG.md with first rebase

---

**Document Version:** 1.0
**Status:** Initial audit complete
**Next:** Detailed refactoring plan
**Owner:** @just-every/automation
