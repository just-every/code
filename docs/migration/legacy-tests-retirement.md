# Legacy Tests Retirement Playbook

## Executive Summary

This playbook provides an actionable plan to safely retire legacy upstream tests from `codex-rs/` after confirming equivalent or superior coverage exists in the `code-rs/` fork. The goal is to reduce maintenance burden while preserving test quality and preventing regressions.

**Current Status (2025-10-05):**
- Total test files in `codex-rs/`: 106
- Total test files in `code-rs/`: 99
- Legacy-only tests identified: 9 files
- Fork-only tests (new coverage): 3 files

---

## Test Coverage Analysis

### Upstream-Only Tests (Candidates for Retirement)

These tests exist in `codex-rs/` but not in `code-rs/`:

#### Core Package Tests (`codex-rs/core/tests/suite/`)

| Test File | Coverage Area | Code-rs Replacement | Gap Status |
|-----------|---------------|---------------------|------------|
| `model_tools.rs` | Model tool selection, SSE tool identifiers | ✅ `code-rs/core/tests/tool_selection.rs` | Coverage restored – ready to delete upstream |
| `read_file.rs` | File reading tool functionality | ❌ No replacement | **COVERAGE GAP** - High |
| `tool_harness.rs` | Tool execution framework | ❌ No replacement | **COVERAGE GAP** - High |
| `tools.rs` | General tool integration tests | ❌ No replacement | **COVERAGE GAP** - High |
| `unified_exec.rs` | Unified executor stdin/timeout tests | ❌ No replacement | **COVERAGE GAP** - Critical |
| `view_image.rs` | Image viewing tool tests | ❌ No replacement | **COVERAGE GAP** - Medium |

#### TUI Package Tests (`codex-rs/tui/tests/suite/`)

| Test File | Coverage Area | Code-rs Replacement | Gap Status |
|-----------|---------------|---------------------|------------|
| `status_indicator.rs` | Status widget rendering | ❌ No focused assertions yet | **COVERAGE GAP** - Medium |
| `vt100_history.rs` | VT100 history rendering, wrapping, ANSI | ❌ vt100 harness not ported | **COVERAGE GAP** - High |
| `vt100_live_commit.rs` | VT100 live commit rendering | ❌ vt100 harness not ported | **COVERAGE GAP** - Medium |

### Fork-Only Tests (Current Coverage)

The fork currently adds a single smoke suite:

| Test File | Package | Coverage Area | Notes |
|-----------|---------|---------------|-------|
| `tui/tests/ui_smoke.rs` | tui | CLI flag defaults, streaming events, approvals | Requires `chatwidget::smoke_helpers`; fails without `#[cfg(test)]` exports |

### Identical Tests (Safe to Keep in code-rs)

The following test files exist in both trees with equivalent coverage:
- All `app-server` tests (14 files) ✅
- All `apply-patch` tests (2 files) ✅
- All `chatgpt` tests (2 files) ✅
- Most `core` tests (18 of 27 files) ✅
- All `exec` tests (7 files) ✅
- All `execpolicy` tests (10 files) ✅
- All `linux-sandbox` tests (2 files) ✅
- All `login` tests (3 files) ✅
- All `mcp-server` tests (2 files) ✅
- All `mcp-types` tests (3 files) ✅

---

## Deletion Prerequisites and Blockers

### Critical Gaps Requiring Action

#### 1. Unified Executor Tests (HIGH PRIORITY)
**Files:** `codex-rs/core/tests/suite/unified_exec.rs`

**Coverage:**
- `unified_exec_reuses_session_via_stdin()` - Tests stdin session reuse
- `unified_exec_timeout_and_followup_poll()` - Tests timeout handling and polling

**Blockers:**
- ❌ No equivalent tests in `code-rs/core/tests/`
- ❌ Fork uses legacy executor architecture (see `subsystem-migration-status.md`)

**Prerequisites for Deletion:**
1. Port unified executor stdin tests to `code-rs/core/tests/suite/exec.rs`
2. Port timeout/polling tests to `code-rs/core/tests/suite/exec_stream_events.rs`
3. Verify coverage with: `cargo test -p code-core unified_exec`
4. Run smoke test: Execute multi-turn conversation with stdin input

**Owner:** Core runtime pod
**Estimated Duration:** 2-3 days

---

#### 2. Model & Tool Tests (HIGH PRIORITY)
**Files:**
- `codex-rs/core/tests/suite/model_tools.rs`
- `codex-rs/core/tests/suite/tool_harness.rs`
- `codex-rs/core/tests/suite/tools.rs`
- `codex-rs/core/tests/suite/read_file.rs`
- `codex-rs/core/tests/suite/view_image.rs`

