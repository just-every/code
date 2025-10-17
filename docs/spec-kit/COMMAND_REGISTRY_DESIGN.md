# Command Registry Architecture (T74)

**Status:** Design Phase
**Created:** 2025-10-16
**Goal:** Eliminate `slash_command.rs` conflicts by moving spec-kit commands to a dynamic registry

---

## Problem Statement

**Current Architecture:**
- 30+ spec-kit enum variants mixed with upstream commands in `SlashCommand` enum
- Every new command requires enum modification → 70-100% conflict probability
- Pattern matching in `app.rs` creates additional conflict surface
- Tight coupling between enum and handler methods

**Conflict Examples:**
```rust
// slash_command.rs - 30 spec-kit variants
pub enum SlashCommand {
    Browser,    // Upstream
    SpecKitNew, // Fork-specific → conflicts
    SpecKitPlan, // Fork-specific → conflicts
    // ... 28 more fork variants
}

// app.rs - Pattern matching conflicts
match command {
    SlashCommand::Browser => { /* upstream */ }
    SlashCommand::SpecKitNew => { /* fork-specific */ }
    // Conflicts when upstream adds new variants
}
```

---

## Proposed Architecture

### 1. Command Trait

```rust
// tui/src/chatwidget/spec_kit/command_registry.rs

pub trait SpecKitCommand: Send + Sync {
    /// Primary command name (e.g., "speckit.new")
    fn name(&self) -> &'static str;

    /// Aliases for backward compatibility (e.g., ["new-spec"])
    fn aliases(&self) -> &[&'static str] {
        &[]
    }

    /// User-visible description
    fn description(&self) -> &'static str;

    /// Execute command
    fn execute(&self, widget: &mut ChatWidget, args: String);

    /// Whether command requires arguments
    fn requires_args(&self) -> bool {
        false
    }

    /// Whether command expands prompts for multi-agent
    fn is_prompt_expanding(&self) -> bool {
        false
    }
}
```

### 2. Registry

```rust
pub struct CommandRegistry {
    commands: HashMap<String, Box<dyn SpecKitCommand>>,
    by_alias: HashMap<String, String>, // alias -> primary name
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
            by_alias: HashMap::new(),
        }
    }

    pub fn register(&mut self, command: Box<dyn SpecKitCommand>) {
        let name = command.name().to_string();
        for alias in command.aliases() {
            self.by_alias.insert(alias.to_string(), name.clone());
        }
        self.commands.insert(name, command);
    }

    pub fn find(&self, name: &str) -> Option<&dyn SpecKitCommand> {
        // Check primary name first
        if let Some(cmd) = self.commands.get(name) {
            return Some(cmd.as_ref());
        }
        // Check aliases
        if let Some(primary) = self.by_alias.get(name) {
            return self.commands.get(primary).map(|cmd| cmd.as_ref());
        }
        None
    }

    pub fn iter(&self) -> impl Iterator<Item = &dyn SpecKitCommand> {
        self.commands.values().map(|cmd| cmd.as_ref())
    }
}
```

### 3. Global Registry

```rust
use once_cell::sync::Lazy;
use std::sync::Mutex;

static SPEC_KIT_REGISTRY: Lazy<Mutex<CommandRegistry>> = Lazy::new(|| {
    let mut registry = CommandRegistry::new();

    // Register all spec-kit commands
    registry.register(Box::new(SpecKitNewCommand));
    registry.register(Box::new(SpecKitPlanCommand));
    // ... register all 30+ commands

    Mutex::new(registry)
});
```

### 4. Example Command Implementation

```rust
struct SpecKitNewCommand;

impl SpecKitCommand for SpecKitNewCommand {
    fn name(&self) -> &'static str {
        "speckit.new"
    }

    fn aliases(&self) -> &[&'static str] {
        &["new-spec"] // Backward compatibility
    }

    fn description(&self) -> &'static str {
        "create new SPEC from description with templates (55% faster)"
    }

    fn execute(&self, widget: &mut ChatWidget, args: String) {
        // Delegate to existing handler
        super::handler::handle_spec_kit_new(widget, args);
    }

    fn requires_args(&self) -> bool {
        true
    }
}
```

### 5. App.rs Integration

```rust
// app.rs - Before enum matching
fn handle_slash_command(&mut self, command_text: &str, args: String) {
    // Try spec-kit registry first
    if let Ok(registry) = SPEC_KIT_REGISTRY.lock() {
        if let Some(cmd) = registry.find(command_text) {
            if let AppState::Chat { widget } = &mut self.app_state {
                cmd.execute(widget, args);
                return;
            }
        }
    }

    // Fall back to upstream enum
    let parsed = match SlashCommand::from_str(command_text) {
        Some(cmd) => cmd,
        None => {
            // Unknown command
            return;
        }
    };

    match parsed {
        SlashCommand::Browser => { /* upstream */ }
        SlashCommand::New => { /* upstream */ }
        // No more spec-kit variants here!
    }
}
```

---

## Migration Strategy

### Phase 1: Infrastructure (This PR)
1. Create `command_registry.rs` module
2. Define `SpecKitCommand` trait
3. Implement `CommandRegistry`
4. Add global `SPEC_KIT_REGISTRY`

### Phase 2: Command Migration
1. Implement each command struct (30+ commands)
2. Register in global registry
3. Keep enum variants temporarily

### Phase 3: Routing Update
1. Update `app.rs` to check registry first
2. Validate all commands still work
3. Run full test suite

### Phase 4: Cleanup
1. Remove spec-kit enum variants from `SlashCommand`
2. Remove spec-kit routing from `app.rs`
3. Update documentation

---

## Benefits

✅ **Zero enum conflicts**: New commands don't touch `SlashCommand`
✅ **Isolated in spec_kit**: All command code stays fork-isolated
✅ **Dynamic discovery**: Can list commands programmatically
✅ **Backward compatible**: Aliases support old command names
✅ **Extensible**: Easy to add new commands

---

## Trade-offs

**Pros:**
- Eliminates highest conflict surface (enum)
- Enables runtime command registration
- Better separation of concerns

**Cons:**
- Dynamic dispatch overhead (negligible for commands)
- Slightly more complex than enum
- Requires Mutex for global state

---

## File Structure

```
tui/src/chatwidget/spec_kit/
├── mod.rs (add command_registry module)
├── command_registry.rs (trait + registry)
├── commands/
│   ├── mod.rs
│   ├── new.rs (SpecKitNewCommand)
│   ├── plan.rs (SpecKitPlanCommand)
│   ├── tasks.rs (SpecKitTasksCommand)
│   └── ... (30+ command structs)
└── handler.rs (existing handlers, called by commands)
```

---

## Acceptance Criteria

- [ ] `SpecKitCommand` trait defined
- [ ] `CommandRegistry` implemented with `find()`, `register()`, `iter()`
- [ ] Global `SPEC_KIT_REGISTRY` created
- [ ] All 30+ spec-kit commands migrated to registry
- [ ] `app.rs` updated to check registry before enum
- [ ] All slash commands still work (manual testing)
- [ ] No clippy warnings or build errors
- [ ] Enum variants can be safely removed (optional for Phase 4)

---

## Estimated Effort

**Total:** 12-15 hours

- Design & infrastructure: 2-3 hours ✅ (this document)
- Command implementations: 6-8 hours
- Routing integration: 2-3 hours
- Testing & validation: 2 hours

---

## Next Steps

1. Review this design
2. Implement `command_registry.rs`
3. Start migrating commands one-by-one
4. Update routing in `app.rs`
5. Validate and mark T74 complete
