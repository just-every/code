# Agent Spawning Analysis Guide

## Current Status

From recent runs: **~20 agents spawned per /spec-auto execution**

## Expected Agent Count

**For single-stage with consensus:**
- 1 guardrail policy check (code agent)
- 3 consensus agents (gemini, claude, gpt_pro)
- 1 arbiter (if conflicts)
= **4-5 agents per stage**

**For 6-stage pipeline:**
- 6 stages × 4-5 agents = **24-30 agents total**

**So 20 agents is NORMAL if:**
- You ran partial pipeline (3-4 stages)
- No arbiter needed (conflicts auto-resolved)

## How to Analyze

**1. Run analysis:**
```bash
bash scripts/spec_ops_004/log_agent_runs.sh 180 > agent_analysis.md
cat agent_analysis.md
```

**2. Check model distribution:**
Look for "By Model" section. Should see:
```
~10 gpt-5-codex (policy checks + code consensus)
~6  gemini-2.5-pro (research agents)
~4  claude-4.5-sonnet (synthesis agents)
```

**3. Identify issues:**

**Problem: All same model**
- Means: orchestrator `agents = [...]` config wrong
- Fix: Ensure `agents = ["code", "gemini", "claude"]`

**Problem: Duplicate prompts**
- Check agent result.txt files for identical user instructions
- Means: Orchestrator retry loop or duplicate spawns
- Fix: Check orchestrator instructions for unnecessary loops

**Problem: Excessive policy checks**
- Count agents with "Policy prefilter" or "Policy final" instructions
- Should be 1 per stage (6 total max)
- If >6: guardrails spawning extra agents
- Fix: Check common.sh policy logic

**4. Timeline analysis:**
Look at "Working Directories" - should see distinct branch names per agent:
```
code-code-inputs--spec-packet-<timestamp>
code-gemini-<stage>-<timestamp>
code-claude-<stage>-<timestamp>
```

If seeing same directory reused → agents sequenced correctly ✓
If seeing many directories → parallel spawning (expected for multi-agent)

## Debugging Specific Issues

### Issue: Too Many Code Agents

**Symptom:** 15+ gpt-5-codex, only 2-3 gemini/claude

**Diagnosis:**
```bash
# Check orchestrator config
grep -A 3 'name = "spec-auto"' ~/.code/config.toml | grep agents

# Should show: agents = ["code", "gemini", "claude"]
# If shows: agents = ["code"]  ← PROBLEM
```

**Fix:** Already applied (agents list updated)

---

### Issue: Duplicate Spawns

**Symptom:** Multiple agents with identical prompts

**Diagnosis:**
```bash
# Compare agent prompts
for AGENT in /home/thetu/code/.code/agents/*/result.txt; do
  echo "=== $(basename $(dirname $AGENT)) ==="
  grep "User instructions:" -A 5 "$AGENT" | head -6
done | less
```

Look for identical instruction blocks.

**Fix:** Simplify orchestrator loop logic (remove retries)

---

### Issue: Unnecessary Stages

**Symptom:** Agents running for stages that should be skipped

**Diagnosis:**
Check if orchestrator is running ALL stages when you only want one:
```bash
# Check which stages have telemetry
ls docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-*/spec-*.json
```

If seeing plan+tasks+implement when you only ran `/spec-auto --from tasks`:
- Orchestrator not respecting `--from` flag
- Fix: Pass resume_from to orchestrator

---

### Issue: Policy Check Explosion

**Symptom:** 10+ agents all running policy checks

**Diagnosis:**
```bash
# Count policy agents
grep -l "Policy prefilter\|Policy final" /home/thetu/code/.code/agents/*/result.txt | wc -l

# Should be: 1-2 per stage (prefilter + final)
# If >12: Something spawning policy checks repeatedly
```

**Fix:** Check `spec_ops_plan.sh` - ensure policy checks run once

---

## Normal vs Excessive

### ✅ Normal: 24-30 agents for full pipeline

**6 stages × (3 consensus + 1 policy) = 24 base**
**+ 2-6 arbiter agents if conflicts = 26-30 total**

### ⚠️ Excessive: >40 agents

Indicates:
- Retry loops in orchestrator
- Duplicate stage execution
- Policy checks spawning multiple times
- Orchestrator misunderstanding instructions

### ✓ Optimal: 18-24 agents

**6 stages × 3 agents (if no conflicts, no policy checks) = 18**

---

## Action Plan

**Step 1:** Analyze current run
```bash
bash scripts/spec_ops_004/log_agent_runs.sh 180
```

**Step 2:** Check model distribution
- Should see gemini, claude, gpt-5 (not just gpt-5-codex)
- If all same: config issue

**Step 3:** Check for duplicates
- Look at "Detected Stage Patterns" section
- Should see: gemini × 6, claude × 6, gpt × 6
- If seeing × 12 or × 18: duplicate spawning

**Step 4:** Review orchestrator instructions
- Check if loop is running multiple times
- Look for retry logic
- Simplify if needed

**Next:** Run `log_agent_runs.sh` NOW and paste results - I'll diagnose.
