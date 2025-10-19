# Spec-Auto Orchestrator Design (Visible Execution)

## Goal
Full pipeline automation WITH visibility - see all agent work, no background black boxes.

## Architecture

**Hybrid approach:**
- Bash scripts: Guardrails, telemetry, evidence capture (opaque but necessary)
- Native agents: Consensus (visible, interruptible, transparent)

## Orchestrator Instructions (Prototype)

```toml
[[subagents.commands]]
name = "spec-auto"
agents = ["code"]
orchestrator-instructions = """
Execute complete spec-kit pipeline for {SPEC_ID} with full visibility.

CRITICAL: Show ALL work. User must see agent execution, consensus building, validation.

=== STAGE LOOP ===

For each stage in [plan, tasks, implement, validate, audit, unlock]:

**Step 1: Guardrail Validation**
Run: bash scripts/spec_ops_004/commands/spec_ops_{stage}.sh {SPEC_ID}
Report: "Guardrail {stage}: <exit code> - <any errors>"
If exit != 0: HALT with error message

**Step 2: Load Prompts**
Read: docs/spec-kit/prompts.json
Extract: prompts for "spec-{stage}" stage
You'll find: gemini, claude, gpt_pro prompts with template variables

**Step 3: Prepare Context**
Read files for context injection:
- docs/{SPEC_ID}/spec.md (if exists)
- docs/{SPEC_ID}/PRD.md
- docs/{SPEC_ID}/plan.md (for tasks stage onward)
- product-requirements.md
- PLANNING.md

**Step 4: Spawn Consensus Agents (VISIBLE)**

Gemini (Research):
  agent_run:
    name: gemini-{stage}
    model: gemini-2.5-pro
    reasoning: thinking
    prompt: <inject SPEC_ID and CONTEXT into gemini prompt template>

Wait for Gemini: agent_wait name=gemini-{stage}
Get result: agent_result name=gemini-{stage}

Claude (Synthesis):
  agent_run:
    name: claude-{stage}
    model: claude-4.5-sonnet
    reasoning: auto
    prompt: <inject CONTEXT + GEMINI_OUTPUT into claude template>

Wait for Claude: agent_wait name=claude-{stage}
Get result: agent_result name=claude-{stage}

GPT (Validation):
  agent_run:
    name: gpt-{stage}
    model: gpt-5
    reasoning: high
    prompt: <inject CONTEXT + ALL_OUTPUTS into gpt template>

Wait for GPT: agent_wait name=gpt-{stage}
Get result: agent_result name=gpt-{stage}

**Step 5: Synthesize Consensus**

Compare outputs:
- Extract agreements (topics all agents agree on)
- Extract conflicts (disagreements between agents)

Create synthesis JSON:
{
  "stage": "spec-{stage}",
  "status": "ok" | "conflict" | "degraded",
  "consensus": {
    "agreements": [list of consensus points],
    "conflicts": [list of disagreements]
  }
}

Write: docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/{SPEC_ID}/spec-{stage}_<timestamp>_synthesis.json

**Step 6: Validate & Advance**

If status == "conflict":
  HALT
  Show: "⚠ Conflicts detected in {stage}:"
  List each conflict
  Ask: "How should we resolve? (manual review required)"

If status == "degraded":
  Warn: "⚠ Some agents failed in {stage}"
  Ask: "Continue anyway?"

If status == "ok":
  Report: "✓ {stage} consensus validated - {n} agreements"
  Continue to next stage automatically

**Step 7: Progress Report**

After each stage: "Completed {n}/6 stages: {list}. Next: {next_stage}"

=== END LOOP ===

Final: "Pipeline complete for {SPEC_ID}. All stages validated with multi-agent consensus."

IMPORTANT EXECUTION RULES:
- NEVER hide bash execution in background
- ALWAYS show agent spawning ("Launching Gemini for research...")
- ALWAYS wait visibly for agents
- ALWAYS show synthesis results
- HALT immediately on conflicts (don't auto-continue)
"""
```

## Complexity Analysis

**Orchestrator must:**
1. Parse JSON (prompts.json) ✅ Doable via Read + JSON parsing
2. String template injection ✅ Simple string replacement
3. Call bash scripts ✅ Via Bash tool
4. Spawn agents ✅ Via agent_run
5. Synthesize outputs ✅ LLM can compare and extract
6. Write JSON files ✅ Via Write tool

**None of this is impossible.** It's just LONG instructions.

## What Gets Reused vs Rewritten

**Keep (bash):**
- ✅ Guardrail scripts (baseline, HAL, policy)
- ✅ Telemetry schema v1 (written by guardrails)
- ✅ Evidence directory structure
- ✅ check_synthesis.py validation

**Replace (consensus_runner.sh → orchestrator):**
- Prompt rendering (150 lines bash → 20 lines orchestrator logic)
- Agent execution (200 lines bash → agent_run tool calls)
- Synthesis (50 lines Python → orchestrator comparison)
- Context collection (100 lines bash → Read tool calls)

**Total bash replaced:** 500 lines
**Orchestrator complexity:** ~300 lines of instructions

## Effort Comparison

**Option B (Orchestrator):** 2-3 days
- Design orchestrator instructions (1 day)
- Test with real SPEC (1 day)
- Debug edge cases (1 day)

**Option A (TUI):** 3 days
- Rust implementation (2 days)
- Testing (1 day)

**Both are similar effort**, but orchestrator has ZERO rebase friction.

## The Real Question

**Can orchestrator instructions be 300 lines and still work reliably?**

Looking at your existing `/implement` command - it's already complex:
- Collect diffs from agents
- Evaluate proposals
- Run validation commands
- Update SPEC.md

And it works.

So yes, complex orchestrator instructions ARE viable in your setup.

## My Actual Recommendation

**Prototype Option B** (Orchestrator hybrid) because:

1. **Your priorities:** Simplicity + upstream alignment
2. **Config-only:** No Rust changes
3. **Similar effort:** 2-3 days vs 3 days TUI
4. **Full visibility:** Agent execution visible
5. **Testable faster:** Change instructions, reload config, retry

If orchestrator approach fails or gets too complex → fall back to TUI implementation.

**Want me to implement the orchestrator design?**