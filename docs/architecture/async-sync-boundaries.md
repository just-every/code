# Async/Sync Architecture Boundaries

**Status**: v1.0 (2025-10-18)
**Owner**: theturtlecsz
**References**: REVIEW.md (lines 255-283), SPEC.md (DOC-7)

---

## 1. Executive Summary

**Problem**: Ratatui TUI framework (synchronous) + Tokio async runtime (asynchronous) + MCP calls (async) create impedance mismatch

**Solution**: `Handle::block_on()` bridges async operations into sync TUI event loop

**Performance**: 8.7ms typical, 700ms cold-start worst-case (MCP initialization race)

**Tradeoffs**: Acceptable blocking for spec-kit workflows (infrequent, background-able)

---

## 2. The Async/Sync Tension

### 2.1 Component Requirements

| Component | Concurrency Model | Why |
|-----------|-------------------|-----|
| **Ratatui** | Synchronous | Terminal I/O is blocking (crossterm), render loop is sync |
| **Tokio** | Asynchronous | HTTP/MCP require non-blocking I/O, agent coordination needs concurrency |
| **Codex Core** | Asynchronous | Model API calls (SSE streaming), tool execution, conversation state |
| **Spec-Kit** | **Hybrid** | Orchestration logic (sync), MCP calls (async), evidence writes (sync) |

**Core Conflict**: TUI event loop runs synchronously, but spec-kit must call async MCP functions.

### 2.2 Architectural Choices

**Option 1: Pure Async TUI** (rejected)
- Use async TUI framework (e.g., `tui-realm`)
- **Pro**: No blocking, native async
- **Con**: Ratatui is mature/stable, async TUI frameworks are immature

**Option 2: Spawn Async Tasks** (partial)
- Spawn tokio tasks for MCP, poll via channels
- **Pro**: No blocking
- **Con**: Complex coordination, channel plumbing, harder debugging

**Option 3: Block on Async** (**selected**)
- Use `Handle::block_on()` for async calls in sync context
- **Pro**: Simple, explicit, works with existing Ratatui
- **Con**: Blocks TUI thread during async operations

**Decision**: Option 3 accepted because spec-kit workflows are:
- Infrequent (user-initiated `/speckit.*` commands)
- Not time-critical (10-60 min pipelines, 8ms blocking is negligible)
- User-visible (progress shown, not background work)

---

## 3. Implementation Details

### 3.1 Tokio Runtime Initialization

**Location**: `core/src/codex.rs:3174` (via `Codex::spawn()`)

**Lifecycle**:
```rust
// Main entry point (tui/src/lib.rs)
pub async fn run_main(cli: Cli) -> TokenUsage {
    // Tokio runtime already exists (spawned by CLI)
    let codex = Codex::spawn(config, auth).await?;
    let app = App::new(codex, mcp_manager).await?;

    // Event loop runs in tokio context
    run_event_loop(app).await
}
```

**Key Point**: TUI runs **inside** tokio context, so `Handle::current()` is available.

### 3.2 Blocking Bridge Pattern

**Pattern**: Sync function needs async operation

**Implementation**:
```rust
// tui/src/chatwidget/spec_kit/handler.rs:722
let consensus_result = match tokio::runtime::Handle::try_current() {
    Ok(handle) => {
        handle.block_on(run_consensus_with_retry(...))  // BLOCKING
    }
    Err(_) => {
        Err(SpecKitError::from_string("No tokio runtime"))
    }
};
```

