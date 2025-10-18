# Upstream Sync Strategy

**Status**: v1.0 (2025-10-18)
**Owner**: theturtlecsz
**Upstream**: https://github.com/just-every/code
**Fork**: https://github.com/theturtlecsz/code
**References**: REVIEW.md, SPEC.md (DOC-6), FORK_DEVIATIONS.md

---

## 1. Overview

This document defines the strategy for syncing theturtlecsz/code fork with the just-every/code upstream repository. The goal is to incorporate upstream improvements while preserving spec-kit functionality.

**Key Challenge**: Spec-kit is deeply embedded in TUI (7,883 LOC in `tui/src/chatwidget/spec_kit/`), creating merge conflicts with upstream changes to `chatwidget/mod.rs` and `app.rs`.

**Mitigation**: 98.8% of spec-kit code is isolated in dedicated module, minimizing conflict surface.

---

## 2. Sync Frequency

**Current Policy**: Monthly or quarterly syncs

**Decision Factors**:
- **Monthly** (preferred): If upstream is actively maintained with frequent releases
- **Quarterly**: If upstream changes are infrequent or spec-kit development is high-velocity

**Last Sync**: Check `git log --oneline --graph --all | head -20` for merge commits

**Next Sync Target**: 2025-11-15 (monthly) or 2026-01-15 (quarterly)

---

## 3. Upstream Tracking

### 3.1 Remote Configuration

**Add upstream remote** (if not already configured):
```bash
git remote add upstream https://github.com/just-every/code.git
git fetch upstream
```

**Verify remotes**:
```bash
git remote -v
# Expected:
# origin    https://github.com/theturtlecsz/code.git (fetch)
# origin    https://github.com/theturtlecsz/code.git (push)
# upstream  https://github.com/just-every/code.git (fetch)
# upstream  https://github.com/just-every/code.git (push)
```

### 3.2 Branch Naming

**Fork Branches**:
- `main` (or `master`) - Stable fork with spec-kit
- `feat/*` - Feature branches for spec-kit development

**Upstream Tracking**:
- `upstream/main` - Upstream's main branch
- Create temporary `sync/YYYY-MM-DD` branch for merge work

---

## 4. Sync Process

### 4.1 Pre-Sync Checklist

**Before starting sync**:
- [ ] All tests passing (`cargo test --workspace`)
- [ ] No uncommitted changes (`git status --short` is empty)
- [ ] SPEC-KIT-DEMO runs successfully (`/speckit.status SPEC-KIT-DEMO`)
- [ ] Create backup branch: `git branch backup/pre-sync-$(date +%Y%m%d)`

### 4.2 Fetch Upstream Changes

```bash
# Fetch latest upstream
git fetch upstream

# Review incoming changes
git log HEAD..upstream/main --oneline | head -20

# Identify potentially conflicting commits
git log HEAD..upstream/main --oneline --grep="tui\|chatwidget\|app\.rs"
```

### 4.3 Merge Strategy

**Use `--no-ff --no-commit` to inspect before finalizing**:
```bash
# Create sync branch
git checkout -b sync/$(date +%Y%m%d)

# Merge with inspection window
git merge --no-ff --no-commit upstream/main

# Review conflicts
git status
```

**Conflict Resolution Priority**:
1. **Spec-kit code** (`tui/src/chatwidget/spec_kit/`) - Preserve fork changes
2. **App.rs** - Carefully merge, prioritize MCP sharing (ARCH-005)
3. **Chatwidget/mod.rs** - Keep spec-kit hooks, accept upstream refactors where compatible
4. **Core/config.rs** - Merge config precedence documentation (ARCH-003)
5. **Everything else** - Accept upstream changes unless spec-kit depends on it

### 4.4 Conflict Resolution Workflow

**Step 1: Identify conflict zones**
```bash
git diff --name-only --diff-filter=U
```

**Step 2: For spec-kit-specific files** (`tui/src/chatwidget/spec_kit/*`):
```bash
# Always keep fork version (ours)
git checkout --ours tui/src/chatwidget/spec_kit/handler.rs
git add tui/src/chatwidget/spec_kit/handler.rs
```

**Step 3: For shared files** (`app.rs`, `chatwidget/mod.rs`):
```bash
# Manual resolution required
$EDITOR tui/src/app.rs

# Look for conflict markers
# <<<<<<< HEAD (fork)
# =======
# >>>>>>> upstream/main

# Strategy:
# - Keep spec-kit initialization code (MCP manager spawn, spec_kit field)
# - Accept upstream refactors (event handling, rendering)
# - Test both independently, then together
```

**Step 4: For core files** (non-spec-kit):
```bash
# Accept upstream version (theirs) unless spec-kit depends on it
git checkout --theirs core/src/codex.rs
git add core/src/codex.rs
```

### 4.5 Post-Merge Validation

