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
