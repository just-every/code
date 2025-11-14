# Workstream B Migration: Prototype Documentation

## Overview

This document describes the prototype migration of two representative crates from `codex-rs` to `code-rs` as part of Workstream B. The goal is to eliminate dependencies on `../codex-rs` for these crates and have them fully integrated into the `code-rs` workspace.

## Migrated Crates

### 1. codex-ansi-escape → code-ansi-escape
- **Location**: `code-rs/ansi-escape/`
- **Complexity**: Simple utility crate with no internal codex-rs dependencies
- **Dependencies**: `ansi-to-tui`, `ratatui`, `tracing`
- **Source files**: 1 library file (`lib.rs`)
- **Tests**: No unit tests in original crate

### 2. codex-mcp-client → code-mcp-client
- **Location**: `code-rs/mcp-client/`
- **Complexity**: Moderate - async client with multiple source files
- **Dependencies**: `anyhow`, `mcp-types`, `serde`, `serde_json`, `tracing`, `tracing-subscriber`, `tokio`
- **Source files**: 3 files (`lib.rs`, `main.rs`, `mcp_client.rs`)
- **Tests**: 1 unit test passing

## Migration Steps

### Step 1: Analyze Current State

Before migration, both crates existed in `code-rs` as thin wrapper crates that re-exported from `codex-rs`:

```rust
// code-rs/ansi-escape/src/lib.rs (before)
pub use codex_ansi_escape::*;

// code-rs/mcp-client/src/lib.rs (before)
pub use codex_mcp_client::*;
```

The `code-rs/Cargo.toml` workspace manifest included:
```toml
codex-mcp-client = { path = "../codex-rs/mcp-client" }
codex-ansi-escape = { path = "../codex-rs/ansi-escape" }
```

### Step 2: Copy Source Files

For each crate, copy all source files from `codex-rs/<crate-name>/` to `code-rs/<crate-name>/`:

```bash
# ansi-escape
cp codex-rs/ansi-escape/src/lib.rs code-rs/ansi-escape/src/lib.rs
cp codex-rs/ansi-escape/README.md code-rs/ansi-escape/README.md

# mcp-client
cp codex-rs/mcp-client/src/lib.rs code-rs/mcp-client/src/lib.rs
cp codex-rs/mcp-client/src/mcp_client.rs code-rs/mcp-client/src/mcp_client.rs
```

Note: `mcp-client/src/main.rs` was already present in code-rs and did not need copying.

### Step 3: Update Crate Cargo.toml Files

Update the individual crate manifests to replace `codex-*` dependencies with direct dependencies:

**code-rs/ansi-escape/Cargo.toml:**
```toml
# Before
[dependencies]
codex-ansi-escape = { workspace = true }

# After
[dependencies]
ansi-to-tui = { workspace = true }
ratatui = { workspace = true, features = [
    "unstable-rendered-line-info",
    "unstable-widget-ref",
] }
tracing = { workspace = true, features = ["log"] }
```

**code-rs/mcp-client/Cargo.toml:**
```toml
# Before
[dependencies]
codex-mcp-client = { workspace = true }
anyhow = { workspace = true }
# ... other deps

# After
[dependencies]
anyhow = { workspace = true }
mcp-types = { workspace = true }
# ... (removed codex-mcp-client line)
```

### Step 4: Update Workspace Cargo.toml

Remove the `codex-*` workspace dependencies for the migrated crates from `code-rs/Cargo.toml`:

```toml
# Removed these lines:
# codex-mcp-client = { path = "../codex-rs/mcp-client" }
# codex-ansi-escape = { path = "../codex-rs/ansi-escape" }
```

### Step 5: Build and Test

Build and test each migrated crate individually:

```bash
cd code-rs

# Build and test code-ansi-escape
cargo build -p code-ansi-escape
cargo test -p code-ansi-escape

# Build and test code-mcp-client
cargo build -p code-mcp-client
cargo test -p code-mcp-client
```

**Results:**
- ✅ code-ansi-escape: Built successfully, 0 tests (none in original)
- ✅ code-mcp-client: Built successfully, 1 test passed

### Step 6: Verify Dependent Crates

Verify that crates depending on the migrated crates still build and test correctly:

```bash
# Verify code-tui (uses code-ansi-escape)
cargo build -p code-tui

# Verify code-core (uses code-mcp-client)
cargo build -p code-core
cargo test -p code-core --lib
```

**Results:**
- ✅ code-tui: Built successfully
- ✅ code-core: Built successfully, 438 tests passed

## Pitfalls and Issues Encountered

### 1. Pre-existing Workspace Issue

**Issue**: During testing, discovered a pre-existing workspace configuration issue where `git-apply` crate referenced `regex = { workspace = true }` but the workspace didn't define `regex` in `workspace.dependencies`.

**Resolution**: Added `regex = "1"` to the workspace dependencies in `code-rs/Cargo.toml`.

**Impact**: This was not caused by the migration but was discovered during validation. Fixed to allow workspace operations to proceed.

### 2. Wrapper Pattern

**Observation**: The original wrapper pattern (`pub use codex_*::*;`) allowed for gradual migration. This meant consumers didn't need updates during migration.

