# Codex Investigation Prompt: Guardrail Subprocess Hang

**Copy this into Codex TUI to continue debugging**

---

## Context

Working on SPEC-066 Phase 4 validation. Quality gate orchestrator fixes complete but blocked by separate issue.

**Primary blocker**: Guardrail policy prefilter subprocess hangs indefinitely.

---

## Problem Statement

**Command**: `/speckit.auto SPEC-KIT-067`

**Expected**: Full 6-stage pipeline executes (plan → tasks → implement → validate → audit → unlock)

**Actual**:
1. Prints metadata (spec info, resume point, HAL mode)
2. Calls `scripts/spec_ops_004/commands/spec_ops_plan.sh SPEC-KIT-067`
3. Guardrail script calls: `code exec --sandbox workspace-write --model gpt-5-codex -- "Policy prefilter..."`
4. **Process hangs** - no output, no completion, runs 10-45 minutes
5. Pipeline blocked - never proceeds to agent execution

**Evidence**:
- Log file: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-067/spec-plan_2025-10-21T16:16:51Z-310439444.log`
- Last entry: "Policy prefilter checks..." (incomplete)
- No JSON output generated
- Multiple stuck processes found: PIDs 1686490, 1686523, 1705425, 1727205

---

## Investigation Steps

### 1. Check Model Configuration

```bash
# Find gpt-5-codex config
grep -A10 "gpt-5-codex\|gpt-5" ~/.code/config.toml

# Check what models are enabled
grep -B5 -A5 "enabled.*true" ~/.code/config.toml | grep -E "name|enabled"
```

**Questions**:
- Is gpt-5-codex configured?
- Is it enabled?
- Does it have valid API credentials?

### 2. Test Exec Mode Directly

```bash
# Test with working model
timeout 30 ./codex-rs/target/dev-fast/code exec --model claude-3-5-sonnet -- "What is 2+2?"

# Test with gpt-5-codex specifically
timeout 30 ./codex-rs/target/dev-fast/code exec --model gpt-5-codex -- "What is 2+2?"

# Check exit code
echo $?
```

**Expected**: Should complete in 5-10 seconds or fail fast
**If hangs**: Model configuration issue
**If "Unsupported model"**: Model not available/configured

### 3. Review Guardrail Script

```bash
# Check what policy prefilter does
cat scripts/spec_ops_004/common.sh | sed -n '65,100p'

# Check environment variables
env | grep SPEC_OPS

# Check if we can skip policy checks
export SPEC_OPS_POLICY_PREFILTER_CMD="echo skipped"
/speckit.auto SPEC-KIT-067 --from spec-plan
```

### 4. Check Recent Changes to Exec Mode

```bash
# See if any commits touched exec-related code
git log --oneline --all --grep="exec" -- codex-rs/exec/ codex-rs/cli/ | head -10

# Check for changes in last 36 commits
git diff 803399c41..HEAD -- codex-rs/exec/ codex-rs/cli/
```

**If changes found**: Our modifications may have broken exec mode
**If no changes**: Guardrail issue is environmental (models, config, credentials)

---

## Workarounds

### Option A: Skip Policy Checks
```bash
export SPEC_OPS_POLICY_PREFILTER_CMD="echo 'policy check skipped'"
/speckit.auto SPEC-KIT-067
```

### Option B: Use Working Model
```bash
# Edit scripts/spec_ops_004/common.sh
# Change: SPEC_OPS_POLICY_PREFILTER_MODEL from gpt-5-codex to claude-3-5-sonnet
# Line ~77: SPEC_OPS_POLICY_PREFILTER_MODEL=claude-3-5-sonnet
```

### Option C: Test Without Guardrails
```bash
# Use individual stage commands (no guardrails)
/speckit.plan SPEC-KIT-067
# Check: ls docs/SPEC-KIT-067/plan.md

/speckit.tasks SPEC-KIT-067
# Check: ls docs/SPEC-KIT-067/tasks.md
```

### Option D: Revert to Known Good
```bash
git checkout 803399c41
./build-fast.sh
# Test with pre-session binary
```

---

## Current Binary

**Path**: `./codex-rs/target/dev-fast/code`
**Hash**: `772748d578d39f592819922209a49e101867001c9b9666b010b166719c4b2131`
**Commit**: `059687df1` - "fix: remove debug logging interfering with exec subprocess"

**Changes from baseline**:
- Quality gates disabled (determine_quality_checkpoint returns None)
- quality_gate_processing flag added (unused)
- Debug logging removed
- 36 commits of experimental fixes

---

## Success Criteria

**Minimum** (Close SPEC-066):
- ✅ ONE successful /speckit.auto completion (any SPEC)
- ✅ Proves native tool migration works
- ✅ Can close SPEC-066 even without quality gates

**Ideal** (Understand Blocker):
- ✅ Identify why gpt-5-codex hangs
- ✅ Fix or workaround guardrail issue
- ✅ Document root cause
- ✅ File proper SPEC for quality gates (SPEC-068)

---

## Files for Reference

**Read first**:
- `docs/SPEC-KIT-066-native-tool-migration/REVIEW.md` (this file)
- `docs/SPEC-KIT-066-native-tool-migration/SESSION-RESTART-PROMPT.md`

**Code locations**:
- Quality gate handler: `codex-rs/tui/src/chatwidget/spec_kit/quality_gate_handler.rs:754-762` (disabled)
- Guardrail script: `scripts/spec_ops_004/commands/spec_ops_plan.sh`
- Policy prefilter: `scripts/spec_ops_004/common.sh:65-160`

**Evidence**:
- Latest log: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-067/spec-plan_2025-10-21T16:16:51Z-310439444.log`
- Local-memory: Search for "quality-gate SPEC-KIT-067 2025-10-21" (12+ entries)

---

## Copy-Paste Start Prompt

```
Investigate guardrail subprocess hang blocking SPEC-066 Phase 4.

ISSUE: code exec --model gpt-5-codex hangs during policy prefilter

STEPS:
1. Check model configuration (gpt-5-codex in ~/.code/config.toml)
2. Test exec mode directly with timeout
3. Try workaround: SPEC_OPS_POLICY_PREFILTER_CMD="echo skip"
4. If fixed: Run /speckit.auto SPEC-KIT-067 end-to-end
5. Close SPEC-066 if successful

CONTEXT: Read docs/SPEC-KIT-066-native-tool-migration/REVIEW.md

GOAL: ONE successful /speckit.auto run to validate native tools work
```

---

**Review complete. Handoff ready.**