**Coverage:**
- Model-specific tool selection logic
- Tool execution framework and harness
- File reading and image viewing tools
- SSE tool identifier parsing

**Blockers:**
- ❌ No tool-specific tests in `code-rs/core/tests/`
- ⚠️  Fork uses different tool router (hybrid approach per migration status)

**Prerequisites for Deletion:**
1. Create `code-rs/core/tests/tool_selection.rs` for model tool tests
2. Create `code-rs/core/tests/suite/tool_execution.rs` for harness/tools tests
3. Create `code-rs/core/tests/suite/file_tools.rs` for read_file/view_image tests
4. Verify coverage with: `cargo test -p code-core tool`
5. Run smoke tests:
   - Request file read via tool use
   - Request image view via tool use
   - Verify model-specific tool selection

**Owner:** Core runtime pod
**Estimated Duration:** 3-5 days

---

#### 3. TUI Rendering Tests (MEDIUM PRIORITY)
**Files:**
- `codex-rs/tui/tests/suite/status_indicator.rs`
- `codex-rs/tui/tests/suite/vt100_history.rs`
- `codex-rs/tui/tests/suite/vt100_live_commit.rs`

**Coverage:**
- Status widget rendering and updates
- VT100 terminal history rendering
- Word wrapping, ANSI sequences, emoji/CJK handling
- Cursor restoration
- Live commit display

**Blockers:**
- ⚠️ Existing smoke test depends on private modules gated behind `#[cfg(test)]`
- ⚠️ Fork uses legacy TUI layout (fork-primary per migration status)

**Prerequisites for Deletion:**
1. Create `code-rs/tui/tests/` directory structure
2. Port `vt100_history.rs` tests (7+ test cases for wrapping/ANSI)
3. Port `vt100_live_commit.rs` tests
4. Port `status_indicator.rs` tests
5. Add test infrastructure: `code-rs/tui/tests/all.rs`, `suite/mod.rs`
6. Verify coverage with: `cargo test -p code-tui`
7. Run visual regression tests with captured VT100 output

**Owner:** TUI pod
**Estimated Duration:** 4-6 days

---

## Smoke and Regression Test Matrix

Before deleting any legacy test file, run these smoke tests to ensure coverage:

### Core Runtime Smoke Tests
```bash
# Unified executor
cargo test -p code-core exec -- --test-threads=1
cargo test -p code-core stream -- --test-threads=1

# Tool execution
cargo test -p code-core -- tool --test-threads=1
cargo test -p code-core -- read_file view_image --test-threads=1

# Model integration
cargo test -p code-core -- model --test-threads=1
```

**Expected:** All tests pass with no panics or timeouts

### TUI Smoke Tests
```bash
# Once TUI tests are created
cargo test -p code-tui -- --test-threads=1

# Manual regression test
./scripts/test-tui-rendering.sh  # Create this script
```

**Expected:**
- VT100 output matches golden files
- No rendering glitches with emoji/CJK/ANSI
- Status indicators update correctly

### End-to-End Regression Tests
```bash
# Full conversation flow
./build-fast.sh --workspace code
cargo run -p code-cli -- chat "read the file README.md and summarize it"

# Multi-turn with tools
cargo run -p code-cli -- chat "list files, then read the largest one"

# Image tools (if applicable)
cargo run -p code-cli -- chat "show me the image at docs/logo.png"
```

**Expected:**
- Tools execute successfully
- Streaming output is ordered correctly
- No executor timeouts or hangs

---

## Deletion Checklist by Package

### Phase 1: Core Runtime Tests (2025-10-20 → 2025-10-31)
**Owner:** Core runtime pod

- [ ] **BLOCKED** - Create tool coverage tests in `code-rs/core/tests/suite/`
  - [x] Port `model_tools.rs` → `tool_selection.rs`
  - [ ] Port `tool_harness.rs` + `tools.rs` → `tool_execution.rs`
  - [ ] Port `read_file.rs` + `view_image.rs` → `file_tools.rs`
  - [ ] Port `unified_exec.rs` → enhance `exec.rs` and `exec_stream_events.rs`

- [ ] **VERIFY** - Run smoke tests (listed above)
  - [ ] All cargo tests pass
  - [ ] Manual tool execution works
  - [ ] No regressions in CI/CD

