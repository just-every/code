# Test Suite Reset Plan for code-rs

## Executive Summary

This document outlines the plan to reset the code-rs test infrastructure by removing the legacy integration test suite and establishing a minimal, maintainable testing baseline. The primary goal is to reduce CI time and maintenance burden while ensuring `./build-fast.sh --workspace code` remains the sole required gate.

**Status (2025-10-05):** Phase 1 complete — legacy integration suites removed. Smoke scaffold landed in `code-rs/tui/tests/ui_smoke.rs` (commit 71046c588); expanding coverage is now the priority.

## Current State Analysis

### Test Infrastructure Overview

- **Total test files**: ~99 Rust test files across 13 test directories
- **Major test suites**:
  - `core/tests/`: 32 files (largest suite with ~18k LOC total)
    - Integration tests for CLI streaming, compact/resume/fork, prompt caching, OpenTelemetry, MCP client
    - Largest individual files: `client.rs` (1489 LOC), `otel.rs` (1196 LOC), `compact.rs` (1035 LOC)
  - `app-server/tests/`: 19 files
    - Message processor flows, auth, conversation management, fuzzy search
  - `exec/tests/`: 10 files
    - Event processor, sandbox, resume, output schema
  - `execpolicy/tests/`: 11 files
    - Command parsing tests for various shell commands
  - Smaller suites: `mcp-server` (7 files), `login` (4 files), `apply-patch` (3 files), `chatgpt` (3 files), etc.

### TUI Legacy Tests (✅ REMOVED)

**Status:** The `legacy_tests` feature flag and all associated `#[cfg(all(test, feature = "legacy_tests"))]` modules were deleted on 2025-10-05 (~11,232 LOC removed).

**Historical context:**
- 27 inline modules covered text formatting, markdown rendering, chat composer flows, pager overlay, and vt100 replay scenarios.
- The feature flag `legacy_tests = []` has been excised from `code-rs/tui/Cargo.toml`.
- Test-only helpers that existed solely for the legacy suite were also removed.

**Next steps:** Replace the legacy suite with modern smoke coverage (see `code-rs/tui/tests/ui_smoke.rs`).

### Test Helper Infrastructure

Three shared test support crates exist:
1. **`core_test_support`** (`core/tests/common/`): SSE fixture loading, default config setup, event waiting utilities
2. **`app_test_support`** (`app-server/tests/common/`): MCP process management, mock model server, SSE response helpers
3. **`mcp_test_support`** (`mcp-server/tests/common/`): Similar to app_test_support for MCP server tests

All three are workspace dependencies (defined in root `Cargo.toml`).

### CI Gating Status

- **Current gate**: `./build-fast.sh --workspace code` (confirmed in `.github/workflows/`)
  - Used in `issue-triage.yml` line 188
  - Used in `upstream-merge.yml` lines 351, 676-712
  - Used in `issue-code.yml` lines 170, 185, 251
- **No `cargo test` invocations** found in any workflow file
- Build-only verification is **already enforced** as the sole requirement

---

## Removal Plan

### Phase 1: Delete Legacy Integration Tests ✅ COMPLETED

**Status:** All legacy integration test directories removed as of 2025-10-05.

**Completed Deletions:**

#### Core Tests (`code-rs/core/tests/`) ✅ REMOVED
- **Deleted entire directory**: `code-rs/core/tests/`
- Files removed (32 total):
  - `all.rs`, `api_surface.rs`, `chat_completions_payload.rs`, `chat_completions_sse.rs`, `mcp_manager.rs`
  - `suite/`: All 20 test modules including `client.rs`, `compact.rs`, `compact_resume_fork.rs`, `cli_stream.rs`, `exec.rs`, `exec_stream_events.rs`, `fork_conversation.rs`, `json_result.rs`, `live_cli.rs`, `model_overrides.rs`, `otel.rs`, `prompt_caching.rs`, `review.rs`, `rmcp_client.rs`, `rollout_list_find.rs`, `rollout_resume.rs`, `seatbelt.rs`, `stream_error_allows_next_turn.rs`, `stream_no_completed.rs`, `stream_order.rs`, `user_notification.rs`, `abort_tasks.rs`
  - `common/`: Test fixtures and helpers (`lib.rs`, `responses.rs`, `test_codex.rs`, `test_codex_exec.rs`)

