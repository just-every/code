# Fork Refactoring Plan - Module Extraction for Clean Rebases

**Date:** 2025-10-15
**Goal:** Extract 16,290 lines of inline spec-kit code to isolated modules
**Target:** <120 lines of upstream file modifications
**Effort:** 10-15 hours
**Payoff:** 99% reduction in rebase conflict surface

---

## Problem Statement

**Current state:**
- chatwidget.rs: 14,112 inline insertions
- app.rs: 1,546 inline insertions
- slash_command.rs: 632 inline insertions
- **Total:** 16,290 lines embedded in upstream files

**Consequence:** Every upstream change to these files creates conflicts

**Solution:** Module extraction with minimal upstream touch points

---

## Architecture: Before → After

### Current (Inline Integration)

```
codex-rs/tui/src/
├── chatwidget.rs (upstream + 14K our lines)
│   ├── struct ChatWidget {
│   │   spec_auto_state: Option<SpecAutoState>  // Ours
│   │   }
│   ├── struct SpecAutoState { /* 50 lines */ }  // Ours inline
│   ├── fn handle_spec_plan_command() { /* 200 lines */ }  // Ours inline
│   └── ... 10 more methods inline
├── app.rs (upstream + 1.5K our lines)
│   └── match command {
│         SpecKitPlan => { /* 20 lines */ }  // Ours inline
│         ... 39 more branches
│       }
└── slash_command.rs (upstream + 632 our lines)
    └── enum SlashCommand {
          SpecKitNew,  // Ours mixed
          SpecKitPlan,  // Ours mixed
          ... 28 more variants
        }
```

### Target (Module Isolation)

```
codex-rs/tui/src/
├── chatwidget.rs (upstream + 50 our lines)
│   ├── use spec_kit::SpecKitHandler;
│   ├── struct ChatWidget {
│   │   spec_kit: Option<SpecKitHandler>  // Single field
│   │   }
│   └── fn handle_spec_plan_command(&mut self, args: &str) {
│         self.spec_kit.as_mut()?.handle_plan(args, &mut self.ctx())
│       }  // 5 lines × 10 commands = 50 lines total
├── app.rs (upstream + 40 our lines)
│   └── match command {
│         SlashCommand::SpecKit(cmd) => handler.route(cmd, args),  // 5 lines
│         SlashCommand::Guardrail(cmd) => handler.route_guardrail(cmd, args),  // 5 lines
│       }  // 2 branches × 20 lines = 40 lines total
├── slash_command.rs (upstream + 30 our lines)
│   └── enum SlashCommand {
│         SpecKit(SpecKitCommand),  // 1 variant + 15 lines impl
│         Guardrail(GuardrailCommand),  // 1 variant + 15 lines impl
│       }
└── spec_kit/  // NEW MODULE (all our code)
    ├── mod.rs (exports)
    ├── handler.rs (14K lines from chatwidget)
    ├── commands.rs (enum definitions)
    ├── state.rs (SpecAutoState, etc)
    └── router.rs (routing logic from app.rs)
```

---

## Phase 1: Extract Handler (chatwidget.rs)

### Step 1.1: Create Module Structure

**Create files:**
```bash
mkdir -p codex-rs/tui/src/spec_kit
touch codex-rs/tui/src/spec_kit/mod.rs
touch codex-rs/tui/src/spec_kit/handler.rs
touch codex-rs/tui/src/spec_kit/state.rs
```

**codex-rs/tui/src/spec_kit/mod.rs:**
```rust
mod handler;
mod state;

pub use handler::SpecKitHandler;
pub use state::{SpecAutoState, SpecAutoPhase};
```

**Add to codex-rs/tui/src/lib.rs:**
```rust
mod spec_kit;
```

---

### Step 1.2: Extract State Structs

**Move from chatwidget.rs to spec_kit/state.rs:**

```rust
// Extract these inline structs
pub struct SpecAutoState {
    pub spec_id: String,
    pub current_phase: SpecAutoPhase,
    pub started_at: Instant,
    pub phases_completed: Vec<SpecStage>,
}

pub enum SpecAutoPhase {
    Plan,
    Tasks,
    Implement,
    Validate,
    Audit,
    Unlock,
    Done,
}

// ... any other state structs
```

**Update chatwidget.rs:**
```rust
// Before
struct SpecAutoState { /* inline 50 lines */ }

// After
use spec_kit::{SpecAutoState, SpecAutoPhase};
```

**Test:** `cargo build -p codex-tui` should still work