- [ ] **DELETE** - Remove upstream tests once verified
  - [ ] Delete `codex-rs/core/tests/suite/model_tools.rs`
  - [ ] Delete `codex-rs/core/tests/suite/tool_harness.rs`
  - [ ] Delete `codex-rs/core/tests/suite/tools.rs`
  - [ ] Delete `codex-rs/core/tests/suite/read_file.rs`
  - [ ] Delete `codex-rs/core/tests/suite/view_image.rs`
  - [ ] Delete `codex-rs/core/tests/suite/unified_exec.rs`
  - [ ] Update `codex-rs/core/tests/suite/mod.rs` to remove deleted modules

- [ ] **DOCUMENT** - Update this playbook
  - [ ] Mark tests as retired in this document
  - [ ] Update `subsystem-migration-status.md` with test coverage status
  - [ ] Record deletion in PR description

**Estimated Duration:** 2 weeks
**Risk Level:** HIGH (critical path functionality)

---

### Phase 2: TUI Rendering Tests (2025-11-01 → 2025-11-15)
**Owner:** TUI pod

- [ ] **BLOCKED** - Create TUI test infrastructure in `code-rs/tui/`
  - [ ] Create `tests/all.rs` entry point
  - [ ] Create `tests/suite/mod.rs` module file
  - [ ] Create `tests/common/` for shared fixtures

- [ ] **PORT** - Port upstream TUI tests
  - [ ] Port `vt100_history.rs` (7+ test cases)
  - [ ] Port `vt100_live_commit.rs`
  - [ ] Port `status_indicator.rs`
  - [ ] Add golden file fixtures for VT100 output

- [ ] **VERIFY** - Run smoke tests
  - [ ] `cargo test -p code-tui` passes
  - [ ] Visual regression: VT100 output matches golden files
  - [ ] Manual TUI test: emoji, CJK, ANSI codes render correctly

- [ ] **DELETE** - Remove upstream tests once verified
  - [ ] Delete `codex-rs/tui/tests/suite/vt100_history.rs`
  - [ ] Delete `codex-rs/tui/tests/suite/vt100_live_commit.rs`
  - [ ] Delete `codex-rs/tui/tests/suite/status_indicator.rs`
  - [ ] Update `codex-rs/tui/tests/suite/mod.rs`

- [ ] **DOCUMENT** - Update documentation
  - [ ] Mark tests as retired
  - [ ] Update `tui-chatwidget-refactor.md` with test coverage

**Estimated Duration:** 2 weeks
**Risk Level:** MEDIUM (UX-critical but isolated)

---

### Phase 3: Final Cleanup (2025-11-16 → 2025-11-30)
**Owner:** Repo maintainers

- [ ] **AUDIT** - Verify all legacy tests are retired or ported
  - [ ] Run coverage comparison script: `./scripts/compare-test-coverage.sh`
  - [ ] Confirm no orphaned test files in `codex-rs/`
  - [ ] Verify `code-rs/` has equal or better coverage

- [ ] **OPTIMIZE** - Clean up test infrastructure
  - [ ] Remove unused `codex-rs/*/tests/common/` if fully duplicated
  - [ ] Consolidate test fixtures between packages
  - [ ] Remove deprecated test utilities

- [ ] **DELETE** - Remove entire `codex-rs/` test directories (if appropriate)
  - [ ] Evaluate if `codex-rs/` can be archived entirely
  - [ ] If yes, move to `archive/codex-rs/` or remove from repo

- [ ] **DOCUMENT** - Final documentation update
  - [ ] Mark playbook as COMPLETE
  - [ ] Update `code-dead-code-sweeps.md` with test deletion outcomes
  - [ ] Archive this playbook to `docs/archive/`

**Estimated Duration:** 2 weeks
**Risk Level:** LOW (cleanup phase)

---

## Sequencing Timeline

```
2025-10-20  Phase 1 Start: Core Runtime Tests
│           ├─ Create tool coverage tests (3-5 days)
│           ├─ Port unified executor tests (2-3 days)
│           ├─ Run smoke tests (1 day)
│           └─ Delete upstream tests (1 day)
2025-10-31  Phase 1 Complete

2025-11-01  Phase 2 Start: TUI Rendering Tests
│           ├─ Create TUI test infrastructure (2 days)
│           ├─ Port VT100 rendering tests (3-4 days)
│           ├─ Port status indicator tests (2 days)
│           ├─ Run visual regression tests (2 days)
│           └─ Delete upstream tests (1 day)
2025-11-15  Phase 2 Complete

2025-11-16  Phase 3 Start: Final Cleanup
│           ├─ Audit coverage (3 days)
│           ├─ Optimize test infrastructure (4 days)
│           ├─ Remove orphaned tests (2 days)
│           └─ Final documentation (2 days)
2025-11-30  Phase 3 Complete - All Legacy Tests Retired
```

