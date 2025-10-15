# Agent Spawning Fix - Root Cause Analysis

## Problem Discovered (from output.md)

**Symptoms:**
- 12 agents spawned, but only 5 executed (7 noops)
- Only gpt-5-codex models ran
- No gemini-2.5-pro or claude-4.5-sonnet despite config
- Agents refusing with "I am Claude, not Gemini. noop"

## Root Cause

**Agent `852a9916` revealed the issue:**
```
I need to respond with "noop" since I am Claude 4.5 Sonnet, not Gemini 2.5 Pro.
```

**What was happening:**
1. Orchestrator tried to spawn "gemini" agent
2. Agent system executed `claude` command instead (wrong mapping)
3. Claude received prompt: "You are Gemini 2.5 Pro, do research on SPEC..."
4. Claude correctly refused: "I'm Claude, not Gemini"

**Why:**
- Old config had generic agents without model specs:
  ```toml
  [[agents]]
  name = "gemini"
  command = "gemini"
  args = ["-y"]  # ← Which gemini model? Unknown!
  ```

- Orchestrator config said `agents = ["claude", "gemini", "code"]`
- But agent routing was ambiguous/random
- Models got cross-wired

## The Fix

**Created model-specific agent definitions:**

```toml
[[agents]]
name = "gemini-pro"
command = "gemini"
args = ["--model", "gemini-2.5-pro"]  # ← Explicit

[[agents]]
name = "claude-sonnet"
command = "claude"
args = ["--model", "sonnet"]  # ← Explicit

[[agents]]
name = "code"
command = "codex"
args = ["-m", "gpt-5-codex"]  # ← Already explicit
```

**Updated all subagent commands:**
```toml
[[subagents.commands]]
name = "spec-auto"
agents = ["gemini-pro", "claude-sonnet", "code"]  # ← Specific agent names
```

## Expected Behavior After Fix

**When orchestrator says "collect proposals from all agents":**
1. Spawns `gemini` command with `--model gemini-2.5-pro`
2. Spawns `claude` command with `--model sonnet`
3. Spawns `codex` command with `-m gpt-5-codex`

**Each agent:**
- Runs with correct model
- Receives appropriate prompt (no role confusion)
- Produces valid output
- No "noop" refusals

## Predicted Agent Counts (After Fix)

**Per stage (no conflicts):**
- 1 Policy prefilter (code/gpt-5-codex)
- 1 Policy final (code/gpt-5)
- 1 Gemini research (gemini/gemini-2.5-pro)
- 1 Claude synthesis (claude/claude-4.5-sonnet)
- 1 GPT validation (code/gpt-5)
= **5 agents/stage**

**6-stage pipeline:**
- 6 × 5 = **30 agents** (if no conflicts)
- +6 arbiters if conflicts = **36 agents**

**From output.md (12 agents):**
- Indicates 2-3 stages ran OR
- Many agents noop'd (now fixed)

## Test Plan

**1. Run small test:**
```bash
/new-spec Test agent model fix
/spec-auto SPEC-KIT-049-test-agent-model-fix
```

**2. Analyze immediately after:**
```bash
bash scripts/spec_ops_004/log_agent_runs.sh 30
```

**3. Verify:**
- ✓ Models Used shows: gemini-2.5-pro, claude-4.5-sonnet, gpt-5-codex
- ✓ No "noop" agents
- ✓ All agents produce valid JSON output
- ✓ ~5 agents per stage completed

**4. If still seeing noops:**
- Check agent CLIs accept those model names
- Verify credentials configured for gemini/claude
- Check if CLIs default to different models

## Optimization Opportunities (After Fix Verified)

Once models work correctly, implement:

1. **Parallel agent spawning** (33% faster)
   - Gemini + Claude in parallel
   - GPT after both complete

2. **Cache policy checks** (10 fewer agents)
   - Run once per pipeline
   - Reuse for all stages

3. **Single HAL check** (5 fewer checks)
   - Validate at start
   - Skip for subsequent stages

**Total optimization:** ~40% faster, 15 fewer agents

## Commands to Run

**Update config (already done):**
- Edited ~/.code/config.toml
- Changed generic "gemini"/"claude" to "gemini-pro"/"claude-sonnet"
- Added explicit --model flags

**Test:**
```bash
# Restart TUI to load new config
/quit
code

# Test agent fix
/spec-auto SPEC-KIT-040-add-simple-config-validation-utility

# Analyze results
bash scripts/spec_ops_004/log_agent_runs.sh 30
```

**Expected output:**
- Model distribution: mixed (gemini, claude, gpt-5-codex)
- No noop agents
- ~5 agents per stage
- All produce valid results

---

## Conclusion

**Root cause:** Generic agent definitions without model specs caused cross-wiring

**Fix:** Model-specific agent definitions with explicit flags

**Impact:** Should eliminate "noop" refusals, enable true multi-model consensus

**Next:** Test and verify with log_agent_runs.sh