---

### Step 1.3: Extract Handler Methods

**Create spec_kit/handler.rs with ChatContext helper:**

```rust
use crate::chatwidget::ChatWidget;

// Context struct to pass chatwidget state to handler
pub struct ChatContext<'a> {
    // Minimal fields needed by handlers
    pub config: &'a Config,
    pub history: &'a mut ConversationHistory,
    pub working_dir: &'a Path,
    // ... other needed fields
}

pub struct SpecKitHandler {
    state: Option<SpecAutoState>,
}

impl SpecKitHandler {
    pub fn new() -> Self {
        Self { state: None }
    }

    pub fn handle_plan(&mut self, args: &str, ctx: &mut ChatContext) {
        // Move handle_spec_plan_command body here
        // Change self.config → ctx.config
        // Change self.history → ctx.history
        // etc.
    }

    pub fn handle_tasks(&mut self, args: &str, ctx: &mut ChatContext) {
        // Move handle_spec_tasks_command body here
    }

    // ... 10 more methods
}
```

**Update chatwidget.rs:**
```rust
// Before (inline)
spec_auto_state: Option<SpecAutoState>,

fn handle_spec_plan_command(&mut self, args: &str) {
    // 200 lines of logic inline
}

// After (delegation)
use spec_kit::SpecKitHandler;
spec_kit: SpecKitHandler,

fn handle_spec_plan_command(&mut self, args: &str) {
    let mut ctx = ChatContext {
        config: &self.config,
        history: &mut self.history,
        working_dir: &self.working_dir,
        // ...
    };
    self.spec_kit.handle_plan(args, &mut ctx);
}
```

**Complexity:** ChatContext might need 10-15 fields. Alternative: Pass &mut ChatWidget to handler (less clean but simpler).

---

### Step 1.4: Test Compilation

```bash
cd codex-rs
cargo build -p codex-tui --profile dev-fast
```

**Expected errors:** Field access issues (ctx.field vs self.field)
**Resolution:** Adjust ChatContext fields as needed

**Iteration:** May need 2-3 build cycles to get all fields right

---

## Phase 2: Isolate Enums (slash_command.rs)

### Step 2.1: Create Command Enums

**Create spec_kit/commands.rs:**

```rust
use strum_macros::{EnumString, EnumIter, AsRefStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumString, EnumIter, AsRefStr)]
#[strum(serialize_all = "kebab-case")]
pub enum SpecKitCommand {
    New,
    Specify,
    Clarify,
    Analyze,
    Checklist,
    Plan,
    Tasks,
    Implement,
    Validate,
    Audit,
    Unlock,
    Auto,
    Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumString, EnumIter, AsRefStr)]
#[strum(serialize_all = "kebab-case")]
pub enum GuardrailCommand {
    Plan,
    Tasks,
    Implement,
    Validate,
    Audit,
    Unlock,
    Auto,
}

impl SpecKitCommand {
    pub fn description(&self) -> &'static str {
        match self {
            SpecKitCommand::New => "create new SPEC with templates",
            SpecKitCommand::Plan => "work breakdown with consensus",
            // ... all descriptions
        }
    }
}

impl GuardrailCommand {
    pub fn description(&self) -> &'static str {
        match self {
            GuardrailCommand::Plan => "plan validation",
            // ... all descriptions
        }
    }
}
```

**Add to spec_kit/mod.rs:**
```rust
mod commands;
pub use commands::{SpecKitCommand, GuardrailCommand};
```

---

### Step 2.2: Modify SlashCommand Enum

**slash_command.rs changes:**

```rust
// Before: 30 individual variants
pub enum SlashCommand {
    Browser,
    SpecKitNew,
    SpecKitPlan,
    // ... 28 more spec-kit variants mixed
}

// After: 2 nested variants
pub enum SlashCommand {
    // ... all upstream variants unchanged ...

    // FORK-SPECIFIC: Spec-kit commands
    #[strum(serialize = "speckit")]
    SpecKit(SpecKitCommand),

    #[strum(serialize = "guardrail")]
    Guardrail(GuardrailCommand),

    // Legacy (backward compat)
    #[strum(serialize = "new-spec")]
    NewSpec,  // Maps to SpecKit(New) in routing
    #[strum(serialize = "spec-plan")]
    SpecPlan,  // Maps to SpecKit(Plan)
    // ... keep legacy variants for backward compat
}
```

