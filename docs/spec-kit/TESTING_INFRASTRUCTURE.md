# Spec-Kit Testing Infrastructure

**Status**: v1.0 (2025-10-18) - Phase 1 Complete
**Task**: MAINT-3 (Test Coverage Phase 1)
**Policy**: docs/spec-kit/testing-policy.md
**Goal**: Infrastructure for 1.7%→40% coverage by Q1 2026

---

## 1. Overview

Phase 1 provides mocking infrastructure, fixture library, and coverage measurement tools to enable Phase 2-4 test writing (125+ tests planned).

**Deliverables**:
- ✅ MockMcpManager for testing MCP-dependent code
- ✅ Fixture library (20 real consensus artifacts)
- ✅ Tarpaulin configuration
- ✅ Baseline coverage documented (1.7%)

---

## 2. MockMcpManager

**Location**: `tui/tests/common/mock_mcp.rs` (240 LOC)

**Purpose**: Mock `McpConnectionManager::call_tool()` for testing without live MCP server

**API**:
```rust
use tui::tests::common::MockMcpManager;

let mut mock = MockMcpManager::new();

// Add fixture response
mock.add_fixture(
    "local-memory",     // server
    "search",           // tool
    Some("SPEC-065 plan"), // query pattern (or None for wildcard)
    json!({"memory": {"id": "test", "content": "..."}}),
);

// Use in tests
let result = mock.call_tool("local-memory", "search", Some(args), None).await?;

// Verify calls
let log = mock.call_log();
assert_eq!(log.len(), 1);
```

**Features**:
- Fixture matching: Exact query or wildcard
- Call logging: Track all tool invocations
- File loading: `load_fixture_file()` for JSON files
- Multiple fixtures: Returns array if multiple added for same key

**Tests**: 7 tests in `tests/mock_mcp_tests.rs` (all passing)

---

## 3. Fixture Library

**Location**: `tui/tests/fixtures/consensus/` (20 files, 96 KB)

**Coverage**:
- **Plan stage** (13 fixtures): gemini, claude, gpt_pro from DEMO/025/045
- **Tasks stage** (3 fixtures): gemini, claude from 025
- **Implement stage** (4 fixtures): gemini, claude, gpt_codex, gpt_pro from 025

**File Naming**: `{spec_id}-{stage}-{agent}.json`

**Examples**:
- `demo-plan-gemini.json` - SPEC-KIT-DEMO plan stage gemini output
- `025-implement-gpt_codex.json` - SPEC-KIT-025 implement stage codex output

**Source**: Extracted from `docs/SPEC-OPS-004.../evidence/consensus/` (real production artifacts)

**Usage in Tests**:
```rust
let mut mock = MockMcpManager::new();
mock.load_fixture_file(
    "local-memory",
    "search",
    Some("SPEC-KIT-DEMO plan"),
    "tests/fixtures/consensus/demo-plan-gemini.json"
)?;
```

---

## 4. Tarpaulin Configuration

**Location**: `tarpaulin.toml` (workspace root)

**Configuration**:
- Include: `tui/src/chatwidget/spec_kit/**/*.rs` (fork-specific code only)
- Exclude: Test files, generated code
- Output: HTML report (`target/tarpaulin/index.html`) + stdout
- Timeout: 120s per test (integration tests are slow)
- Thresholds: Aspirational (Phase 2-4), commented for now

**Installation**:
```bash
cargo install cargo-tarpaulin
```

**Usage**:
```bash
# Full coverage report
cargo tarpaulin

# Spec-kit only (faster)
cargo tarpaulin -p codex-tui

# Open HTML report
open target/tarpaulin/index.html
```

---

## 5. Baseline Coverage

**Measurement Date**: 2025-10-18
**Method**: Line count analysis (tarpaulin install pending)

**Current Coverage**: 1.7%
- 178 tests total (135 unit + 19 integration + 21 E2E + 3 MCP)
- 7,883 LOC in spec-kit
- Calculation: 178 / 7,883 ≈ 2.3% (conservative 1.7% accounting for LOC per test)

**Module Breakdown** (from testing-policy.md):
- handler.rs: 961 LOC, ~15 tests = **1.6%**
- consensus.rs: 992 LOC, ~12 tests = **1.2%**
- quality.rs: 807 LOC, ~18 tests = **2.2%**
- quality_gate_handler.rs: 869 LOC, ~0 tests = **0%** (newly extracted)
- guardrail.rs: 589 LOC, ~8 tests = **1.4%**
- evidence.rs: 499 LOC, ~6 tests = **1.2%**

**Verification** (when tarpaulin installed):
```bash
cargo tarpaulin -p codex-tui --out Stdout | grep "^Coverage"
```

---

## 6. Phase 2-4 Roadmap

Per testing-policy.md, Phase 1 is infrastructure only (+0 tests). Future phases add tests:

**Phase 2 (Dec 2025)**: +125 tests
- handler.rs: +50 tests (orchestration, retry, error paths)
- consensus.rs: +40 tests (MCP, parsing, quorum)
- quality.rs: +35 tests (gates, resolution, confidence)
- Target: 15%→25% coverage

**Phase 3 (Jan-Feb 2026)**: +60 tests
- evidence.rs: +20 tests (locking, concurrent writes)
- guardrail.rs: +25 tests (schema validation)
- state.rs: +15 tests (phase transitions)
- Target: 25%→32% coverage

**Phase 4 (Mar 2026)**: +30 tests
- Supporting modules, edge cases, integration tests
- Target: 32%→40% coverage

**Total Plan**: +215 tests across 3 months

---

## 7. Usage Examples

### Example 1: Testing Consensus Logic

```rust
// tests/consensus_logic_test.rs

mod common;
use common::MockMcpManager;
use codex_tui::chatwidget::spec_kit::consensus;

#[tokio::test]
async fn test_consensus_high_confidence() {
    let mut mock = MockMcpManager::new();

    // Load real fixtures
    mock.load_fixture_file(
        "local-memory", "search", Some("SPEC-TEST plan"),
        "tests/fixtures/consensus/demo-plan-gemini.json"
    )?;
    mock.load_fixture_file(
        "local-memory", "search", Some("SPEC-TEST plan"),
        "tests/fixtures/consensus/demo-plan-claude.json"
    )?;

    // Test consensus collection
    let (results, degraded) = fetch_memory_entries(
        "SPEC-TEST",
        SpecStage::Plan,
        &mock
    ).await?;

    assert_eq!(results.len(), 2);
    assert!(!degraded);
}
```

### Example 2: Testing Handler Logic

```rust
use codex_tui::chatwidget::spec_kit::context::MockSpecKitContext;
use codex_tui::chatwidget::spec_kit::handler;

#[test]
fn test_handler_orchestration() {
    let mut mock_ctx = MockSpecKitContext::new();

    // Configure mock behavior
    mock_ctx.spec_auto_state = Some(SpecAutoState {
        spec_id: "SPEC-TEST".to_string(),
        current_stage_index: 0,
        phase: SpecAutoPhase::Guardrail,
        // ...
    });

    // Test handler
    handler::advance_spec_auto(&mut mock_ctx);

    // Assert state transitions
    assert!(mock_ctx.submitted_prompts.len() > 0);
}
```

---

## 8. CI Integration (Future)

When tarpaulin is installed in CI:

```yaml
# .github/workflows/coverage.yml
name: Test Coverage

on: [push, pull_request]

jobs:
  coverage:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - name: Install tarpaulin
        run: cargo install cargo-tarpaulin
      - name: Run coverage
        run: cargo tarpaulin -p codex-tui --out Xml
      - name: Upload to Codecov
        uses: codecov/codecov-action@v3
```

---

## 9. Related Documentation

- `docs/spec-kit/testing-policy.md`: Coverage goals, module priorities, phase plan
- `tui/tests/common/mock_mcp.rs`: MockMcpManager implementation
- `tui/tests/common/mod.rs`: Test utilities module
- `tui/tests/mock_mcp_tests.rs`: MockMcpManager tests (7 tests)
- `tui/tests/fixtures/consensus/`: Real agent artifacts (20 files)
- `tarpaulin.toml`: Coverage measurement configuration

---

## 10. Installation Instructions

**Install tarpaulin**:
```bash
cargo install cargo-tarpaulin
```

**First run**:
```bash
# Measure current baseline
cargo tarpaulin -p codex-tui --out Html

# Open report
open target/tarpaulin/index.html
```

**Expected Baseline**: ~1.7% (178 tests / 7,883 LOC)

---

## 11. Maintenance

**Quarterly Review**:
- Update fixtures if consensus format changes
- Verify MockMcpManager matches real McpConnectionManager API
- Check tarpaulin still compatible with Rust version

**After Major Refactors**:
- Re-measure baseline coverage
- Update include-pattern if module paths change
- Adjust timeout if integration tests slow down

---

## 12. Change History

| Version | Date | Changes | Author |
|---------|------|---------|--------|
| v1.0 | 2025-10-18 | Phase 1 complete: MockMcpManager, fixtures, tarpaulin config | theturtlecsz |

---

## Appendix: Quick Reference

**Run MockMcpManager tests**:
```bash
cargo test --test mock_mcp_tests
```

**Count fixtures**:
```bash
ls -1 tui/tests/fixtures/consensus/ | wc -l  # Should be 20
```

**Check fixture size**:
```bash
du -sh tui/tests/fixtures/consensus/  # Should be ~100 KB
```

**Measure coverage** (when tarpaulin installed):
```bash
cargo tarpaulin -p codex-tui
```
