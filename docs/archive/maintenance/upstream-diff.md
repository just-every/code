# Upstream Tracking and Diff Pipeline

This document describes the process for tracking differences between the fork (`code-rs`) and upstream (`codex-rs`), identifying critical changes, and planning merges.

## Overview

The fork maintains parallel crate trees:
- **`codex-rs/`** - Upstream baseline (synced from `https://github.com/openai/codex.git`)
- **`code-rs/`** - Fork implementation with enhancements

The tracking pipeline helps identify:
1. **Structural differences** - Which crates have diverged
2. **Critical changes** - Changes affecting prompts, API surface, or executor
3. **Merge decisions** - Historical log of what was adopted/adapted/preserved

## Pipeline Scripts

All scripts are located in `scripts/upstream-merge/`:

### 1. `diff-crates.sh` - Structural Diff

Compares codex-rs vs code-rs on a per-crate basis.

**Usage:**
```bash
# Compare a single crate
./scripts/upstream-merge/diff-crates.sh core

# Compare all shared crates
./scripts/upstream-merge/diff-crates.sh --all

# Generate summary report
./scripts/upstream-merge/diff-crates.sh --summary
```

**Output:**
- Individual diff files: `.github/auto/upstream-diffs/<crate>.diff`
- Summary report: `.github/auto/upstream-diffs/SUMMARY.md`

**What it tracks:**
- Line-by-line differences between matching crates (after normalizing branding like `code-*` → `codex-*` and ignoring `.DS_Store` noise)
- Added/removed lines count
- Files present in one tree but not the other

### 2. `highlight-critical-changes.sh` - Critical Change Detection

Analyzes diffs to highlight changes in sensitive subsystems.

**Usage:**
```bash
# Analyze a specific crate
./scripts/upstream-merge/highlight-critical-changes.sh core

# Analyze all crates
./scripts/upstream-merge/highlight-critical-changes.sh --all
```

**Output:**
- Per-crate critical changes: `.github/auto/upstream-diffs/critical-changes/<crate>-critical.md`
- Summary: `.github/auto/upstream-diffs/critical-changes/CRITICAL-SUMMARY.md`

**What it tracks:**
- **Prompts**: Changes to `prompts/*.md`, `openai_tools.rs`, `agent_tool.rs`
- **API Surface**: Changes to `protocol.rs`, `app-server-protocol`, `mcp-types`
- **Executor**: Changes to `codex.rs`, `exec`, `apply-patch`, `acp.rs`
- **Config**: Changes to config types and behavior toggles
- Function signature changes
- Enum/struct definition changes
- Protocol message changes (EventMsg, Op variants)

### 3. `log-merge.sh` - Merge Activity Logger

Tracks merge decisions and resolutions for future reference.

**Usage:**
```bash
# Start a new merge
./scripts/upstream-merge/log-merge.sh init upstream/main

# Add notes during merge
./scripts/upstream-merge/log-merge.sh note conflict "core/src/codex.rs has protocol changes"

# Log decisions
./scripts/upstream-merge/log-merge.sh decision core preserve "Keep fork's QueueUserInput support"
./scripts/upstream-merge/log-merge.sh decision exec adopt "Upstream exec improvements compatible"

# Finalize when done
./scripts/upstream-merge/log-merge.sh finalize

# View historical logs
./scripts/upstream-merge/log-merge.sh summary
```

**Output:**
- Timestamped log: `docs/maintenance/upstream-merge-logs/merge-YYYYMMDD.md`

**What it tracks:**
- Upstream ref and commit being merged
- Fork state at merge start
- Critical changes identified
- Decisions made (adopt/adapt/preserve) with rationale
- Post-merge checklist status

### 4. `verify.sh` - Post-Merge Verification

Validates that the fork still builds and maintains critical functionality after a merge.

**Usage:**
```bash
./scripts/upstream-merge/verify.sh
```

**What it verifies:**
- Build passes (`build-fast.sh`)
- Core API surface compiles
- Static guards (browser/agent tools still registered)
- Version handling intact
- Branding consistency

## Monthly Cadence

We follow a two-phase cadence. Use the first Monday for lightweight scans; reserve the second Monday for merge planning if changes warrant action.

### First Monday — Quick Diff

```bash
# Fetch latest upstream
git fetch upstream

# Check for new commits
git log HEAD..upstream/main --oneline

# Generate structural diff
./scripts/upstream-merge/diff-crates.sh --all

# Review high-level summary
cat .github/auto/upstream-diffs/SUMMARY.md
```

Goal: understand upstream churn. If nothing critical changed, stop here.

### Second Monday — Merge Planning (Conditional)

```bash
# Highlight critical changes
./scripts/upstream-merge/highlight-critical-changes.sh --all

# Review critical summary
cat .github/auto/upstream-diffs/critical-changes/CRITICAL-SUMMARY.md

# Initialize merge log when planning an integration
./scripts/upstream-merge/log-merge.sh init upstream/main
```