**Parsing logic:**
```rust
// Handle "speckit.plan" → SpecKit(Plan)
impl FromStr for SlashCommand {
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(subcommand) = s.strip_prefix("speckit.") {
            return Ok(SlashCommand::SpecKit(
                SpecKitCommand::from_str(subcommand)?
            ));
        }
        if let Some(subcommand) = s.strip_prefix("guardrail.") {
            return Ok(SlashCommand::Guardrail(
                GuardrailCommand::from_str(subcommand)?
            ));
        }
        // ... upstream parsing
    }
}
```

---

### Step 2.3: Update Match Statements

**Every match on SlashCommand needs update:**

```rust
// Before
match command {
    SlashCommand::SpecKitPlan => { /* ... */ }
    SlashCommand::SpecKitTasks => { /* ... */ }
}

// After
match command {
    SlashCommand::SpecKit(SpecKitCommand::Plan) => { /* ... */ }
    SlashCommand::SpecKit(SpecKitCommand::Tasks) => { /* ... */ }
    // Or use catchall:
    SlashCommand::SpecKit(cmd) => handler.route(cmd),
}
```

**Files to update:**
- app.rs (main routing)
- slash_command.rs (helper methods)
- Anywhere we pattern match on SlashCommand

---

### Step 2.4: Test Compilation

```bash
cargo build -p codex-tui --profile dev-fast
```

**Expected errors:** Pattern matching exhaustiveness
**Resolution:** Update all match statements to handle nested enum

---

## Phase 3: Extract Routing (app.rs)

### Step 3.1: Create Router Module

**Create spec_kit/router.rs:**

```rust
use super::SpecKitCommand;
use crate::chatwidget::ChatWidget;

pub fn route_speckit_command(
    command: SpecKitCommand,
    args: &str,
    widget: &mut ChatWidget,
) {
    match command {
        SpecKitCommand::Plan => widget.handle_spec_plan_command(args),
        SpecKitCommand::Tasks => widget.handle_spec_tasks_command(args),
        SpecKitCommand::Implement => widget.handle_spec_implement_command(args),
        // ... all 13 commands
    }
}

pub fn route_guardrail_command(
    command: GuardrailCommand,
    args: &str,
    widget: &mut ChatWidget,
) {
    match command {
        GuardrailCommand::Plan => widget.handle_guardrail_plan(args),
        // ... all 7 commands
    }
}
```

---

### Step 3.2: Simplify app.rs Routing

**app.rs changes:**

```rust
// Before: 40+ branches, 1,546 insertions
match command {
    SlashCommand::SpecKitPlan => {
        if let AppState::Chat { widget } = &mut self.app_state {
            widget.handle_spec_plan_command(args);
        }
    }
    SlashCommand::SpecKitTasks => { /* similar */ }
    // ... 38 more branches
}

// After: 2 branches, 40 insertions
use spec_kit::router::{route_speckit_command, route_guardrail_command};

match command {
    SlashCommand::SpecKit(cmd) => {
        if let AppState::Chat { widget } = &mut self.app_state {
            route_speckit_command(cmd, command_args, widget);
        }
    }
    SlashCommand::Guardrail(cmd) => {
        if let AppState::Chat { widget } = &mut self.app_state {
            route_guardrail_command(cmd, command_args, widget);
        }
    }
    // Legacy routing
    SlashCommand::NewSpec => {
        if let AppState::Chat { widget } = &mut self.app_state {
            route_speckit_command(SpecKitCommand::New, command_args, widget);
        }
    }
    // ... all upstream branches unchanged
}
```

---

## Validation Protocol

### After Each Phase

**Compilation check:**
```bash
cd codex-rs
cargo build -p codex-tui --profile dev-fast
```

**Functional check (in TUI):**
```bash
# Test Tier 0
/speckit.status SPEC-KIT-045-mini

# Test Tier 2
/speckit.clarify SPEC-KIT-065

# Test guardrail
/guardrail.plan SPEC-KIT-065 --dry-run

# Test legacy
/spec-status SPEC-KIT-045-mini
```

**Behavior validation:**
- Output identical to pre-refactoring
- Evidence files created in same paths
- Local-memory updates identical
- No performance regression

---

## Detailed Implementation Steps

### Phase 1: Handler Extraction (4-6 hours)

**Hour 1-2: Setup and state extraction**
1. Create module structure (`spec_kit/mod.rs`, `handler.rs`, `state.rs`)
2. Move SpecAutoState, SpecAutoPhase to state.rs
3. Update imports in chatwidget.rs
4. Compile and fix errors

