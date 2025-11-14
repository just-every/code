# Crate Migration Guide: codex-rs to code-rs

This guide documents the process for moving shared crates from `codex-rs` into `code-rs`, making them fully independent.

## Overview

The `code-rs` repository currently contains thin wrapper crates that re-export functionality from their `codex-rs` counterparts. This creates a cross-repository dependency via relative paths (`../codex-rs/...`). The migration process moves the implementation into `code-rs` so that the crate owns its code and no longer depends on `codex-rs`.

## Case Study: linux-sandbox Migration

We successfully migrated `code-linux-sandbox` as a representative example. This crate had:
- Multiple source modules (landlock.rs, linux_run_main.rs)
- A binary entry point (main.rs)
- Integration tests
- Platform-specific dependencies (Linux only)
- Dependencies on `codex-core` (which had already been migrated to `code-core`)

## Migration Pattern

### 1. Pre-Migration Analysis

Before starting, understand:
- **Source crate location**: `/home/azureuser/code/codex-rs/<crate-name>/`
- **Destination crate location**: `/home/azureuser/code/code-rs/<crate-name>/`
- **Current wrapper structure**: Usually just `pub use codex_<crate>::*;`
- **Dependencies**: What other crates does this depend on?
- **Consumers**: What crates in code-rs depend on this wrapper?

### 2. File Structure Migration

Copy all implementation files from codex-rs to code-rs:

```bash
# Source modules (excluding lib.rs which needs special handling)
cp codex-rs/<crate>/src/*.rs code-rs/<crate>/src/
# Exception: main.rs will need modification

# Tests (if they exist)
cp -r codex-rs/<crate>/tests/ code-rs/<crate>/tests/
```

**Files to copy:**
- All source modules: `src/*.rs` (except `lib.rs` - see below)
- Test files: `tests/**/*.rs`
- Integration test structure: `tests/all.rs`, `tests/suite/`

### 3. Update Cargo.toml

Replace the wrapper dependency with actual implementation dependencies.

**Before (wrapper):**
```toml
[dependencies]
codex-linux-sandbox = { workspace = true }
```

**After (independent):**
```toml
[target.'cfg(target_os = "linux")'.dependencies]
clap = { workspace = true, features = ["derive"] }
code-core = { workspace = true }  # Note: codex-core → code-core
landlock = { workspace = true }
libc = { workspace = true }
seccompiler = { workspace = true }

[target.'cfg(target_os = "linux")'.dev-dependencies]
tempfile = { workspace = true }
tokio = { workspace = true, features = ["io-std", "macros", "process", "rt-multi-thread", "signal"] }
```

**Key considerations:**
- Copy ALL dependency sections from codex-rs Cargo.toml
- Replace `codex-*` dependencies with `code-*` equivalents where they exist
- Keep the same features and version constraints
- Include both `[dependencies]` and `[dev-dependencies]`
- Handle platform-specific dependencies (e.g., `[target.'cfg(target_os = "linux")'.dependencies]`)

### 4. Update lib.rs

Replace the wrapper re-export with the actual module structure.

**Before (wrapper):**
```rust
//! Thin wrapper around the upstream `codex-linux-sandbox` crate.
pub use codex_linux_sandbox::*;
```

**After (independent):**
```rust
#[cfg(target_os = "linux")]
mod landlock;
#[cfg(target_os = "linux")]
mod linux_run_main;

#[cfg(target_os = "linux")]
pub fn run_main() -> ! {
    linux_run_main::run_main();
}

#[cfg(not(target_os = "linux"))]
pub fn run_main() -> ! {
    panic!("code-linux-sandbox is only supported on Linux");
}
```

**Key considerations:**
- Copy the exact module structure from `codex-rs/<crate>/src/lib.rs`
- Keep platform-specific conditional compilation (`#[cfg(...)]`)
- Preserve all public exports
- Update any codex-specific naming to code-specific

### 5. Update main.rs (if binary)

Change the namespace from `codex_*` to `code_*`.

**Before:**
```rust
fn main() -> ! {
    codex_linux_sandbox::run_main()
}
```

**After:**
```rust
/// Note that the cwd, env, and command args are preserved in the ultimate call
/// to `execv`, so the caller is responsible for ensuring those values are
/// correct.
fn main() -> ! {
    code_linux_sandbox::run_main()
}
```

