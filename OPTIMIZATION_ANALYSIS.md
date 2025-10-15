# Spec-Auto Performance Optimizations

## Current Bottlenecks (Analysis)

### Timing Breakdown (Per Stage)

**Plan stage example:**
```
00:00 - Bash guardrail starts
00:00   ├─ Baseline audit (fast, ~30s)
00:00   ├─ Policy prefilter (LLM agent, ~3-5 min)
03:00   ├─ Policy final (LLM agent, ~3-5 min)
06:00   └─ HAL validation (API calls, ~30s)
07:00 - Bash guardrail completes

07:00 - Orchestrator multi-agent consensus starts
07:00   ├─ Spawn gemini (0.1s)
07:01   ├─ Wait for gemini (~3-5 min)
10:00   ├─ Spawn claude (0.1s)
10:01   ├─ Wait for claude (~3-5 min)
13:00   ├─ Spawn gpt_pro (0.1s)
13:01   ├─ Wait for gpt_pro (~3-5 min)
16:00   └─ Consensus synthesis (file I/O, ~10s)
16:10 - Stage complete
```

**Total: ~16 minutes per stage**
**6 stages: 96 minutes (~1.5 hours)**

---

## Optimization Opportunities (Impact vs Effort)

### 1. 🔥 Parallel Agent Spawning (HIGHEST IMPACT)

**Current (sequential):**
```
Gemini → wait (5 min) → Claude → wait (5 min) → GPT → wait (5 min)
Total: 15 minutes
```

**Optimized (parallel):**
```
Gemini + Claude + GPT all start together
Wait for slowest (~5 min)
Total: 5 minutes
```

**Savings:** 10 minutes per stage × 6 = **60 minutes total (40% faster)**

**Implementation:**
```toml
# In ~/.code/config.toml orchestrator instructions:
STEP 2: Multi-agent consensus (PARALLEL)

Start all agents simultaneously:
agent_run name=research-{stage} prompt={gemini prompt}
agent_run name=synthesis-{stage} prompt={claude prompt}
agent_run name=validation-{stage} prompt={gpt prompt}

Wait for all three to complete.
Collect results.
```

**Risk:** ZERO - agents don't depend on each other until synthesis
**Effort:** 5 minutes (update orchestrator instructions)
**Quality:** NO LOSS

---

### 2. 🔥 Parallel Policy Checks

**Current (sequential):**
```
Policy prefilter (5 min) → Policy final (5 min)
Total: 10 minutes
```

**Optimized (parallel):**
```
Policy prefilter & Policy final run together
Wait for both (~5 min)
Total: 5 minutes
```

**Savings:** 5 minutes per stage × 6 = **30 minutes total**

**Implementation:**
```bash
# In spec_ops_plan.sh (and other stage scripts):
spec_ops_run_policy_prefilter "${SPEC_ID}" "spec-plan" &
PREFILTER_PID=$!

spec_ops_run_policy_final "${SPEC_ID}" "spec-plan" &
FINAL_PID=$!

wait $PREFILTER_PID
wait $FINAL_PID
```

**Risk:** LOW - policy checks are independent
**Effort:** 30 minutes (modify 6 guardrail scripts)
**Quality:** NO LOSS

---

### 3. 🟡 Single HAL Check Per Pipeline

**Current:**
```
6 stages × HAL validation (30s each) = 3 minutes
```

**Optimized:**
```
HAL check once at pipeline start = 30 seconds
```

**Savings:** **2.5 minutes total**

**Implementation:**
```bash
# In spec_auto.sh, before stage loop:
if [[ "${SPEC_OPS_HAL_SKIP:-0}" != "1" ]]; then
  spec_ops_run_hal_smoke
  export SPEC_OPS_HAL_CACHED=1
fi

# In stage scripts:
if [[ "${SPEC_OPS_HAL_CACHED:-0}" == "1" ]]; then
  # Skip HAL, use cached result
fi
```

**Risk:** MEDIUM - if implement stage breaks API, later stages won't detect
**Effort:** 1 hour
**Quality:** Small risk (API breakage between stages)

---

### 4. 🟡 Reduce Policy Check Reasoning