#### App Server Tests (`code-rs/app-server/tests/`) ✅ REMOVED
- **Deleted entire directory**: `code-rs/app-server/tests/`
- Files removed (19 total):
  - `all.rs`
  - `suite/`: `archive_conversation.rs`, `auth.rs`, `code_message_processor_flow.rs`, `config.rs`, `create_conversation.rs`, `fuzzy_file_search.rs`, `interrupt.rs`, `list_resume.rs`, `login.rs`, `send_message.rs`, `set_default_model.rs`, `user_agent.rs`, `user_info.rs`
  - `common/`: `lib.rs`, `mcp_process.rs`, `mock_model_server.rs`, `responses.rs`

#### Exec Tests (`code-rs/exec/tests/`) ✅ REMOVED
- **Deleted entire directory**: `code-rs/exec/tests/`
- Files removed (10 total):
  - `all.rs`, `event_processor_with_json_output.rs`
  - `suite/`: `apply_patch.rs`, `auth_env.rs`, `common.rs`, `mod.rs`, `output_schema.rs`, `resume.rs`, `sandbox.rs`, `server_error_exit.rs`

#### MCP Server Tests (`code-rs/mcp-server/tests/`) ✅ REMOVED
- **Deleted entire directory**: `code-rs/mcp-server/tests/`
- Files removed (7 total):
  - `all.rs`
  - `suite/`: `codex_tool.rs`, `mod.rs`
  - `common/`: `lib.rs`, `mcp_process.rs`, `mock_model_server.rs`, `responses.rs`

#### Login Tests (`code-rs/login/tests/`) ✅ REMOVED
- **Deleted entire directory**: `code-rs/login/tests/`
- Files removed (4 total): `all.rs`, `suite/device_code_login.rs`, `suite/login_server_e2e.rs`, `suite/mod.rs`

#### ChatGPT Tests (`code-rs/chatgpt/tests/`) ✅ REMOVED
- **Deleted entire directory**: `code-rs/chatgpt/tests/`
- Files removed (3 total): `all.rs`, `suite/apply_command_e2e.rs`, `suite/mod.rs`

#### Apply Patch Tests (`code-rs/apply-patch/tests/`) ✅ REMOVED
- **Deleted entire directory**: `code-rs/apply-patch/tests/`
- Files removed (3 total): `all.rs`, `suite/cli.rs`, `suite/mod.rs`

#### CLI Tests (`code-rs/cli/tests/`) ✅ REMOVED
- **Deleted entire directory**: `code-rs/cli/tests/`
- Files removed (2 total): `mcp_add_remove.rs`, `mcp_list.rs`

#### ExecPolicy Tests (`code-rs/execpolicy/tests/`) ✅ REMOVED
- **Deleted entire directory**: `code-rs/execpolicy/tests/`
- All 11 command parsing tests removed (can be recreated as lightweight inline tests if needed)

#### Retained Test Directories
- **`mcp-types/tests/`** (4 files): ✅ **KEPT** - Protocol serialization tests
- **`linux-sandbox/tests/`** (3 files): ✅ **KEPT** - Security-critical landlock tests
- **`cloud-tasks/tests/`** (1 file: `env_filter.rs`): ✅ **KEPT** - Minimal, focused test

### Phase 2: Remove TUI Legacy Tests ✅ COMPLETED

**Status:** All TUI legacy test infrastructure removed.

**Completed Actions:**
- ✅ Removed `legacy_tests` feature from `code-rs/tui/Cargo.toml`
- ✅ Deleted all 27 `#[cfg(all(test, feature = "legacy_tests"))]` test modules from `tui/src/`
- ✅ Removed test-only helper methods (e.g., `ChatWidget::test_for_request`)
- ✅ Removed `vt100-tests` feature flag

### Phase 3: Clean Up Test Dependencies ✅ COMPLETED

**Status:** Test support crates and dev-dependencies cleaned.

