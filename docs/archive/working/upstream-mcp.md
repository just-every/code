# Upstream ACP Integration – Design Notes

## Context

Upstream PR [openai/codex#1707](https://github.com/openai/codex/pull/1707) introduces an
experimental Agent Client Protocol (ACP) bridge so Zed can drive Codex via the
`session/new` and `session/prompt` tool calls. The branch (fetched locally as
`upstream-pr-1707`) rewrites large portions of `code-rs/core`, the MCP server,
and the TypeScript CLI/TUI to accommodate the new workflow.

### Current Status (2025-09-22)

- **Core** – Apply-patch execution now runs in-process with the ACP filesystem
  shim, preserving Code’s validation harness and approval prompts while
  emitting ACP tool-call metadata for downstream consumers.
- **MCP Server** – `session/new` and `session/prompt` are available alongside
  existing Codex tools. The new `acp_tool_runner` bridges Codex events to ACP
  `session/update` notifications and reuses the existing conversation manager to track live
  sessions.
- **Configuration** – `ConfigOverrides` and the TOML schema understand
  `experimental_client_tools` plus inline MCP server definitions, allowing IDE
  clients to announce their ACP capabilities without dropping legacy settings.
- **Validation** – `./build-fast.sh` passes after the integration, and new MCP
  tests cover ACP list-tool discovery plus a prompt roundtrip.

Code diverges substantially from upstream in these same areas: we retain the
Rust TUI, trimmed CLI scripts, and extended core features such as confirm
guards, queued user input, plan tool updates, and sandbox policies tuned for our
workflow. Directly rebasing onto upstream would drop or regress many of these
capabilities.

## What Upstream Added

- **Core (`code-rs/core`)**
  - New `acp.rs` module with helpers for translating Codex exec/patch events
    into ACP `ToolCall` updates, including an ACP-backed `FileSystem` shim and a
    permission-request flow (`acp::request_permission`).
  - `codex.rs` rewritten around `agent_client_protocol`, removing many existing
    operations (`QueueUserInput`, `RegisterApprovedCommand`, validation toggles,
    etc.) and introducing new event wiring for ACP tool calls.
  - `protocol.rs` pared down to match the simplified upstream surface (fewer
    `Op` variants, different `AskForApproval` options, new MCP call events).
- **MCP server (`code-rs/mcp-server`)**
  - New `acp_tool_runner.rs` that spawns Codex sessions on demand, relays MCP
    notifications, and surfaces ACP updates.
  - `message_processor.rs` extended to expose `session/new` and
    `session/prompt`, and to translate Codex events into ACP notifications (`session/update`).
- **CLI/TUI (Node + Rust)**
  - TypeScript terminal UI completely replaced with ACP-first experience.
  - Rust TUI and associated tests removed.

## Code-Specific Functionality We Must Preserve

- **Protocol & Ops** – `QueueUserInput`, validation toggles, confirm guards,
  rich approval semantics (`ReviewDecision::ApprovedForSession`), and sandbox
  policy options currently in `code-rs/core/src/protocol.rs:1`.
- **Execution Safety & Logging** – Confirm-guard enforcement and the richer
  `EventMsg` variants emitted from our `code-rs/core/src/codex.rs:1`.
- **TUI** – Entire Rust TUI stack (`code-rs/tui/src/chatwidget.rs:1`,
  `code-rs/tui/src/history_cell.rs:1`, etc.) must remain functional and ignore
  unknown ACP events gracefully.
- **CLI Footprint** – Our trimmed `codex-cli` structure and scripts differ from
  upstream’s wholesale replacement; we will not adopt the TypeScript overhaul.
- **Config Schema** – Existing TOML fields (confirm guards, validation toggles,
  sandbox defaults) must stay intact.

## Integration Approach

1. **Introduce ACP Helpers Without Regressions**
   - Port `code-rs/core/src/acp.rs:1` (and dependent structs) into Code, but
     adapt it to reuse the current `FileChange` representations and respect
     confirm guard / approval flows.
   - Extend `code-rs/core/src/codex.rs:1` to emit ACP events while preserving
     the existing `Op` variants, queueing logic, and validation toggles.
   - Update `code-rs/core/src/util.rs:1`, `code-rs/core/src/apply_patch.rs:1`,
     and related modules so ACP tool-call generators can derive the same
     metadata our TUI already consumes.

2. **MCP Server Wiring**
   - Add `code-rs/mcp-server/src/acp_tool_runner.rs:1` and integrate it with
     `message_processor.rs:1`, ensuring we keep Code-specific auth/sandbox setup
     and error reporting.
   - Maintain existing MCP tools and ensure ACP tools are opt-in (guarded by
     config or feature flag) so Code retains current behavior when ACP is
     unused.

3. **Event & Frontend Compatibility**
   - Extend our event enums (`code-rs/core/src/protocol.rs:1`) with any new
     variants required by ACP while keeping deprecated ones for backward
     compatibility.
   - Teach the Rust TUI to ignore or minimally display ACP-specific events so
     terminal UX does not panic when ACP notifications flow through.

4. **Config & Build**
   - Add `agent-client-protocol` dependency where needed, updating
     `code-rs/core/Cargo.toml:1`, `code-rs/mcp-server/Cargo.toml:1`, and
     `Cargo.lock`.
   - Introduce configuration toggles (if any) in `code-rs/core/src/config.rs:1`
     and `code-rs/core/src/config_types.rs:1` without breaking existing TOML
     files.
   - Update documentation (targeted sections in `docs/experimental.md:1` or a
     new page) to describe ACP/Zed support plus Code-specific caveats.

5. **Testing & Validation**
   - Add focused tests covering ACP request flow (unit-level in core and MCP
     server). Reuse existing harnesses (e.g., `code-rs/mcp-server/tests`) to
     simulate a session.
   - Validate end-to-end via `./build-fast.sh` only, honoring our policy against
     running additional formatters automatically.

## Open Questions

- How should ACP be surfaced in Code’s configuration (auto-enabled vs per
  server flag)?
- Do we expose ACP status in the Rust TUI, or treat it as headless-only? (Lean
  toward headless-first with optional UI indicators.)
- Are there additional permission flows (e.g., review approvals) required for
  Zed beyond what `ReviewDecision` already provides?

Document last updated: 2025-09-22.
# Upstream MCP Client Implementation Guide

Quick reference for implementing the upstream reuse strategy.

## Quick Start: Migrate to Upstream Dependencies

### 1. Update Workspace Cargo.toml

```toml
# code-rs/Cargo.toml
[workspace.dependencies]
# Replace fork dependencies with upstream
codex-mcp-client = { path = "../codex-rs/mcp-client" }
codex-responses-api-proxy = { path = "../codex-rs/responses-api-proxy" }
codex-process-hardening = { path = "../codex-rs/process-hardening" }

# Optional: Keep re-export aliases for gradual migration
code-mcp-client = { path = "../codex-rs/mcp-client", package = "codex-mcp-client" }
code-responses-api-proxy = { path = "../codex-rs/responses-api-proxy", package = "codex-responses-api-proxy" }
code-process-hardening = { path = "../codex-rs/process-hardening", package = "codex-process-hardening" }
```

### 2. Update Core Dependencies

```toml
# code-rs/core/Cargo.toml
[dependencies]
# Option A: Direct migration
codex-mcp-client = { workspace = true }

# Option B: Gradual migration with alias
code-mcp-client = { workspace = true }
```

### 3. Handle Binary Naming (If Required)

#### Option A: Build Script Rename

```rust
// code-rs/cli/build.rs
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    // If embedding binary
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Copy upstream binary with code- prefix
    let upstream_bin = "../target/release/codex-responses-api-proxy";
    let renamed_bin = out_dir.join("code-responses-api-proxy");

    if PathBuf::from(upstream_bin).exists() {
        fs::copy(upstream_bin, renamed_bin)
            .expect("Failed to copy responses-api-proxy binary");
    }
}
```

#### Option B: Accept Upstream Naming

```rust
// code-rs/cli/src/whatever_uses_the_binary.rs
// Simply use codex- prefix everywhere
const PROXY_BINARY: &str = "codex-responses-api-proxy";
```

### 4. Create Buffer Size Wrapper (If Needed)

Test first if default buffer is sufficient. If not:

```rust
// code-rs/core/src/mcp/client_wrapper.rs
use codex_mcp_client::McpClient;
use std::collections::HashMap;
use std::ffi::OsString;

/// Creates MCP client optimized for large responses (code-rs specific)
pub async fn create_large_response_client(
    program: OsString,
    args: Vec<OsString>,
    env: Option<HashMap<String, String>>,
) -> std::io::Result<McpClient> {
    // For now, upstream implementation may already handle this well
    // TODO: Benchmark with actual large responses before customizing
    McpClient::new_stdio_client(program, args, env).await
}

// If buffer customization proves necessary, contribute upstream PR
// See: upstream-mcp-reuse-strategy.md Strategy B
```

### 5. Update Imports

```rust
// Before (code-rs fork)
use code_mcp_client::McpClient;
use code_process_hardening;

// After (upstream direct)
use codex_mcp_client::McpClient;
use codex_process_hardening;

// Or with re-export alias (no code changes needed)
use code_mcp_client::McpClient;  // Still works via Cargo.toml alias
use code_process_hardening;       // Still works via Cargo.toml alias
```

## Testing the Migration

### 1. Unit Tests

```bash
cd code-rs
cargo test -p core  # Test MCP client integration
cargo test -p cli   # Test proxy binary handling
```

### 2. Integration Tests with Large Payloads

```rust
// code-rs/core/tests/mcp_large_response_test.rs
#[tokio::test]
async fn test_large_tool_response() {
    // Generate 2MB JSON response
    let large_payload = generate_large_json_payload(2 * 1024 * 1024);

    // Test MCP client handles it without truncation
    let client = create_test_mcp_client().await.unwrap();
    let result = client.call_tool(
        "large_data_tool".to_string(),
        Some(large_payload),
        Some(Duration::from_secs(30))
    ).await;

    assert!(result.is_ok());
    // Validate full payload received
}
```

### 3. Security Validation

```bash
# Verify process hardening still active
cargo build --release
./target/release/code-responses-api-proxy --help

# On Linux: Verify non-dumpable
cat /proc/$(pgrep code-responses)/status | grep Dumpable
# Should show: Dumpable: 0

# On macOS: Verify ptrace protection
# Attempt: lldb -p $(pgrep code-responses)
# Should fail: "Operation not permitted"
```

## Upstream Contribution Workflow

### Proposing Buffer Configuration Feature

```rust
// Proposed upstream change for codex-rs/mcp-client
// File: codex-rs/mcp-client/src/mcp_client.rs

/// Configuration options for MCP client
#[derive(Debug, Clone)]
pub struct McpClientConfig {
    /// Buffer capacity for reading server responses.
    /// Default: 8KB (Tokio default)
    /// Use larger values (e.g., 1MB) for servers with large tool responses
    pub buffer_capacity: Option<usize>,
}

impl Default for McpClientConfig {
    fn default() -> Self {
        Self {
            buffer_capacity: None,
        }
    }
}

impl McpClient {
    // Existing method unchanged for compatibility
    pub async fn new_stdio_client(
        program: OsString,
        args: Vec<OsString>,
        env: Option<HashMap<String, String>>,
    ) -> std::io::Result<Self> {
        Self::new_stdio_client_with_config(
            program,
            args,
            env,
            McpClientConfig::default()
        ).await
    }

    // New method with configuration
    pub async fn new_stdio_client_with_config(
        program: OsString,
        args: Vec<OsString>,
        env: Option<HashMap<String, String>>,
        config: McpClientConfig,
    ) -> std::io::Result<Self> {
        // ... existing setup code ...

        let reader_handle = {
            let pending = pending.clone();
            let mut lines = match config.buffer_capacity {
                Some(capacity) => BufReader::with_capacity(capacity, stdout).lines(),
                None => BufReader::new(stdout).lines(),
            };

            // ... rest of implementation ...
        };

        // ... rest of implementation ...
    }
}
```

### PR Description Template

```markdown
## Add buffer configuration to MCP client

### Motivation
When working with MCP servers that return large tool responses (>100KB),
the default buffer size can impact performance. This PR adds optional
buffer configuration while maintaining backward compatibility.

### Changes
- Add `McpClientConfig` struct with optional buffer_capacity
- Add `new_stdio_client_with_config` method
- Keep existing `new_stdio_client` method unchanged (uses default config)

### Testing
- Existing tests pass (backward compatibility verified)
- New test: large response handling with 1MB buffer
- Benchmark: 2MB response ~30% faster with larger buffer

### Breaking Changes
None - existing API unchanged, new method is additive

### Use Case (code-rs)
We use this in code-rs for MCP servers that return large file contents
or extensive analysis results. Setting buffer to 1MB eliminates multiple
read syscalls for these responses.
```

## Rollback Plan

If migration causes issues:

```toml
# Revert to fork in code-rs/Cargo.toml
[workspace.dependencies]
code-mcp-client = { path = "mcp-client" }
code-responses-api-proxy = { path = "responses-api-proxy" }
code-process-hardening = { path = "process-hardening" }
```

```bash
# Restore fork code
git checkout main -- code-rs/mcp-client
git checkout main -- code-rs/responses-api-proxy
git checkout main -- code-rs/process-hardening
```

## Performance Benchmarks

Before migration, establish baseline:

```rust
// code-rs/benches/mcp_client_bench.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_large_response(c: &mut Criterion) {
    c.bench_function("mcp_client_1mb_response", |b| {
        b.iter(|| {
            // Test current fork implementation
            let client = create_fork_client();
            let result = client.call_tool_with_large_response();
            black_box(result)
        })
    });
}

criterion_group!(benches, benchmark_large_response);
criterion_main!(benches);
```

After migration, compare:

```bash
cargo bench --bench mcp_client_bench > before.txt
# Apply migration
cargo bench --bench mcp_client_bench > after.txt
# Compare results
diff before.txt after.txt
```

## Monitoring Post-Migration

### CI/CD Checks

```yaml
# .github/workflows/mcp-upstream-health.yml
name: MCP Upstream Health Check
on:
  schedule:
    - cron: '0 8 * * 1'  # Weekly Monday 8am
  workflow_dispatch:

jobs:
  check-upstream:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
        with:
          submodules: true

      - name: Check for upstream changes
        run: |
          cd codex-rs
          git fetch origin
          git diff origin/main -- mcp-client/ responses-api-proxy/ process-hardening/

      - name: Run MCP integration tests
        run: |
          cargo test -p core -- mcp::
          cargo test -p cli -- proxy::

      - name: Security audit
        run: cargo audit
```

### Alerting

Set up GitHub notifications:
1. Watch codex-rs repository
2. Custom notification rules:
   - All activity in `mcp-client/`
   - All activity in `responses-api-proxy/`
   - All activity in `process-hardening/`
   - Security advisories

## Summary Checklist

- [ ] Update workspace dependencies to point to codex-rs
- [ ] Test with default buffer size first
- [ ] Implement wrapper only if needed (benchmark first)
- [ ] Update imports or use Cargo.toml aliases
- [ ] Run full test suite
- [ ] Validate process hardening on all platforms
- [ ] Benchmark performance before/after
- [ ] Set up upstream monitoring
- [ ] Prepare upstream PR for buffer config (if needed)
- [ ] Document migration in CHANGELOG
- [ ] Update internal documentation references

## Next Steps

1. **Immediate**: Test upstream `codex-mcp-client` with code-rs workloads
2. **Week 1**: Migrate mcp-client dependency if tests pass
3. **Week 2**: Migrate responses-api-proxy with binary rename
4. **Week 3**: Propose upstream buffer config PR
5. **Ongoing**: Monitor upstream changes weekly

See [upstream-mcp-reuse-strategy.md](./upstream-mcp-reuse-strategy.md) for detailed analysis and rationale.
# Upstream MCP Client Reuse Strategy

## Executive Summary

This document analyzes the feasibility of reusing `codex-rs/mcp-client` and `codex-rs/responses-api-proxy` directly from code-rs without maintaining separate forks. The investigation reveals **minimal divergence** between implementations, making upstream reuse highly feasible with a thin wrapper/patch strategy.

## Analysis of Current Divergence

### 1. MCP Client (`mcp-client`)

**Differences Found:**
- **Buffer Size**: code-rs uses 1MB buffer (`BufReader::with_capacity(1024 * 1024, stdout)`) vs codex-rs default buffer
  - Location: `mcp-client/src/mcp_client.rs:145`
  - Reason: Handle large tool responses without truncation
  - Impact: Performance optimization for specific use cases

**Similarities:**
- Identical package structure and dependencies
- Same API surface and MCP protocol implementation
- Same JSON-RPC message handling
- Same environment variable filtering logic
- File sizes: 476 vs 475 lines (virtually identical)

### 2. Responses API Proxy (`responses-api-proxy`)

**Differences Found:**
- **None** - The lib.rs files are byte-for-byte identical

**Naming Differences:**
- Package name: `code-responses-api-proxy` vs `codex-responses-api-proxy`
- Binary name: `code-responses-api-proxy` vs `codex-responses-api-proxy`
- Library name: `code_responses_api_proxy` vs `codex_responses_api_proxy`
- Process hardening dependency: `code-process-hardening` vs `codex-process-hardening`

### 3. Process Hardening (`process-hardening`)

**Differences Found:**
- **Minor style difference** in filter_map closure (code-rs uses compact `.then_some()`, codex-rs uses verbose if/else)
  - Location: `process-hardening/src/lib.rs:44-50` (Linux), `77-84` (macOS)
  - Functional equivalence: Identical behavior
  - Comment difference: Line 113 has `TODO(mbolin):` in codex-rs vs `TODO:` in code-rs

**Similarities:**
- Identical hardening strategy (core dumps, ptrace, env vars)
- Same platform-specific implementations
- Same error handling and exit codes

## Proposed Wrapper/Patch Strategy

### Strategy A: Direct Upstream Dependency (Recommended)

Use codex-rs crates directly with minimal wrapper to handle fork-specific needs.

**Implementation:**

1. **MCP Client**: Add configuration parameter for buffer size
   ```rust
   // In code-rs workspace Cargo.toml
   [dependencies]
   codex-mcp-client = { path = "../codex-rs/mcp-client" }

   // Create thin wrapper in code-rs if buffer config needed
   pub fn create_mcp_client_large_responses(
       program: OsString,
       args: Vec<OsString>,
       env: Option<HashMap<String, String>>,
   ) -> std::io::Result<codex_mcp_client::McpClient> {
       // Option 1: Use as-is (upstream buffer may be sufficient)
       codex_mcp_client::McpClient::new_stdio_client(program, args, env)

       // Option 2: Propose buffer size config upstream
       // codex_mcp_client::McpClient::with_buffer_capacity(1024*1024)
       //     .new_stdio_client(program, args, env)
   }
   ```

2. **Responses API Proxy**: Use upstream with renamed binary
   ```rust
   // In code-rs/cli binary embedding
   #[cfg(feature = "embed-binaries")]
   const RESPONSES_PROXY_BINARY: &[u8] =
       include_bytes!(concat!(env!("OUT_DIR"), "/codex-responses-api-proxy"));

   // Or: Copy/symlink binary during build with code- prefix
   ```

3. **Process Hardening**: Direct dependency, re-export if needed
   ```rust
   // In code-rs workspace
   pub use codex_process_hardening as code_process_hardening;
   ```

**Advantages:**
- Zero maintenance burden for core logic
- Automatic upstream security fixes and improvements
- Minimal code duplication

**Disadvantages:**
- Dependency on external codebase structure
- Binary naming requires build script handling

### Strategy B: Feature-Flag in Upstream

Contribute buffer configuration as optional feature to codex-rs.

**Upstream PR Proposal:**
```rust
// In codex-rs/mcp-client/src/mcp_client.rs
pub struct McpClientConfig {
    pub buffer_capacity: Option<usize>,
    // Future: timeout configs, retry logic, etc.
}

impl Default for McpClientConfig {
    fn default() -> Self {
        Self { buffer_capacity: None }
    }
}

impl McpClient {
    pub async fn new_stdio_client_with_config(
        program: OsString,
        args: Vec<OsString>,
        env: Option<HashMap<String, String>>,
        config: McpClientConfig,
    ) -> std::io::Result<Self> {
        // ... existing code ...

        let mut lines = if let Some(capacity) = config.buffer_capacity {
            BufReader::with_capacity(capacity, stdout).lines()
        } else {
            BufReader::new(stdout).lines()
        };

        // ... rest of implementation ...
    }
}
```

**Advantages:**
- Cleanest long-term solution
- Benefits both codebases
- No wrapper needed

**Disadvantages:**
- Requires upstream acceptance
- Timeline depends on upstream review

### Strategy C: Minimal Fork with Automated Sync

Maintain current fork structure but automate sync from upstream.

**Implementation:**
```bash
# .github/workflows/sync-upstream-mcp.yml
name: Sync Upstream MCP Components
on:
  schedule:
    - cron: '0 0 * * 1'  # Weekly
  workflow_dispatch:

jobs:
  sync:
    runs-on: ubuntu-latest
    steps:
      - name: Sync mcp-client
        run: |
          rsync -av --exclude=Cargo.toml \
            ../codex-rs/mcp-client/src/ \
            ./code-rs/mcp-client/src/
          # Apply code-rs specific patches
          patch -p1 < patches/mcp-client-buffer-size.patch
```

**Patch file** (`patches/mcp-client-buffer-size.patch`):
```diff
--- a/code-rs/mcp-client/src/mcp_client.rs
+++ b/code-rs/mcp-client/src/mcp_client.rs
@@ -141,7 +141,8 @@
         let reader_handle = {
             let pending = pending.clone();
-            let mut lines = BufReader::new(stdout).lines();
+            // Use a larger buffer size (1MB) to handle large tool responses
+            let mut lines = BufReader::with_capacity(1024 * 1024, stdout).lines();
```

**Advantages:**
- Automated tracking of upstream changes
- Clear patch management
- Fork-specific customization preserved

**Disadvantages:**
- Still maintains separate crate
- Patch conflicts require manual resolution

## Fork-Specific Modifications Required

### 1. Binary/Package Naming

**Current Naming:**
| Component | codex-rs | code-rs |
|-----------|----------|---------|
| MCP client package | `codex-mcp-client` | `code-mcp-client` |
| Proxy package | `codex-responses-api-proxy` | `code-responses-api-proxy` |
| Proxy binary | `codex-responses-api-proxy` | `code-responses-api-proxy` |
| Process hardening | `codex-process-hardening` | `code-process-hardening` |

**Resolution Strategies:**
1. **Build-time rename**: Copy and rename binaries during build
2. **Cargo alias**: Use `[[bin]]` section with custom name
3. **Accept upstream names**: Use `codex-*` binaries in code-rs (simplest)

### 2. Buffer Size Configuration

**Options:**
1. **Environment variable**: `MCP_BUFFER_SIZE=1048576`
2. **Config file parameter**: Add to MCP server config
3. **Upstream contribution**: Add to McpClientConfig (Strategy B)
4. **Accept default**: Test if upstream buffer is sufficient

### 3. Process Hardening Integration

**Current approach:**
```rust
// code-rs binaries
use code_process_hardening;

#[ctor::ctor]
fn pre_main() {
    code_process_hardening::pre_main_hardening();
}
```

**Upstream reuse:**
```rust
// Option 1: Re-export
pub use codex_process_hardening as code_process_hardening;

// Option 2: Direct use
use codex_process_hardening;

#[ctor::ctor]
fn pre_main() {
    codex_process_hardening::pre_main_hardening();
}
```

## Recommended Approach

**Phase 1: Immediate (Low-Risk Migration)**
1. Depend on `codex-mcp-client` directly from code-rs
2. Test with default buffer size - may already be sufficient
3. If buffer issues arise, implement thin wrapper (Strategy A, Option 1)
4. Use `codex-process-hardening` via re-export

**Phase 2: Short-Term (1-2 weeks)**
1. Propose upstream PR for buffer configuration (Strategy B)
2. Migrate responses-api-proxy to use upstream with build-time binary rename
3. Document any remaining customization needs

**Phase 3: Long-Term Maintenance**
1. Establish weekly automated checks for upstream changes
2. Participate in upstream development for shared needs
3. Maintain minimal patch set (ideally zero) for fork-specific requirements

## Upstream Parity Maintenance Checklist

### Weekly Tasks
- [ ] Check codex-rs commits to mcp-client, responses-api-proxy, process-hardening
- [ ] Review upstream issues/PRs that may affect code-rs usage
- [ ] Run integration tests with latest upstream commits
- [ ] Update dependency pins if changes detected

### Per-Upstream-Release Tasks
- [ ] Review changelog for breaking changes
- [ ] Test buffer size behavior with new release
- [ ] Validate process hardening still meets security requirements
- [ ] Update code-rs documentation if API changes
- [ ] Run full test suite with new upstream version

### Monthly Tasks
- [ ] Measure and document any performance differences
- [ ] Review need for fork-specific customizations
- [ ] Propose upstream contributions for shared needs
- [ ] Audit dependency tree for security updates

### Monitoring Triggers
- [ ] Set up GitHub notifications for codex-rs MCP-related commits
- [ ] Create alerts for upstream security advisories
- [ ] Track upstream issue tracker for relevant bug reports
- [ ] Monitor codex-rs release notes for MCP changes

### Testing Requirements
- [ ] Verify large tool response handling (>1MB payloads)
- [ ] Test MCP client with various server implementations
- [ ] Validate process hardening on all target platforms (Linux, macOS, Windows)
- [ ] Benchmark buffer size impact on performance
- [ ] Security audit of responses-api-proxy authentication

### Documentation Maintenance
- [ ] Keep this document updated with any new divergences
- [ ] Document reasons for any fork-specific patches
- [ ] Maintain migration guide for upstream API changes
- [ ] Track technical debt and sunset timeline for workarounds

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Upstream breaks API | Low | High | Pin versions, automated testing |
| Buffer size regression | Medium | Medium | Integration tests with large payloads |
| Binary rename conflicts | Low | Low | Build script validation |
| Security patch delay | Low | High | Automated weekly sync, subscribe to advisories |
| Upstream abandonment | Very Low | High | Fork maintained as fallback (current state) |

## Conclusion

**The investigation reveals that code-rs can effectively reuse codex-rs MCP components with minimal overhead.** The primary difference (1MB buffer) is a trivial configuration change that can be:
1. Tested to determine if even necessary
2. Implemented as a thin wrapper
3. Proposed upstream for mutual benefit

**Recommended Action**: Proceed with **Strategy A** (Direct Upstream Dependency) for immediate benefits, while pursuing **Strategy B** (Feature-Flag in Upstream) for long-term sustainability.

**Expected Outcome**: Reduced maintenance burden, automatic security updates, and stronger collaboration with upstream codex-rs development.