Goal: categorize changes (adopt/adapt/preserve) and document decisions.

## Typical Workflow

```bash
# 1. Initialize merge log
./scripts/upstream-merge/log-merge.sh init upstream/main

# 2. Review diffs
cat .github/auto/upstream-diffs/SUMMARY.md
cat .github/auto/upstream-diffs/critical-changes/CRITICAL-SUMMARY.md

# 3. For each critical crate, make decisions
#    Actions: adopt (take upstream), adapt (merge changes), preserve (keep fork)
./scripts/upstream-merge/log-merge.sh decision core adapt "Merge ACP support while preserving QueueUserInput"
./scripts/upstream-merge/log-merge.sh decision protocol adapt "Add new ACP variants, keep existing Op types"
./scripts/upstream-merge/log-merge.sh decision tui preserve "Fork-only Rust TUI, ignore upstream TS changes"

# 4. Apply changes crate-by-crate
#    (Manual or scripted - depends on complexity)

# 5. Verify after each crate
./scripts/upstream-merge/verify.sh

# 6. Finalize and document
./scripts/upstream-merge/log-merge.sh finalize
git add .
git commit -m "chore(codex-rs): sync with upstream main

See docs/maintenance/upstream-merge-logs/merge-YYYYMMDD.md"
```

### Integration Strategies

**Adopt (take upstream as-is):**
- Upstream change is a strict improvement
- No fork-specific functionality conflicts
- Example: Performance improvements, bug fixes in non-critical paths

**Adapt (merge both):**
- Upstream adds new capability, fork has extensions
- Combine both sets of features
- Example: Upstream adds ACP events, fork keeps existing event types

**Preserve (keep fork):**
- Upstream removes/changes functionality fork depends on
- Fork behavior is intentionally different
- Example: Rust TUI vs TypeScript TUI, fork-specific tools

## Critical Subsystems

Pay special attention to changes in:

### Prompts and Tools
- `prompts/*.md` - System prompts
- `codex-rs/core/src/openai_tools.rs` - Tool definitions
- `codex-rs/core/src/agent_tool.rs` - Agent-specific tools

Changes here affect LLM behavior and available capabilities.

### Protocol and API
- `codex-rs/core/src/protocol.rs` - Core event types
- `codex-rs/app-server-protocol/` - Frontend communication
- `codex-rs/mcp-types/` - MCP protocol definitions

Changes here affect client compatibility and event flow.

### Executor
- `codex-rs/core/src/codex.rs` - Main execution loop
- `codex-rs/exec/src/` - Command execution
- `codex-rs/apply-patch/src/` - File modification
- `codex-rs/core/src/acp.rs` - Agent Client Protocol bridge

Changes here affect how tools execute and side effects are managed.

### Configuration
- `codex-rs/core/src/config.rs` - Config loading
- `codex-rs/core/src/config_types.rs` - Config schema

Changes here affect default behavior and user-facing options.

## Parity Tracking

After each merge, update `docs/code-crate-parity-tracker.md`:
- Note which crates moved closer to parity
- Update "Ready to delete code-*?" column
- Add notes about remaining divergence

Goal: Eventually delete `code-rs/*` crates that match upstream, consolidating to single implementation.

## References

- **Upstream ACP Integration**: `docs/upstream-acp-merge.md` - Detailed notes on integrating Agent Client Protocol
- **Parity Tracker**: `docs/code-crate-parity-tracker.md` - Per-crate divergence status
- **Merge Prompts**: `prompts/MERGE.md` - Guidance for resolving conflicts
- **Triage Prompts**: `prompts/TRIAGE.md` - Categorizing upstream changes

## Automation Opportunities

Future enhancements to pipeline:
- Auto-detect upstream releases via GitHub API
- Generate merge preview PRs automatically
- Classify changes by semantic impact (breaking/compatible/internal)
- Extract commit messages for changelog integration
- Cross-reference with fork's issue tracker

## Troubleshooting

**Diff shows everything changed:**
- Check line endings (CRLF vs LF)
- Verify both trees are clean (`git status`)
- Ensure upstream remote is correctly configured

**Critical changes not detected:**
- Update patterns in `highlight-critical-changes.sh`
- Check that diff files exist in `.github/auto/upstream-diffs/`

**Verify script fails:**
- Review `.github/auto/VERIFY.json` for specific failures
- Check logs in `.github/auto/VERIFY_*.log`
- Ensure all fork-specific guards are still valid

**Merge log not found:**
- Logs are in `docs/maintenance/upstream-merge-logs/`
- Initialize with `log-merge.sh init` before logging notes/decisions

## Maintenance

Update these scripts when:
- New critical files/patterns emerge (update `CRITICAL_PATTERNS` in `highlight-critical-changes.sh`)
- New crates are added to either tree (update `SHARED_CRATES` in `diff-crates.sh`)
- New verification guards needed (update `verify.sh`)
- Merge workflow evolves (update this doc and `log-merge.sh`)

---

Document created: 2025-10-05
Last updated: 2025-10-05