### 6. Update Source Files: Replace Namespace References

Replace all `codex_*` module references with `code_*` equivalents:

```bash
# Example: Update codex_core to code_core
find code-rs/<crate>/src -name '*.rs' -exec sed -i 's/codex_core/code_core/g' {} +

# Update codex_<crate> to code_<crate> (with underscores)
find code-rs/<crate>/src -name '*.rs' -exec sed -i 's/codex_linux_sandbox/code_linux_sandbox/g' {} +
```

**Common replacements:**
- `use codex_core::` → `use code_core::`
- `codex_core::error::Result` → `code_core::error::Result`
- `codex_core::protocol::SandboxPolicy` → `code_core::protocol::SandboxPolicy`

**Files that commonly need updates:**
- Module implementation files (e.g., `landlock.rs`, `linux_run_main.rs`)
- Any file that imports from other crates
- Test files that reference the crate's types

### 7. Update Workspace Cargo.toml

Remove the cross-repository dependency reference.

**Location:** `/home/azureuser/code/code-rs/Cargo.toml`

**Before:**
```toml
[workspace.dependencies]
# ...
codex-linux-sandbox = { path = "../codex-rs/linux-sandbox" }
# ...
```

**After:**
```toml
[workspace.dependencies]
# ... (line removed)
```

### 8. Build and Verify

```bash
# Build the migrated crate
cd /home/azureuser/code/code-rs
cargo build -p code-<crate-name>

# Run tests
cargo test -p code-<crate-name>

# Build dependent crates (e.g., code-arg0 depends on code-linux-sandbox)
cargo build -p <dependent-crate>
```

**Success criteria:**
- `cargo build -p code-<crate>` compiles without errors
- Tests pass (if applicable)
- Dependent crates still build correctly
- No references to `codex-<crate>` remain in the migrated crate

## Automated Migration Script

Use the provided script for initial file copying:

```bash
cd /home/azureuser/code/code-rs
./migrate-crate.sh <crate-name>
```

The script will:
1. Copy source files from codex-rs to code-rs
2. Copy test files
3. Display the dependencies you need to add
4. Provide a checklist of manual steps

**Note:** The script handles the tedious file copying but requires manual steps for:
- Updating Cargo.toml dependencies
- Modifying lib.rs and main.rs
- Replacing namespace references
- Updating workspace configuration

## Common Pitfalls and Solutions

### Pitfall 1: Missing Dependencies

**Symptom:** Build fails with "cannot find crate" errors.

**Solution:** Ensure ALL dependencies from the codex-rs Cargo.toml are copied to code-rs Cargo.toml, including:
- Platform-specific dependencies (`[target.'cfg(...)'.dependencies]`)
- Dev dependencies for tests
- Feature flags on dependencies

**Example:**
```toml
# Don't forget platform-specific dependencies!
[target.'cfg(target_os = "linux")'.dependencies]
libc = { workspace = true }
```

### Pitfall 2: Incomplete Namespace Replacement

**Symptom:** Build fails with "use of undeclared crate or module `codex_*`" errors.

**Solution:** Search for ALL occurrences of `codex_` in the migrated crate:

```bash
grep -r "codex_" code-rs/<crate>/src/
grep -r "codex_" code-rs/<crate>/tests/
```

Replace with the appropriate `code_*` variant. Common locations:
- `use` statements
- Type annotations
- Function calls
- Comments and documentation (optional but recommended)

### Pitfall 3: Forgetting Platform-Specific Compilation

**Symptom:** Build succeeds but runtime behavior is incorrect, or code compiles on wrong platforms.

**Solution:** Preserve ALL `#[cfg(...)]` attributes from the original:

```rust
#[cfg(target_os = "linux")]  // Don't forget these!
mod landlock;

#[cfg(not(target_os = "linux"))]
pub fn run_main() -> ! {
    panic!("code-linux-sandbox is only supported on Linux");
}
```

### Pitfall 4: Test Directory Structure

**Symptom:** Tests don't compile or can't find test modules.

**Solution:** Copy the ENTIRE test directory structure, including:
- `tests/all.rs` (integration test entry point)
- `tests/suite/` directory with all modules
- `tests/suite/mod.rs` (module aggregation)