**Why `block_on()` is safe here**:
- Called from TUI event loop (single-threaded)
- No risk of deadlock (not inside another async fn)
- User initiated (not background task)
- Progress shown in TUI (user knows it's working)

### 3.3 Blocking Hotspots

**Identified blocking points** (from REVIEW.md):

| Location | Operation | Typical Time | Worst Case | Frequency |
|----------|-----------|--------------|------------|-----------|
| `handler.rs:429` | Consensus check (plan stage) | 8.7ms | 700ms (cold) | Per stage (6×/pipeline) |
| `handler.rs:722` | Consensus check (implement) | 8.7ms | 700ms (cold) | Per stage |
| `handler.rs:900` | Consensus check (unlock) | 8.7ms | 700ms (cold) | Per stage |
| `evidence.rs:write_with_lock()` | File lock acquisition | <1ms | Unbounded (HDD) | Per artifact write (20×/pipeline) |

**Total pipeline blocking**: ~50ms typical, ~4s worst-case (6 stages × 700ms)

**User perception**: Imperceptible for 50ms, noticeable for 4s but acceptable (one-time cost)

---

## 4. Performance Characteristics

### 4.1 MCP Consensus Calls

**Measurement** (ARCH-002 benchmark, `tui/tests/mcp_consensus_benchmark.rs`):
- **Subprocess baseline**: 46ms
- **Native MCP**: 8.7ms
- **Improvement**: 5.3x faster

**Cold-start penalty** (first call):
- MCP server spawn: 500-700ms (stdio subprocess initialization)
- Subsequent calls: 8.7ms (persistent connection)

**Mitigation** (ARCH-005):
- App-level MCP manager spawn (once at startup)
- Shared across widgets (no multiplication)
- Warm by the time user runs `/speckit.*` command

### 4.2 Evidence Writes

**File lock acquisition** (`evidence.rs:write_with_lock()`):
- **SSD**: <1ms
- **HDD**: 10-50ms (seek time)
- **Network mount**: Unbounded (NFS/SMB latency)

**RAII guarantee**: Lock released even if panic (via Drop trait)

**Concurrency**: Exclusive locks per SPEC-ID (no global bottleneck)

### 4.3 Async Overhead

**Task spawning** (negligible):
- `tokio::spawn()` overhead: ~10μs
- Used for agent execution (not in critical path)

**Channel communication** (crossbeam):
- `AppEvent` channel send/recv: <1μs
- Used for event dispatch (TUI → App)

---

## 5. Mitigations & Optimizations

### 5.1 Retry Logic (AR-2, AR-3)

**Exponential backoff**:
```rust
// consensus.rs MCP retry
let mut retries = 0;
let mut backoff_ms = 100;

while retries < 3 {
    match mcp_manager.call_tool(...).await {
        Ok(result) => return Ok(result),
        Err(e) => {
            tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
            backoff_ms *= 2;  // 100ms → 200ms → 400ms
            retries += 1;
        }
    }
}
```

**Total worst-case**: 3 × (8.7ms + backoff) = 26ms + 700ms = ~730ms

### 5.2 File-Based Fallback (ARCH-002)

**MCP unavailable → evidence directory**:
```rust
match fetch_memory_entries(spec_id, stage, mcp_manager).await {
    Ok((entries, warnings)) => Ok((entries, warnings)),
    Err(mcp_err) => {
        // Fallback: load from evidence/consensus/<SPEC-ID>/
        load_artifacts_from_evidence(evidence_root, spec_id, stage)
    }
}
```

**Impact**: 10-50ms file reads (vs 8.7ms MCP), but prevents hard failure

### 5.3 Progress Indication

**User feedback during blocking**:
```rust
// Show progress in TUI
widget.history_push(format!("⏳ Checking consensus (may take 1-2s)..."));
widget.render()?;

// Perform blocking operation
let result = handle.block_on(run_consensus_with_retry(...))?;

// Update progress
widget.history_push(format!("✅ Consensus complete"));
```

**Perception**: User knows something is happening, reduces perceived lag

---

## 6. Concurrency Correctness

### 6.1 No Deadlocks

**Safe because**:
- `block_on()` called from sync context (no nested async)
- No circular waits (consensus → MCP → network, no callbacks)
- File locks are per-SPEC (no global lock contention)

**Forbidden pattern** (would deadlock):
```rust
async fn foo() {
    tokio::runtime::Handle::current().block_on(async {
        // DEADLOCK: blocking inside async fn
    });
}
```

**Our pattern** (safe):
```rust
fn sync_handler() {  // Not async
    tokio::runtime::Handle::current().block_on(async {
        // OK: blocking from sync context
    });
}
```

### 6.2 Thread Safety

**TUI event loop**: Single-threaded (no race conditions)

**MCP connections**: `Arc<Mutex<Option<Arc<McpConnectionManager>>>>` (ARCH-005)
- Mutex protects access
- Arc allows sharing across widgets

**Evidence writes**: `fs2::FileExt` exclusive locks (ARCH-007)
- OS-level file locking
- Prevents concurrent writes to same SPEC

---

## 7. Future Improvements

### 7.1 Async File I/O (Low Priority)

**Current**: Synchronous `std::fs` operations
**Proposal**: Use `tokio::fs` for async file writes

**Pro**:
- Non-blocking evidence writes
- Better concurrency

**Con**:
- Minimal benefit (writes are <1ms)
- Increased complexity
- Only helps on slow disks (rare)

**Decision**: Defer unless evidence writes become bottleneck

### 7.2 Timeout Guards (Medium Priority)

**Current**: No hard timeout on `block_on()`
**Proposal**: Wrap in `tokio::time::timeout()`

**Implementation**:
```rust
let result = tokio::time::timeout(
    Duration::from_secs(5),  // Kill after 5s
    handle.block_on(run_consensus_with_retry(...))
)?;
```

**Pro**:
- Prevents infinite hangs
- Better UX (fail fast)

**Con**:
- 5s may be too short for slow networks
- Requires tuning per operation

**Status**: Consider for next version

### 7.3 Extract Spec-Kit to Async Crate (High Value, High Effort)

**Current**: Spec-kit embedded in TUI (sync context)
**Proposal**: Separate `codex-spec-kit` crate (async-first API)

**Benefits**:
- Native async (no `block_on()`)
- Reusable (CLI, API server, CI)
- Better testability
- Reduces TUI coupling

**Effort**: 2-4 weeks (REVIEW.md estimate)

**Status**: Deferred until upstream sync friction or reusability need arises

---

## 8. Comparative Analysis

### 8.1 vs Pure Async Approach

**Hypothetical**: Make TUI fully async

**Required**:
- Switch to async TUI framework (e.g., `tui-realm`)
- Rewrite event loop (async/await)
- Propagate async through all handlers

**Benefit**: No blocking (8.7ms → 0ms in event loop)

**Cost**:
- Framework immaturity risk
- Rewrite effort (2-3 weeks)
- Upstream drift (harder to sync)

**Conclusion**: Not worth it (8.7ms is imperceptible)

### 8.2 vs Message Passing

**Alternative**: Spawn async tasks, communicate via channels

**Pattern**:
```rust
// Spawn task
let (tx, rx) = oneshot::channel();
tokio::spawn(async move {
    let result = run_consensus(...).await;
    tx.send(result);
});

// Poll in event loop
match rx.try_recv() {
    Ok(result) => handle_result(result),
    Err(TryRecvError::Empty) => /* not ready */,
}
```

**Pro**: No blocking

**Con**:
- Complex state management
- Harder debugging
- More code churn

**Decision**: `block_on()` is simpler for infrequent operations

---

## 9. Measurement & Monitoring

### 9.1 Current Instrumentation

**Tracing** (via `tracing` crate):
```rust
use tracing::info;

let start = std::time::Instant::now();
let result = handle.block_on(run_consensus(...))?;
info!("Consensus completed in {:?}", start.elapsed());
```

**Output**: `~/.code/logs/codex-tui.log`

**Grep for performance**:
```bash
grep "Consensus completed" ~/.code/logs/codex-tui.log
# Consensus completed in 8.7ms
# Consensus completed in 712ms  # Cold start
```

### 9.2 Performance Regression Detection

**Benchmark test** (`tui/tests/mcp_consensus_benchmark.rs`):
```rust
#[tokio::test]
async fn bench_mcp_consensus_vs_subprocess() {
    let mcp_time = measure_mcp_consensus().await;
    let subprocess_time = measure_subprocess_consensus().await;

    assert!(mcp_time < subprocess_time / 3);  // Must be 3x faster
}
```

**CI Integration** (future):
- Run benchmark on every PR
- Alert if performance regresses >20%

---

## 10. Developer Guidelines

### 10.1 When to Use `block_on()`

**✅ Good Use Cases**:
- User-initiated commands (`/speckit.*`)
- Infrequent operations (<10/min)
- Operations that must complete before proceeding
- Operations with visible progress

**❌ Bad Use Cases**:
- Tight loops (event poll, render loop)
- Background tasks (use `tokio::spawn` instead)
- Nested async functions (deadlock risk)
- Time-critical operations (<10ms target)

### 10.2 Code Patterns

**Good**:
```rust
fn handle_user_command() {
    // User clicked button, OK to block briefly
    let result = tokio::runtime::Handle::current()
        .block_on(async_operation())?;

    display_result(result);
}
```

**Bad**:
```rust
fn render_loop() {
    loop {
        // BAD: Blocking in render loop
        let data = tokio::runtime::Handle::current()
            .block_on(fetch_data())?;

        render(data);
    }
}
```

**Better** (for render loop):
```rust
fn render_loop() {
    let (tx, rx) = mpsc::channel();
    tokio::spawn(async move {
        loop {
            let data = fetch_data().await;
            tx.send(data);
        }
    });

    loop {
        if let Ok(data) = rx.try_recv() {
            render(data);
        }
    }
}
```

---

## 11. Related Documentation

- `REVIEW.md`: Architecture analysis (lines 255-283, 387-417)
- `ARCHITECTURE-TASKS.md`: ARCH-002 (MCP fallback), ARCH-005 (MCP sharing)
- `codex-rs/tui/src/chatwidget/spec_kit/handler.rs`: Blocking implementations
- `codex-rs/tui/src/chatwidget/spec_kit/consensus.rs`: Async MCP calls
- `codex-rs/tui/tests/mcp_consensus_benchmark.rs`: Performance validation

---

## 12. Change History

| Version | Date | Changes | Author |
|---------|------|---------|--------|
| v1.0 | 2025-10-18 | Initial architecture documentation | theturtlecsz |

---

## Appendix: Profiling Tools

**Measure blocking duration**:
```rust
let start = std::time::Instant::now();
let result = handle.block_on(async_op())?;
println!("Blocked for {:?}", start.elapsed());
```

**Tokio console** (advanced):
```bash
# Install tokio-console
cargo install tokio-console

# Enable tokio tracing in Cargo.toml
tokio = { version = "1", features = ["full", "tracing"] }

# Run with console
RUSTFLAGS="--cfg tokio_unstable" cargo run
tokio-console
```

**Flamegraph** (CPU profiling):
```bash
cargo install flamegraph
cargo flamegraph --bin code
# Generates flamegraph.svg
```
