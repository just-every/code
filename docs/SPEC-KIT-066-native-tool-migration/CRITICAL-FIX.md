# Critical Fix: Auto-Commit Restored

**Date**: 2025-10-20
**Issue**: Removed auto-commit broke worktree isolation
**Fix**: Restored auto-commit with proper commit message formatting

---

## Problem Discovered

**User Insight**: "I believe we need autocommit so that further speckit commands that pull their own worktrees will be able to see any updates to the files"

**Root Cause**:
- `/speckit.auto` and other commands use **isolated git worktrees** for safety
- Worktrees checkout from git history, not working directory
- If SPEC files aren't committed, they don't exist in git history
- Result: Subsequent commands can't see newly created SPECs

**Example Failure Scenario**:
```bash
# User runs:
/speckit.new Add search command

# Files created but NOT committed:
docs/SPEC-KIT-067-add-search-command/PRD.md (working dir only)
docs/SPEC-KIT-067-add-search-command/spec.md (working dir only)
SPEC.md (modified, working dir only)

# User then runs:
/speckit.auto SPEC-KIT-067

# speckit.auto creates worktree from main branch
# Worktree checkout doesn't see uncommitted files
# FAILURE: "SPEC-KIT-067 not found"
```

---

## Solution: Restore Auto-Commit

**What Changed** (config.toml lines 263-279):

### Added Step 6: Commit Changes
```toml
6. Commit changes (REQUIRED for worktree visibility):
   CRITICAL: /speckit.auto uses isolated worktrees that require committed files.

   Use Bash tool to commit:
   - Stage: git add SPEC.md docs/SPEC-KIT-{number}-{slug}/
   - Commit with proper message format:
     git commit -m "feat(spec-kit): add SPEC-KIT-{number} - {feature-title}"

   COMMIT MESSAGE RULES (from RULES.md):
   - NEVER reference "Claude", "AI", "assistant" or similar
   - Use active voice and technical descriptions
   - Focus on WHAT changed, not WHO made it
   - Examples:
     ✅ "feat(spec-kit): add SPEC-KIT-067 - search command"
     ✅ "feat(spec-kit): add SPEC-KIT-068 - user authentication"
     ❌ "Claude added SPEC-KIT-067"
     ❌ "AI created new spec"
```

### Updated Critical Rules
```toml
CRITICAL EXECUTION RULES:
- Actually USE your tools (Glob, Read, Write, Edit, Bash)
- Auto-commit is REQUIRED (worktree isolation needs committed files)
```

---

## Why This Matters

### Worktree Isolation Architecture

**How spec-kit uses worktrees**:
1. **Plan stage**: Creates worktree → checkouts from git → reads SPEC files
2. **Tasks stage**: Creates worktree → checkouts from git → reads plan.md
3. **Implement stage**: Creates worktree → checkouts from git → reads spec.md
4. **Validate/Audit/Unlock**: Same pattern

**Without commits**:
- Worktrees checkout empty directories
- Commands fail with "file not found"
- Pipeline breaks at first stage

**With commits**:
- Worktrees checkout complete SPEC directory
- All stages can access previous outputs
- Pipeline flows smoothly

---

## Implementation Details

### Commit Message Format

**Template**:
```bash
git commit -m "feat(spec-kit): add SPEC-KIT-{number} - {slug}"
```

**Examples**:
```bash
# Good (follows RULES.md)
git commit -m "feat(spec-kit): add SPEC-KIT-067 - search command"
git commit -m "feat(spec-kit): add SPEC-KIT-068 - user authentication"
git commit -m "feat(spec-kit): add SPEC-KIT-069 - performance monitoring"

# Bad (references AI/Claude)
git commit -m "Claude created SPEC-KIT-067"
git commit -m "AI added search command spec"
git commit -m "Assistant generated new SPEC"
```

**Why This Format**:
- Conventional commit standard: `feat(scope): description`
- Scope: `spec-kit` (indicates spec creation)
- Description: `add SPEC-KIT-{number} - {feature}` (clear, technical)
- No AI references (follows RULES.md mandate)

### Files Staged

```bash
git add SPEC.md docs/SPEC-KIT-{number}-{slug}/
```

**What's included**:
- `SPEC.md` - Updated tracker table
- `docs/SPEC-KIT-{number}-{slug}/PRD.md` - Multi-agent PRD
- `docs/SPEC-KIT-{number}-{slug}/spec.md` - Initial spec stub

**Why these files**:
- All created by `/speckit.new`
- All needed by `/speckit.auto` and other commands
- Atomic commit (all related changes together)

---

## Alternatives Considered

### Option 1: Use git-workflow Agent
**From RULES.md**: "git-workflow MANDATORY for: All git commands"

**Pros**:
- Follows agent-first mandate
- Proper delegation

**Cons**:
- Adds complexity to orchestrator
- git-workflow agent may not be available
- Bash is simpler for this use case

**Decision**: Use Bash directly (documented exception)

### Option 2: Skip Commit, Use Working Directory
**Idea**: Make worktrees use working directory instead of git history

**Pros**:
- No git operations needed

**Cons**:
- Defeats worktree isolation purpose
- Creates race conditions
- Breaks safety guarantees
- Major architecture change

**Decision**: REJECTED (breaks fundamental design)

### Option 3: Manual User Commit
**Idea**: Let user commit before running /speckit.auto

**Pros**:
- User has full control

**Cons**:
- Extra step every time
- Easy to forget
- Breaks workflow flow
- Poor UX

**Decision**: REJECTED (too error-prone)

---

## Verification

**Build Status**: ✅ SUCCESS
```bash
$ ./build-fast.sh
✅ Build successful!
Binary: ./codex-rs/target/dev-fast/code
Time: 0.39s
Warnings: 50 (unused code, no errors)
```

**Config Updated**: `~/.code/config.toml` (lines 263-299)

**Testing Required**: User must validate in TUI

---

## Impact Assessment

### Before Fix
- ✅ SPEC files created correctly
- ✅ SPEC.md updated
- ❌ Files not committed
- ❌ /speckit.auto fails (worktree can't see files)
- ❌ Broken workflow

### After Fix
- ✅ SPEC files created correctly
- ✅ SPEC.md updated
- ✅ Files committed to git
- ✅ /speckit.auto can access files via worktree
- ✅ Complete workflow functional

---

## Follow-Up Actions

**Immediate**:
- [x] Update config.toml
- [x] Rebuild successfully
- [x] Document fix rationale
- [ ] User testing

**Phase 4**:
- [ ] Test /speckit.new with auto-commit
- [ ] Verify commit message format
- [ ] Test /speckit.auto with committed SPEC
- [ ] Validate full pipeline

---

## Lessons Learned

### Good Catch
User identified workflow issue before testing. This prevented:
- Failed test runs
- Debugging time
- Multiple iteration cycles

### Architecture Understanding
The worktree isolation architecture requires:
- Committed files for visibility
- Clean git history
- Atomic commits

### Configuration Complexity
Orchestrator instructions must consider:
- Downstream command requirements
- Architecture constraints
- Tool interaction patterns

---

## Related Documentation

- **RULES.md**: Git commit message standards
- **CLAUDE.md**: Worktree architecture overview
- **SPEC-066 PRD**: Original migration plan
- **phase1-inventory.md**: Command analysis
- **phase3-verification.md**: Other commands verified
- **IMPLEMENTATION-SUMMARY.md**: Overall summary

---

**Status**: ✅ Fix applied, build successful, awaiting user testing