**After resolving all conflicts**:
```bash
# Complete the merge
git commit -m "Merge upstream main ($(date +%Y-%m-%d))"

# Run full test suite
cargo test --workspace --all-features

# Run spec-kit smoke tests
cargo test -p codex-tui spec_kit

# Build release binary
cd codex-rs && cargo build --release

# Run SPEC-KIT-DEMO end-to-end
# (Manual: Launch TUI, run /speckit.status SPEC-KIT-DEMO)
```

**If tests fail**:
1. Identify failure location (`cargo test --workspace -- --nocapture`)
2. Check if upstream changed API surface that spec-kit depends on
3. Update spec-kit code to match new upstream API
4. Re-run tests until passing

### 4.6 Merge to Main

**After validation passes**:
```bash
# Switch to main
git checkout main

# Merge sync branch (fast-forward or merge commit)
git merge sync/$(date +%Y%m%d)

# Push to fork
git push origin main

# Clean up sync branch
git branch -d sync/$(date +%Y%m%d)
```

---

## 5. Conflict Hotspots

### 5.1 Known Conflict Areas

Based on REVIEW.md analysis and fork history:

| File | Fork LOC | Spec-Kit Coupling | Conflict Risk |
|------|----------|-------------------|---------------|
| `tui/src/chatwidget/mod.rs` | 16,598 | HIGH (spec_auto_state field, slash command routing) | **CRITICAL** |
| `tui/src/app.rs` | - | MEDIUM (MCP manager spawn, event handling) | **HIGH** |
| `core/src/config.rs` | - | LOW (shell policy validation ARCH-003) | MEDIUM |
| `core/src/codex.rs` | 11,395 | NONE (upstream only) | LOW |
| `tui/src/chatwidget/spec_kit/**` | 7,883 | N/A (fork-only) | **NONE** |

### 5.2 chatwidget/mod.rs Merge Strategy

**Fork-specific additions** (lines to preserve):
- `spec_auto_state: Option<SpecAutoState>` field (state machine)
- `handle_spec_*` method calls in slash command routing
- `SpecKitContext` trait implementation

**Upstream changes to accept**:
- Event loop refactors
- Rendering optimizations
- Tool call handling improvements
- History cell updates

**Conflict Resolution**:
```rust
// Example conflict in chatwidget/mod.rs
// <<<<<<< HEAD (fork)
pub struct ChatWidget {
    // ...
    spec_auto_state: Option<SpecAutoState>,  // KEEP THIS
}
// =======
pub struct ChatWidget {
    // ...
    new_upstream_field: SomeType,  // ACCEPT THIS
}
// >>>>>>> upstream/main

// Resolution: Keep both
pub struct ChatWidget {
    // ...
    spec_auto_state: Option<SpecAutoState>,
    new_upstream_field: SomeType,
}
```

### 5.3 app.rs Merge Strategy

**Fork-specific additions** (ARCH-005):
- App-level MCP manager spawn (lines ~314)
- `mcp_manager` field passed to ChatWidget
- Shared manager across forked widgets

**Resolution**: Keep fork version, but check if upstream added new MCP features to adopt.

---

## 6. Isolation Metrics

### 6.1 Current Isolation (ARCH-005 complete)

**Spec-kit isolation**: 98.8%
- 7,883 LOC in dedicated `spec_kit/` module
- 14 files entirely fork-specific
- Only 2 files with shared coupling:
  - `chatwidget/mod.rs` (~100 LOC spec-kit code / 16,598 total = 0.6%)
  - `app.rs` (~80 LOC spec-kit code / unknown total)

**Conflict surface**: ~180 LOC (out of 7,883 spec-kit + 180 integration = 2.2%)

### 6.2 Improving Isolation (Future)

**ARCH-005 already addressed**:
- MCP manager moved to App (eliminates one conflict source)

**Remaining improvements** (REVIEW.md recommendations):
- Extract spec-kit to separate crate (`codex-spec-kit`) - **2-4 week effort**
- Reduces conflict surface to ~50 LOC (trait implementations only)
- Makes spec-kit reusable (CLI, API server)

**Decision**: Defer extraction until upstream sync friction becomes blocking (not yet).

---

## 7. Testing Strategy

### 7.1 Pre-Merge Tests

**Before finalizing merge commit**:
```bash
# Full workspace tests
cargo test --workspace --all-features

# Spec-kit unit tests
cargo test -p codex-tui --lib spec_kit

# Spec-kit integration tests
cargo test -p codex-tui --test quality_gates_integration
cargo test -p codex-tui --test spec_auto_e2e

# MCP consensus tests
cargo test -p codex-tui --test mcp_consensus_integration
```

**Expected**: All 178+ tests passing

### 7.2 Smoke Test (Manual)

**Run SPEC-KIT-DEMO**:
```bash
cd codex-rs
cargo build --release --bin code
./target/release/code  # Launch TUI

# In TUI:
/speckit.status SPEC-KIT-DEMO
# Expected: Stage completion status, no errors

# Optional: Run full pipeline
/speckit.auto SPEC-KIT-DEMO --from plan
# (If DEMO is already complete, create new SPEC)
```

### 7.3 Post-Merge Monitoring