**Hour 3-4: Extract first 3 methods**
5. Move `handle_spec_plan_command` to handler.rs
6. Move `handle_spec_tasks_command` to handler.rs
7. Move `handle_spec_implement_command` to handler.rs
8. Create ChatContext struct with needed fields
9. Update chatwidget.rs delegation
10. Compile and fix field access errors

**Hour 5-6: Extract remaining methods**
11. Move remaining 7 handle_spec_* methods
12. Move helper methods (advance_spec_auto_phase, etc.)
13. Final compilation
14. Test in TUI: run one /speckit.auto pipeline

**Checkpoint:** chatwidget.rs should have ~50-100 insertions, handler.rs should have ~14K lines

---

### Phase 2: Enum Isolation (3-4 hours)

**Hour 1-2: Create command enums**
1. Create spec_kit/commands.rs
2. Define SpecKitCommand enum (13 variants)
3. Define GuardrailCommand enum (7 variants)
4. Implement description() methods
5. Add to spec_kit/mod.rs exports
6. Compile check

**Hour 3-4: Refactor SlashCommand**
7. Change SpecKit*/Guardrail* to nested SpecKit(cmd)/Guardrail(cmd)
8. Update FromStr parsing for "speckit.plan" → SpecKit(Plan)
9. Update all match statements (app.rs, slash_command.rs helpers)
10. Keep legacy variants (NewSpec, SpecPlan, etc.) for backward compat
11. Compile and fix exhaustive pattern matching errors
12. Test in TUI: /speckit.plan, /guardrail.plan, /spec-plan (legacy)

**Checkpoint:** slash_command.rs should have ~30 insertions for nested variants

---

### Phase 3: Routing Extraction (2-3 hours)

**Hour 1-2: Create router**
1. Create spec_kit/router.rs
2. Move routing logic from app.rs
3. route_speckit_command(cmd, args, widget)
4. route_guardrail_command(cmd, args, widget)
5. Compile check

**Hour 3: Simplify app.rs**
6. Replace 40+ branches with 2 delegation branches
7. Update legacy command routing (NewSpec → route to SpecKit(New))
8. Compile and fix
9. Test in TUI: All commands

**Checkpoint:** app.rs should have ~40 insertions total

---

### Phase 4: Validation (1-2 hours)

**Hour 1: Comprehensive testing**
1. Build release profile: `cargo build -p codex-tui --release`
2. Test all 13 /speckit.* commands
3. Test all 7 /guardrail.* commands
4. Test backward compat (legacy commands)
5. Run full /speckit.auto pipeline on small SPEC
6. Verify evidence files created correctly
7. Check local-memory updates
8. Validate performance (no regression)

**Hour 2: Documentation**
9. Update FORK_DEVIATIONS.md with new architecture
10. Document new module structure
11. Create commit
12. Update RESTART.md

---

## Expected Challenges

### Challenge 1: ChatWidget Field Access

**Problem:** Handler needs access to ChatWidget fields (history, config, etc.)

**Solution Options:**

**Option A: Context struct (cleaner but more work)**
```rust
pub struct ChatContext<'a> {
    pub config: &'a Config,
    pub history: &'a mut History,
    // ... 10-15 fields
}

impl SpecKitHandler {
    pub fn handle_plan(&mut self, ctx: &mut ChatContext) {
        ctx.history.add_message(/* ... */);
    }
}
```

**Option B: Pass &mut ChatWidget (simpler but less isolated)**
```rust
impl SpecKitHandler {
    pub fn handle_plan(&mut self, widget: &mut ChatWidget) {
        widget.history.add_message(/* ... */);
    }
}
```

**Recommendation:** Option B for speed, refactor to Option A later if needed

---

### Challenge 2: Enum Parsing Complexity

**Problem:** "speckit.plan" needs to parse to SpecKit(Plan)

**Solution:** Custom FromStr or strum attributes

```rust
// Manual parsing in FromStr impl
fn from_str(s: &str) -> Result<Self, Self::Err> {
    if let Some(sub) = s.strip_prefix("speckit.") {
        return Ok(SlashCommand::SpecKit(SpecKitCommand::from_str(sub)?));
    }
    // ... strum default for other variants
}
```

**Test extensively:** `/speckit.plan`, `/guardrail.auto`, `/spec-plan` (legacy)

---

### Challenge 3: Pattern Match Exhaustiveness

**Problem:** Rust requires exhaustive matching, nested enums add complexity

**Solution:** Use catchall where appropriate