**Current:**
```toml
args = ["-m", "gpt-5-codex"]  # Uses default reasoning (medium)
```

**Optimized:**
```toml
args = ["-m", "gpt-5-codex", "-c", "model_reasoning_effort=\"low\""]
```

**Savings:** ~2 minutes per policy check × 12 = **24 minutes total**

**Risk:** MEDIUM - lower reasoning might miss policy violations
**Effort:** 10 minutes (update agent config)
**Quality:** POTENTIAL DEGRADATION

---

### 5. 🟢 Agent Result Streaming (Don't Wait for Complete)

**Current:**
```
Start gemini → wait until 100% done → start claude
```

**Optimized:**
```
Start gemini → check at 80% → start claude early
```

**Savings:** ~2 minutes per stage = **12 minutes total**

**Risk:** LOW - claude gets partial gemini output but agents are smart
**Effort:** HIGH - requires TUI/orchestrator changes
**Quality:** Minimal impact

---

### 6. ⚪ Context Caching (Already Done)

**Status:** ✅ Implemented

**Savings:** ~5-10% of policy check time (~5 min total)

---

## Recommended Implementation Order

### Priority 1: Parallel Agent Spawning (60 min savings, 5 min effort)

**DO THIS NOW:**

Edit `~/.code/config.toml`, change orchestrator instructions:

**Before:**
```
agent_run name=gemini-{stage} ...
agent_wait name=gemini-{stage}

agent_run name=claude-{stage} ...
agent_wait name=claude-{stage}

agent_run name=gpt-{stage} ...
agent_wait name=gpt-{stage}
```

**After:**
```
Spawn all 3 agents in parallel:
agent_run name=research-{stage} ...
agent_run name=synthesis-{stage} ...
agent_run name=validation-{stage} ...

Wait for all to complete:
agent_wait (all three)

Collect results:
agent_result name=research-{stage}
agent_result name=synthesis-{stage}
agent_result name=validation-{stage}
```

---

### Priority 2: Parallel Policy Checks (30 min savings, 30 min effort)

**Implementation:**

Update all 6 stage scripts (`spec_ops_plan.sh`, etc.):

```bash
# Run policy checks in parallel
(
  spec_ops_run_policy_prefilter "${SPEC_ID}" "${STAGE}"
  echo $? > /tmp/prefilter-exit-$$
) &
PREFILTER_PID=$!

(
  spec_ops_run_policy_final "${SPEC_ID}" "${STAGE}"
  echo $? > /tmp/final-exit-$$
) &
FINAL_PID=$!

wait $PREFILTER_PID
wait $FINAL_PID

# Check results
PREFILTER_EXIT=$(cat /tmp/prefilter-exit-$$ 2>/dev/null || echo 1)
FINAL_EXIT=$(cat /tmp/final-exit-$$ 2>/dev/null || echo 1)
```

---

### Priority 3: Single HAL Check (2.5 min savings, 1 hour effort)

**Skip for now** - minimal gain, adds complexity

---

### Priority 4: Lower Policy Reasoning (24 min savings, risky)

**Skip** - quality degradation not worth it

---

## Expected Total Savings

**If we implement Priority 1 + 2:**
- Parallel agents: 60 minutes
- Parallel policy: 30 minutes
- Context caching: 5 minutes (done)
- **Total: 95 minutes faster**

**New pipeline time:**
- Current: ~96 minutes
- Optimized: ~40 minutes (58% faster)

---

## Test Plan

**After Priority 1:**
```bash
# Update config
# Restart TUI
/quit; code

# Time single stage
time /spec-auto SPEC-KIT-040 --from plan

# Should complete plan in ~6 min instead of 16 min
```

**After Priority 2:**
```bash
# Full pipeline
time /spec-auto SPEC-KIT-040

# Should complete all 6 stages in ~40 min instead of 96 min
```

---

## Implementation Now

**Want me to:**
1. Update orchestrator instructions for parallel agent spawning (5 min)
2. Modify 6 guardrail scripts for parallel policy (30 min)
3. Test and verify improvements

**OR wait and focus on getting 1 full successful run first?**