**Total Duration:** 6 weeks
**Critical Path:** Phase 1 (Core Runtime Tests) blocks Phase 2 and 3

---

## Safety Checklist

Before deleting ANY legacy test file, confirm:

- [ ] ✅ Replacement test exists in `code-rs/` OR coverage gap is documented and accepted
- [ ] ✅ Replacement test covers all scenarios from legacy test (verified line-by-line)
- [ ] ✅ `cargo test -p <package>` passes with replacement tests
- [ ] ✅ Smoke tests pass (see matrix above)
- [ ] ✅ CI/CD pipeline shows no regressions
- [ ] ✅ Manual regression test completed for critical paths
- [ ] ✅ Deletion is recorded in PR description and this playbook
- [ ] ✅ Team reviewed and approved deletion (require 2+ approvals for high-risk tests)

---

## Risk Mitigation

### High-Risk Deletions (Require Extra Scrutiny)

1. **Unified executor tests** - Critical for multi-turn conversations
   - Mitigation: Port first, verify extensively, delete last
   - Rollback plan: Restore from git history if issues found

2. **Tool execution tests** - Core functionality
   - Mitigation: Create comprehensive replacement suite before deletion
   - Rollback plan: Re-enable upstream tests via feature flag

3. **VT100 rendering tests** - UX-critical
   - Mitigation: Use golden file regression testing
   - Rollback plan: Keep upstream tests until visual parity confirmed

### Rollback Procedure

If a deletion causes regressions:

1. **Immediate:** Revert the deletion commit
2. **Short-term:** Re-enable affected upstream test in `codex-rs/`
3. **Long-term:** Investigate gap, enhance replacement test, retry deletion

---

## Metrics and Success Criteria

Track these metrics throughout retirement:

| Metric | Baseline (2025-10-05) | Target (2025-11-30) |
|--------|----------------------|---------------------|
| Legacy test files | 9 files | 0 files ✅ |
| Code-rs test coverage | ~85% (estimated) | ≥90% |
| Test execution time | TBD | ≤ baseline + 10% |
| CI/CD test failures | TBD | No increase |
| Regression bugs filed | 0 | 0 |

**Success Criteria:**
- All 9 legacy test files deleted or coverage gaps explicitly documented
- No increase in production bugs related to deleted test areas
- `code-rs/` has equal or better test coverage than `codex-rs/`
- Team confidence in test suite quality remains high

---

## References

- Test comparison script: `./scripts/compare-test-coverage.sh` (create this)
- Coverage gaps: Tracked in GitHub issues with label `test-coverage-gap`
- Related docs:
  - `docs/subsystem-migration-status.md` - Subsystem adoption status
  - `docs/code-dead-code-sweeps.md` - Dead code cleanup schedule
  - `docs/tui-chatwidget-refactor.md` - TUI refactoring plan

---

## Appendix: Test File Details

### Legacy Test Inventory

#### `codex-rs/core/tests/suite/model_tools.rs`
- **Functions tested:** 4 tests
- **Key scenarios:** SSE tool completion, model-specific tool selection
- **Dependencies:** Mock SSE server, tool identifier parsing
- **Replacement status:** ✅ Ported as `code-rs/core/tests/tool_selection.rs` (2025-11-16)

#### `codex-rs/core/tests/suite/unified_exec.rs`
- **Functions tested:** 2 tests
- **Key scenarios:** stdin session reuse, timeout and followup poll
- **Dependencies:** Unified executor module (not in fork)
- **Replacement status:** ❌ Blocked by executor migration

#### `codex-rs/tui/tests/suite/vt100_history.rs`
- **Functions tested:** 7+ tests
- **Key scenarios:**
  - Basic insertion without wrap
  - Long token wrapping
  - Emoji and CJK character handling
  - ANSI escape sequence rendering
  - Cursor restoration
  - Word wrap without mid-word split
  - Em-dash and space word wrap
- **Dependencies:** VT100 renderer, word wrap logic
- **Replacement status:** ❌ No TUI tests in fork

---

**Last Updated:** 2025-11-16
**Next Review:** 2025-11-23 (Phase 1 status check)
**Playbook Status:** 🟡 IN PROGRESS - Awaiting test porting