```rust
// Before
match command {
    SpecKitPlan => { /* specific */ }
    SpecKitTasks => { /* specific */ }
    // ... 38 more
}

// After
match command {
    SlashCommand::SpecKit(cmd) => {
        // Delegate all to router
        route_speckit_command(cmd, args, widget)
    }
    // Cleaner, single branch
}
```

---

## Success Criteria (All Must Pass)

**Code Quality:**
- [ ] chatwidget.rs: <100 lines of spec-kit code
- [ ] app.rs: <50 lines of spec-kit code
- [ ] slash_command.rs: <50 lines of spec-kit code
- [ ] All spec-kit logic in spec_kit/ module
- [ ] Zero upstream file modifications >100 lines

**Functional:**
- [ ] All 13 /speckit.* commands work
- [ ] All 7 /guardrail.* commands work
- [ ] Backward compat: /spec-* and /spec-ops-* work
- [ ] One full /speckit.auto pipeline succeeds
- [ ] Evidence files created correctly
- [ ] Local-memory updates work

**Performance:**
- [ ] No speed regression (measure one pipeline)
- [ ] Compilation time similar (<5% increase)
- [ ] Binary size similar (<10% increase)

**Rebase Readiness:**
- [ ] validate_rebase.sh passes
- [ ] Manual rebase test against current upstream
- [ ] Conflict resolution <30 minutes
- [ ] Post-rebase functional test passes

---

## Post-Refactoring Rebase Test

**After refactoring complete:**

**Test rebase (dry run):**
```bash
git fetch upstream master
git checkout -b rebase-test-$(date +%Y%m%d)
git rebase upstream/master

# Measure:
# - Number of conflicts
# - Files with conflicts
# - Lines in conflict
# - Time to resolve

# Document in REBASE_LOG.md
```

**Expected results:**
- Conflicts: <5 files (vs 100+ without refactoring)
- Conflict lines: <200 (vs 16,000+ without refactoring)
- Resolution time: <30 min (vs 4-8 hours without refactoring)

**If test succeeds:**
- Merge rebase-test to feat/spec-auto-telemetry
- Document patterns in CONFLICT_RESOLUTION_PLAYBOOK.md
- Schedule quarterly rebases

**If test fails:**
- Analyze unexpected conflicts
- Refine extraction strategy
- Iterate

---

## Commit Strategy

**Refactoring commits (4 total):**

1. **Commit 1: Module structure**
   ```
   refactor(spec-kit): create isolated module structure

   - Add spec_kit/mod.rs, handler.rs, state.rs
   - Move SpecAutoState structs
   - Update imports
   - Compilation check
   ```

2. **Commit 2: Extract handlers**
   ```
   refactor(spec-kit): extract handlers from chatwidget to module

   - Move all handle_spec_* methods to SpecKitHandler
   - chatwidget.rs: 14,112 → 100 insertions
   - Add delegation methods
   - Functional validation passed
   ```

3. **Commit 3: Isolate enums**
   ```
   refactor(spec-kit): isolate command enums from upstream SlashCommand

   - Create SpecKitCommand, GuardrailCommand enums
   - SlashCommand uses nested variants
   - slash_command.rs: 632 → 30 insertions
   - Backward compat maintained
   ```

4. **Commit 4: Extract routing**
   ```
   refactor(spec-kit): extract routing to module

   - Create spec_kit/router.rs
   - app.rs: 1,546 → 40 insertions
   - Minimal upstream touch
   - All commands functional
   ```

**Each commit:** Compiles, passes tests, functional validation

**Squashing:** NO. Keep granular for easier revert if needed.

---

## Timeline

**Optimal execution:**
- **Session 1 (4 hours):** Phase 1 (handler extraction)
- **Session 2 (3 hours):** Phase 2 (enum isolation)
- **Session 3 (3 hours):** Phase 3 (routing) + validation
- **Total:** 10 hours across 3 focused sessions

**Single session:**
- Risky: fatigue leads to errors
- Not recommended for 10-hour effort

**Recommendation:** Break across 2-3 sessions, commit after each phase

---

## Next Actions

**This session (complete these artifacts):**
1. ✅ FORK_ISOLATION_AUDIT.md (this document)
2. [ ] REFACTORING_PLAN.md (this document)
3. [ ] validate_rebase.sh (automated checker)
4. [ ] Update FORK_DEVIATIONS.md

**Next session:**
5. Execute Phase 1 (handler extraction)

---

**Document Version:** 1.0
**Status:** Planning complete, ready for execution
**Owner:** @just-every/automation