The pattern in codex-rs uses a single integration test binary:
```rust
// tests/all.rs
mod suite;

// tests/suite/mod.rs
mod landlock;
mod other_test_modules;
```

### Pitfall 5: Workspace Dependency Conflicts

**Symptom:** Cargo complains about duplicate dependencies or conflicting versions.

**Solution:**
1. Remove the `codex-<crate>` dependency from workspace Cargo.toml
2. Ensure `code-<crate>` is already defined in workspace dependencies
3. If dependents break, they may need to be updated to use the new independent crate

### Pitfall 6: Binary Crates with Libraries

**Symptom:** Binary doesn't link or can't find library functions.

**Solution:** Ensure BOTH lib and bin are defined in Cargo.toml:

```toml
[lib]
name = "code_linux_sandbox"  # Note: underscores
path = "src/lib.rs"

[[bin]]
name = "code-linux-sandbox"  # Note: hyphens
path = "src/main.rs"
```

The binary name uses hyphens, the library name uses underscores (Rust convention).

## Dependency Migration Order

Crates should be migrated in dependency order. Dependencies must be migrated before their dependents.

**Already migrated:**
- ✅ `code-core` (was `codex-core`)
- ✅ `code-linux-sandbox` (was `codex-linux-sandbox`)

**Remaining wrappers (identified order):**
1. Low-level utilities (no codex dependencies):
   - `ansi-escape` (identical implementation, trivial)
   - `git-apply` (minimal dependencies)

2. Process/system level:
   - `process-hardening` (may depend on core)
   - `execpolicy` (depends on core, process-hardening)

3. Networking/client:
   - `backend-client` (depends on core)
   - `cloud-tasks-client` (depends on backend-client)
   - `responses-api-proxy` (depends on backend-client)

**Note:** Run `grep -r "codex-" code-rs/*/Cargo.toml` to see current wrapper dependencies.

## Verification Checklist

After migration, verify:

- [ ] `cargo build -p code-<crate>` succeeds
- [ ] `cargo test -p code-<crate>` succeeds (if tests exist)
- [ ] No `codex-<crate>` dependency remains in workspace Cargo.toml
- [ ] No `use codex_*` imports remain in src/ (except for unavoidable upstream deps)
- [ ] All dependent crates still build: `cargo build -p <dependent>`
- [ ] Binary crates (if any) execute correctly
- [ ] Tests copied from codex-rs still pass
- [ ] Platform-specific behavior preserved (if applicable)

## File Checklist for Migration

For each crate, ensure you handle:

- [ ] `Cargo.toml` - Dependencies, features, and metadata
- [ ] `src/lib.rs` - Main library module structure
- [ ] `src/main.rs` - Binary entry point (if applicable)
- [ ] `src/*.rs` - All implementation modules
- [ ] `tests/all.rs` - Integration test entry point (if exists)
- [ ] `tests/suite/*.rs` - Individual test modules (if exists)
- [ ] Workspace `Cargo.toml` - Remove codex-* reference
- [ ] Dependent crates - Verify they still build

## Summary

The migration process transforms a thin wrapper crate into a fully independent implementation by:

1. **Copying** implementation files from codex-rs to code-rs
2. **Updating** Cargo.toml with actual dependencies (replacing codex-* with code-*)
3. **Replacing** wrapper re-exports with actual module structure in lib.rs
4. **Renaming** namespaces from codex_* to code_* throughout the code
5. **Removing** the codex-<crate> dependency from workspace Cargo.toml
6. **Building** and verifying the migrated crate and its dependents

The result is a crate that owns its implementation and no longer depends on `../codex-rs`, making code-rs more independent and easier to maintain.

## linux-sandbox Migration Results

The `code-linux-sandbox` crate was successfully migrated:
- **Build status:** ✅ Success (`cargo build -p code-linux-sandbox` in 1m 50s)
- **Files migrated:** 7 files (lib.rs, main.rs, landlock.rs, linux_run_main.rs, + 3 test files)
- **Dependencies:** 5 external + 1 internal (code-core)
- **Namespace changes:** 4 occurrences (codex_core → code_core, codex_linux_sandbox → code_linux_sandbox)
- **Workspace change:** Removed 1 line from workspace Cargo.toml

The crate now compiles independently without any reference to `codex-rs`.