**Completed Actions:**
- ✅ Removed workspace test support dependencies (`app_test_support`, `core_test_support`, `mcp_test_support`)
- ✅ Pruned `[dev-dependencies]` in `core/`, `app-server/`, `exec/`, `tui/` crates
- ✅ Build verified with `./build-fast.sh --workspace code`

---

## Minimal Tests Retained ✅

### Current Baseline (Phase 1 Complete)

1. **Protocol/Types Tests** (`mcp-types/tests/`) ✅ KEPT
   - JSON-RPC serialization/deserialization correctness
   - Progress notification format
   - Initialize handshake format
   - **Rationale**: Catches breaking API changes early

2. **Security Tests** (`linux-sandbox/tests/`) ✅ KEPT
   - Landlock sandboxing validation
   - **Rationale**: Security-critical, platform-specific

3. **Utility Tests** (`cloud-tasks/tests/env_filter.rs`) ✅ KEPT
   - Environment variable filtering logic
   - **Rationale**: Low-cost, high-value edge case coverage

4. **Doc Tests** (Existing `#[doc]` examples) ✅ KEPT
   - Already run by `cargo test` with minimal cost
   - Inline examples in library code
   - **Rationale**: Ensures public API examples compile

### Total Retained Test LOC
- **~400 lines** today; expect <1000 LOC after smoke tests land (down from ~18k+ integration tests) ✅

---

## Next Phase: Lightweight Smoke Tests (IN PROGRESS)

### Recommended Additions

1. **TUI Smoke Tests** (`tui/tests/ui_smoke.rs`) — **IN PROGRESS**
   - Exercise ChatWidget event handling, approval flows, and tool calls
   - Extend current scaffold to cover executor streaming and MCP tool invocations
   - Targeted assertions on emitted `AppEvent`s; avoid vt100 render snapshots for now

2. **Executor Integration** — **PLANNED**
   - Verify minimal command execution flow without bringing back legacy harness
   - Focus on `exec` crate policy enforcement and stdout/stderr ordering

3. **MCP Client Smoke** — **PLANNED**
   - Exercise handshake + single tool invocation using lightweight mocks
   - Ensure re-export wrappers stay in sync with upstream schema

**Key Principles:**
- Favor helper functions over resurrecting deleted support crates
- Keep total smoke coverage under 1,000 LOC combined
- Tests must finish in <5 seconds locally to preserve fast feedback

---

## Required Helper Refactors

### No Major Refactors Needed

Since we're **deleting** rather than migrating tests, minimal code changes are required:

1. **Remove workspace test support dependencies** (see Phase 3 above)
2. **Remove test-only pub methods** in production code:
   - Search for `#[cfg(any(test, feature = "legacy_tests"))]` and remove those methods
   - Example: `tui/src/chatwidget.rs` line 911 `pub(crate) fn test_for_request`