**After pushing merge to main**:
- Monitor CI (if configured)
- Check for user-reported issues
- Validate on production-like environment

---

## 8. Rollback Procedures

### 8.1 If Merge Goes Wrong

**Before pushing to origin**:
```bash
# Abort merge
git merge --abort

# Or reset to pre-merge state
git reset --hard backup/pre-sync-$(date +%Y%m%d)
```

**After pushing to origin** (emergency):
```bash
# Revert merge commit
git revert -m 1 HEAD

# Push revert
git push origin main

# Investigate issue, retry sync when resolved
```

### 8.2 Recovery Strategy

**If upstream changes break spec-kit**:
1. Identify breaking change (API refactor, removed method, etc.)
2. Update spec-kit code to match new API
3. Re-run tests
4. If complex, create feature branch for adaptation work
5. Merge when stable

---

## 9. Communication & Coordination

### 9.1 Upstream Contributions

**Contributing spec-kit back to upstream?**
- **Current stance**: No plans (spec-kit is opinionated workflow tool)
- **Future**: If upstream shows interest, consider extracting generic parts

**Contributing bug fixes back**:
- If fork discovers upstream bugs, submit PR to just-every/code
- Reduces future sync friction

### 9.2 Tracking Upstream

**Monitor upstream activity**:
```bash
# Check for new releases
gh release list --repo just-every/code

# Check commit activity
git log upstream/main --oneline --since="1 month ago"
```

**Subscribe to upstream** (optional):
- Watch just-every/code repository on GitHub
- Get notified of major releases or breaking changes

---

## 10. Long-Term Strategy

### 10.1 Upstream Sync Goals

**Short-term** (2025-2026):
- Quarterly syncs to stay current
- Minimize conflict surface (maintain 98.8% isolation)
- Adopt upstream performance improvements

**Long-term** (2026+):
- Extract spec-kit to separate crate (if sync friction increases)
- Contribute generic improvements back upstream
- Reduce sync frequency if upstream stabilizes

### 10.2 Fork Independence Assessment

**Decision criteria** for going fully independent:
- Upstream development stops
- Spec-kit becomes >50% of codebase
- Upstream breaks compatibility repeatedly

**Current stance**: Remain synced (upstream is active, spec-kit benefits from base improvements)

---

## 11. Rebase Strategy (Alternative)

**Current approach**: Merge (preserves history)

**Rebasing** (not recommended for this fork):
- **Pro**: Linear history, no merge commits
- **Con**: Rewrites commit history, dangerous with spec-kit conflicts
- **Con**: Breaks shared history with upstream

**See FORK_DEVIATIONS.md** for detailed rebase safety analysis (98.8% isolation documented).

**Decision**: Use merge strategy, not rebase.

---

## 12. Appendix: Quick Reference

### Sync Checklist

```bash
# 1. Pre-sync
git status --short  # Must be empty
cargo test --workspace --all-features  # Must pass
git branch backup/pre-sync-$(date +%Y%m%d)

# 2. Fetch and merge
git fetch upstream
git checkout -b sync/$(date +%Y%m%d)
git merge --no-ff --no-commit upstream/main

# 3. Resolve conflicts
git status  # Check conflicted files
# For spec-kit files: git checkout --ours <file>
# For shared files: manual resolution
# For core files: git checkout --theirs <file> (if no spec-kit dependency)

# 4. Validate
cargo test --workspace --all-features
cargo test -p codex-tui spec_kit
# Manual smoke test: /speckit.status SPEC-KIT-DEMO

# 5. Finalize
git commit -m "Merge upstream main ($(date +%Y-%m-%d))"
git checkout main
git merge sync/$(date +%Y%m%d)
git push origin main
```

### Conflict Resolution

**Spec-kit files** (`tui/src/chatwidget/spec_kit/*`): `git checkout --ours`
**Core files** (no spec-kit dependency): `git checkout --theirs`
**Shared files** (`app.rs`, `chatwidget/mod.rs`): Manual merge
**Config files** (cargo, build): Inspect carefully, usually accept upstream

---

## 13. Related Documentation

- `FORK_DEVIATIONS.md`: 98.8% isolation analysis, rebase safety matrix
- `REVIEW.md`: Architecture analysis, conflict hotspots
- `ARCHITECTURE-TASKS.md`: ARCH-005 (MCP manager isolation)
- `docs/spec-kit/testing-policy.md`: Post-merge test strategy

---

## 14. Change History

| Version | Date | Changes | Author |
|---------|------|---------|--------|
| v1.0 | 2025-10-18 | Initial sync strategy | theturtlecsz |

---

## 15. Next Steps

**Immediate**:
- [ ] Verify upstream remote configured
- [ ] Schedule next sync (monthly: 2025-11-15 or quarterly: 2026-01-15)
- [ ] Document last sync date in SPEC.md

**Before next sync**:
- [ ] Run pre-sync checklist (§4.1)
- [ ] Review upstream release notes for breaking changes
- [ ] Allocate 2-4 hours for sync + validation
