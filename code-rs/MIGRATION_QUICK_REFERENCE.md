# Quick Reference: Crate Migration from codex-rs to code-rs

## TL;DR - Migration Steps

```bash
# 1. Use the migration script
cd /home/azureuser/code/code-rs
./migrate-crate.sh <crate-name>

# 2. Update Cargo.toml (copy deps from codex-rs, replace codex-* with code-*)
vim <crate-name>/Cargo.toml

# 3. Update lib.rs (replace wrapper with actual module structure)
vim <crate-name>/src/lib.rs

# 4. Update main.rs if binary (change codex_* to code_*)
vim <crate-name>/src/main.rs

# 5. Replace namespace references in all source files
find <crate-name>/src -name '*.rs' -exec sed -i 's/codex_core/code_core/g' {} +
find <crate-name>/src -name '*.rs' -exec sed -i 's/codex_<old_name>/code_<new_name>/g' {} +

# 6. Remove from workspace Cargo.toml
vim Cargo.toml  # Delete the codex-<crate> = { path = "../codex-rs/..." } line

# 7. Build and verify
cargo build -p code-<crate-name>
cargo test -p code-<crate-name>
```

## Pattern Example: linux-sandbox

### Before (Wrapper)

**Cargo.toml:**
```toml
[dependencies]
codex-linux-sandbox = { workspace = true }
```

**lib.rs:**
```rust
pub use codex_linux_sandbox::*;
```

### After (Independent)

**Cargo.toml:**
```toml
[target.'cfg(target_os = "linux")'.dependencies]
clap = { workspace = true, features = ["derive"] }
code-core = { workspace = true }
landlock = { workspace = true }
libc = { workspace = true }
seccompiler = { workspace = true }
```

**lib.rs:**
```rust
#[cfg(target_os = "linux")]
mod landlock;
#[cfg(target_os = "linux")]
mod linux_run_main;

#[cfg(target_os = "linux")]
pub fn run_main() -> ! {
    linux_run_main::run_main();
}
```

**main.rs:**
```rust
fn main() -> ! {
    code_linux_sandbox::run_main()  // Changed from codex_linux_sandbox
}
```

## Common Namespace Replacements

| From (codex-rs) | To (code-rs) |
|-----------------|--------------|
| `use codex_core::` | `use code_core::` |
| `codex_linux_sandbox::` | `code_linux_sandbox::` |
| `codex_backend_client::` | `code_backend_client::` |
| `codex-*` (in Cargo.toml) | `code-*` (if migrated) |

## Files to Modify

1. **`<crate>/Cargo.toml`** - Add real dependencies
2. **`<crate>/src/lib.rs`** - Replace wrapper with modules
3. **`<crate>/src/main.rs`** - Update namespace (if binary)
4. **`<crate>/src/*.rs`** - Replace codex_* → code_*
5. **`Cargo.toml`** (workspace root) - Remove codex-* dependency

## Validation Commands

```bash
# Check for remaining codex references
grep -r "codex_" code-rs/<crate>/src/

# Verify build
cargo build -p code-<crate>

# Run tests
cargo test -p code-<crate>

# Check dependents still work
cargo build -p <dependent-crate>
```

## Common Issues & Quick Fixes

| Issue | Fix |
|-------|-----|
| "cannot find crate codex_*" | Replace import with code_* |
| "dependency not found" | Copy ALL deps from codex-rs Cargo.toml |
| Tests don't compile | Copy entire tests/ directory structure |
| Platform-specific build fails | Copy #[cfg(...)] attributes |
| Workspace build fails | Remove codex-<crate> from workspace Cargo.toml |

## Migration Order (by dependency)

Migrate in this order to avoid dependency issues:

1. ✅ `code-core` (already done)
2. ✅ `code-linux-sandbox` (just completed)
3. Next candidates:
   - `ansi-escape` (no codex deps)
   - `git-apply` (minimal deps)
   - `process-hardening` (depends on core)
   - `backend-client` (depends on core)
   - Others as needed

## Success Criteria

- ✅ `cargo build -p code-<crate>` passes
- ✅ No `codex-<crate>` in workspace Cargo.toml
- ✅ No `use codex_*` in src/ (except unavoidable upstream)
- ✅ Dependent crates still build
- ✅ Tests pass (if exist)

## Full Documentation

See `MIGRATION_GUIDE.md` for detailed explanation, pitfalls, and troubleshooting.