3. **Clean up unused imports** flagged by clippy after deletion
4. **Verify no CI breakage** from removed test crates (unlikely since tests aren't run in CI)

### Optional: Consolidate Remaining Tests

For `execpolicy` and other small suites, consider:
- Moving tests from `tests/` directory to inline `#[cfg(test)] mod tests` in `src/`
- Reduces filesystem clutter
- Allows direct access to private functions
- Example pattern:
  ```rust
  // src/lib.rs or src/module.rs
  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn test_parsing_logic() { /* ... */ }
  }
  ```

---

## CI Gating Strategy

### Current Behavior (No Change Needed)
- **Primary gate**: `./build-fast.sh --workspace code` (build-only)
- **No test execution** in any workflow
- Workflows already enforce:
  - Build success (compile check)
  - Clippy lints pass
  - Formatting via `rustfmt` (if configured)

### Post-Reset Behavior
- **Unchanged**: `./build-fast.sh --workspace code` remains sole gate
- Minimal test suites (mcp-types, linux-sandbox, etc.) can be run **locally** on-demand:
  ```bash
  cargo test -p mcp-types
  cargo test -p code-linux-sandbox
  cargo test -p code-cloud-tasks
  ```
- No automatic test execution in CI prevents:
  - Slow PR feedback
  - Flaky test maintenance burden
  - Need for test infrastructure (mock servers, fixtures, etc.)

### Future Considerations
If integration tests are re-introduced later:
- Gate behind explicit workflow dispatch or nightly schedule
- Use separate workflow file (e.g., `.github/workflows/integration-tests.yml`)
- Mark as non-blocking advisory check

---

## Migration Checklist ✅ COMPLETED

### Pre-Deletion Validation ✅
- [x] Archive current test suite state to a branch (archived in git history)
- [x] Document any critical test scenarios that should be preserved as manual validation steps
- [x] Verify no production code has hard dependencies on test crates

### Deletion Steps ✅
1. [x] Delete test directories (Phase 1):
   - [x] `code-rs/core/tests/`
   - [x] `code-rs/app-server/tests/`
   - [x] `code-rs/exec/tests/`
   - [x] `code-rs/mcp-server/tests/`
   - [x] `code-rs/login/tests/`
   - [x] `code-rs/chatgpt/tests/`
   - [x] `code-rs/apply-patch/tests/`
   - [x] `code-rs/cli/tests/`
   - [x] `code-rs/execpolicy/tests/`
2. [x] Remove TUI legacy tests (Phase 2):
   - [x] Delete `legacy_tests` feature from `tui/Cargo.toml`
   - [x] Remove all 27 gated test modules in `tui/src/`
   - [x] Remove `chatwidget_stream_tests.rs`
   - [x] Remove `vt100-tests` feature
3. [x] Clean dependencies (Phase 3):
   - [x] Remove `app_test_support`, `core_test_support`, `mcp_test_support` from workspace `Cargo.toml`
   - [x] Prune `[dev-dependencies]` in affected crates
4. [x] Remove test-only code:
   - [x] Search and remove `#[cfg(any(test, feature = "legacy_tests"))]` methods
   - [x] Clean up unused imports (run `cargo clippy --fix`)

### Post-Deletion Validation ✅
- [x] Run `./build-fast.sh --workspace code` successfully
- [x] Run `cargo clippy --workspace` with no errors
- [x] Run remaining tests locally: `cargo test -p mcp-types -p code-linux-sandbox -p code-cloud-tasks`
- [x] Verify no warnings about missing workspace members
- [x] Confirm CI workflows still pass

### Next Steps (Phase 2)
- [x] Seed TUI smoke scaffold (`tui/tests/ui_smoke.rs`)
- [x] Create test helper module (`chatwidget::smoke_helpers`)
- [ ] Expand TUI smoke tests to cover executor streaming + MCP interactions
- [ ] Draft executor smoke coverage plan (command lifecycle, approvals)
- [ ] Draft MCP smoke coverage plan (handshake + tool call)
- [ ] Document manual validation procedures for critical paths
- [ ] Port 9 critical tests from codex-rs (see `docs/migration/legacy-tests-retirement.md`)

---

## Test Scaffolding Patterns (2025-10-06)

### TUI Test Helpers

**Location:** `code-rs/tui/src/chatwidget/smoke_helpers.rs`

This module provides test infrastructure for writing ChatWidget smoke tests without requiring terminal I/O dependencies.

#### ChatWidgetHarness

A test harness that constructs a fully-functional ChatWidget instance suitable for testing:

```rust
use code_tui::chatwidget::smoke_helpers::ChatWidgetHarness;
use code_core::protocol::Event;

#[test]
fn test_basic_message_flow() {
    let mut harness = ChatWidgetHarness::new();

    // Send events to the widget
    harness.handle_event(Event::TextDelta { ... });

    // Check emitted app events
    let events = harness.drain_events();
    assert_has_insert_history(&events);
}
```

**Key features:**
- Creates a default test configuration
- Sets up event channels (AppEvent sender/receiver)
- Provides a shared tokio runtime for async operations
- Exposes event handling and assertion helpers

**Available helpers:**
- `ChatWidgetHarness::new()` - Construct harness with default config
- `harness.handle_event(event)` - Send a protocol event to the widget
- `harness.drain_events()` - Collect all emitted AppEvents
- `harness.chat()` - Access the underlying ChatWidget for inspection

#### Assertion Helpers

Pre-built assertions for common test patterns:

```rust
// Assert history insertion occurred
assert_has_insert_history(&events);

// Assert background event with specific content
assert_has_background_event_containing(&events, "Error:");

// Assert terminal output chunk contains text
assert_has_terminal_chunk_containing(&events, "$ ls");

// Assert CodexEvent was emitted
assert_has_codex_event(&events);

// Assert no events were emitted
assert_no_events(&events);
```

These helpers reduce boilerplate and make test failures more readable by showing the full event list on assertion failure.

### Usage Example

From `code-rs/tui/tests/ui_smoke.rs`:

```rust
#[test]
fn test_approval_flow() {
    let mut harness = ChatWidgetHarness::new();

    // Simulate approval request
    harness.handle_event(Event::BashRequest { command: "rm -rf /" });

    // Verify approval modal was shown
    let events = harness.drain_events();
    assert_has_background_event_containing(&events, "Approval required");

    // Approve the command
    harness.handle_event(Event::ApprovalResponse { approved: true });

    // Verify execution started
    let events = harness.drain_events();
    assert_has_terminal_chunk_containing(&events, "rm -rf /");
}
```

### Best Practices

1. **Use harness for integration-style tests** - Test widget behavior through public event interfaces
2. **Keep assertions focused** - One logical assertion per helper call
3. **Drain events between steps** - Clear event queue to isolate test phases
4. **Avoid testing internal state** - Focus on observable outputs (emitted events, rendered output)
5. **Use descriptive test names** - `test_approval_flow_rejects_dangerous_commands` over `test_approval`

### Extending the Harness

To add new test helpers:

1. Add new assertion functions to `smoke_helpers.rs`:
   ```rust
   pub fn assert_has_error_message(events: &[AppEvent], error: &str) {
       let found = events.iter().any(|event| {
           matches!(event, AppEvent::Error(msg) if msg.contains(error))
       });
       assert!(found, "expected error '{error}', got: {events:#?}");
   }
   ```

2. Add builder methods to `ChatWidgetHarness` for custom configurations:
   ```rust
   impl ChatWidgetHarness {
       pub fn with_config(cfg: Config) -> Self {
           // Custom initialization
       }
   }
   ```

### Future Test Infrastructure

**Planned additions:**
- `AppHarness` - Full app-level test harness (includes bottom pane, status bar)
- `ExecutorHarness` - Test harness for executor flows (command lifecycle, approvals)
- `MCPHarness` - Test harness for MCP server interactions (handshake, tool calls)
- VT100 replay tests - Capture/replay terminal sequences for rendering regression tests

Document future automation plans inline here as they evolve; remove stale pointers when work completes.

---

## Rollback Plan

If issues arise post-deletion:
1. Revert to archived branch: `git checkout archive/legacy-tests-2025-10`
2. Cherry-pick critical fixes onto legacy test branch
3. Evaluate minimal subset of tests to restore

---

## Benefits Summary

1. **Reduced Maintenance**: No need to update/fix tests when refactoring internal APIs
2. **Faster Local Builds**: No test compilation overhead for developers
3. **CI Simplicity**: Single build gate reduces complexity and failure modes
4. **Codebase Clarity**: Less test scaffolding obscuring production code
5. **Lower Barrier to Entry**: New contributors only need to ensure build passes

---

## Risks & Mitigations

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Loss of regression coverage | Medium | Archive tests; rely on manual validation for critical paths; re-introduce targeted tests if regressions occur |
| Breaking changes undetected | Medium | Build gate + clippy catches most issues; user reports catch integration issues |
| Reduced confidence in refactoring | Low | Strong type system + compiler checks provide safety net |
| Difficulty debugging production issues | Low | Logging/telemetry more valuable than stale tests; can add focused repro tests on-demand |

---

## References

- Current build script: `./build-fast.sh` (lines 465, 633-636 confirm no test execution)
- Workflow files:
  - `.github/workflows/issue-triage.yml` line 188
  - `.github/workflows/upstream-merge.yml` lines 351, 676-712, 733
  - `.github/workflows/issue-code.yml` lines 170, 185, 251
- Feature flags: `code-rs/tui/Cargo.toml` lines 16, 20
- Test helpers: `code-rs/Cargo.toml` lines 48, 76, 78

---

**Document Version**: 1.0
**Last Updated**: 2025-10-05
**Status**: Ready for implementation