**Benefit**: Zero changes required in consuming crates (`code-tui`, `code-core`, etc.).

## Scaling to Remaining Crates

### Recommended Approach

Based on this prototype, the following process should scale well to the remaining crates:

1. **Classify crates by complexity:**
   - Simple (like ansi-escape): Just source copying and dependency updates
   - Moderate (like mcp-client): Multiple files but no internal codex-rs dependencies
   - Complex: May have internal dependencies on other codex-rs crates

2. **Migration order:**
   - Start with leaf crates (no internal codex-rs dependencies)
   - Work up the dependency tree
   - Use `cargo tree` to visualize dependencies

3. **Automation opportunities:**
   - Script to copy source files
   - Script to update Cargo.toml dependencies
   - Automated testing pipeline

### Crates Still Depending on codex-rs

From `code-rs/Cargo.toml`, the remaining codex-rs dependencies are:
- `codex-backend-client`
- `codex-cloud-tasks-client`
- `codex-execpolicy`
- `codex-git-apply`
- `codex-linux-sandbox`
- `codex-process-hardening`
- `codex-responses-api-proxy`
- `upstream-mcp-types` (special case: this is `codex-rs/mcp-types`)

### Special Considerations

#### mcp-types

There are TWO mcp-types packages:
- `code-mcp-types` in `code-rs/mcp-types/`
- `mcp-types` (upstream) in `codex-rs/mcp-types/`

The workspace uses:
```toml
mcp-types = { path = "mcp-types", package = "code-mcp-types" }
upstream-mcp-types = { path = "../codex-rs/mcp-types", package = "mcp-types" }
```

Migration of mcp-types requires careful analysis of which version is used where.

#### Inter-crate Dependencies

Some codex-rs crates may depend on other codex-rs crates. These need to be identified and migrated together or in the correct order.

### Validation Strategy

For each migrated crate:

1. **Direct validation:**
   ```bash
   cargo build -p <crate-name>
   cargo test -p <crate-name>
   cargo clippy -p <crate-name>
   ```

2. **Consumer validation:**
   ```bash
   # Find all consumers
   grep -r "code-<crate-name>" code-rs/*/Cargo.toml

   # Build each consumer
   cargo build -p <consumer-name>
   cargo test -p <consumer-name>
   ```

3. **Full workspace validation:**
   ```bash
   cargo check --workspace
   cargo test --workspace
   ```

## Follow-up Work

### Immediate Next Steps

1. **Migrate the next batch of simple crates:**
   - Likely candidates: `execpolicy`, `git-apply`, `linux-sandbox`
   - These appear to be utility crates without complex dependencies

2. **Document dependency graph:**
   - Generate visual dependency graph of remaining codex-rs crates
   - Identify migration order based on dependencies

3. **Create migration automation:**
   - Shell script to automate file copying
   - Script to update Cargo.toml files
   - Integration testing script

### Medium-term Tasks

1. **Handle complex crates:**
   - `backend-client`, `cloud-tasks-client`, `responses-api-proxy`
   - These may have more complex dependencies or external API contracts

2. **Resolve mcp-types duality:**
   - Understand why both versions exist
   - Plan migration or consolidation strategy

3. **Clean up codex-rs:**
   - After migration, remove migrated crates from codex-rs
   - Update codex-rs workspace manifest

### Long-term Validation

1. **CI/CD pipeline:**
   - Ensure all tests pass in CI
   - Add regression tests for migrated crates

2. **Performance validation:**
   - Ensure no performance regressions from migration
   - Binary size comparisons

3. **Documentation:**
   - Update architecture documentation
   - Update developer onboarding docs to remove codex-rs references

## Conclusion

The prototype migration of `code-ansi-escape` and `code-mcp-client` was successful. Both crates now live entirely in `code-rs` with no dependencies on `../codex-rs`. All builds and tests pass, including dependent crates.

The migration approach is straightforward and should scale well to the remaining crates. The key insight is that the existing wrapper pattern allows for seamless migration without requiring changes to consuming crates.

## Appendix: Command Reference

```bash
# Check what depends on a crate
cargo tree -p code-ansi-escape --invert

# Build specific crate
cargo build -p <crate-name>

# Test specific crate
cargo test -p <crate-name>

# Check workspace
cargo check --workspace

# Find all codex-rs dependencies in workspace
grep "codex-" code-rs/Cargo.toml

# Find consumers of a crate
grep -r "code-<crate>" code-rs/*/Cargo.toml
```

## Files Modified

- `code-rs/ansi-escape/src/lib.rs` - Replaced wrapper with actual implementation
- `code-rs/ansi-escape/Cargo.toml` - Updated dependencies
- `code-rs/ansi-escape/README.md` - Copied from codex-rs
- `code-rs/mcp-client/src/lib.rs` - Replaced wrapper with actual implementation
- `code-rs/mcp-client/src/mcp_client.rs` - Copied from codex-rs
- `code-rs/mcp-client/Cargo.toml` - Updated dependencies
- `code-rs/Cargo.toml` - Removed codex-ansi-escape and codex-mcp-client workspace deps, added missing regex dependency
