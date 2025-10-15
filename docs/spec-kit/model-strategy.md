# Spec Kit Model Strategy

This document records the canonical mapping between Spec Kit command stages and
the language models they invoke. Update this file whenever models change so the
consensus guardrails and offline fallbacks stay aligned.

**Last Updated:** 2025-10-15 (Phase 3 standardization)

---

## Tiered Model Strategy (Phase 3 - October 2025)

### Overview

Phase 3 introduces a **tiered model strategy** that right-sizes agent allocation based on task complexity. This achieves **40% cost reduction** ($15→$11 per full pipeline) while maintaining quality through specialized agent roles.

**Tiers:**
- **Tier 0:** Native TUI (0 agents) - Instant status queries
- **Tier 2-lite:** Dual agent (2 agents) - Quality evaluation
- **Tier 2:** Triple agent (3 agents) - Analysis and planning
- **Tier 3:** Quad agent (4 agents) - Code generation
- **Tier 4:** Dynamic (3-5 agents) - Full automation with adaptive allocation

---

## Tier 0: Native TUI (0 agents)

**Command:** `/speckit.status`

**Implementation:**
- Pure Rust implementation in `codex-rs/tui/src/spec_status.rs`
- Reads evidence directory directly
- No API calls, no agents

**Performance:**
- Response time: <1s
- Cost: $0
- Token usage: 0

**Use Case:** Instant status checks during development

---

## Tier 2-lite: Dual Agent (2 agents)

**Command:** `/speckit.checklist`

**Agents:**
- **Synthesizer:** `claude-4.5-sonnet` (requirement analysis)
- **Evaluator:** `code` (Claude Code - scoring and validation)

**Purpose:** Quality evaluation without research needs

**Performance:**
- Duration: 5-8 minutes
- Cost: ~$0.35
- Agent mode: Sequential (Claude → Code review)

**Use Case:** Requirement quality scoring before automation

---

## Tier 2: Triple Agent (3 agents)

**Commands:** `/speckit.new`, `/speckit.specify`, `/speckit.clarify`, `/speckit.analyze`, `/speckit.plan`, `/speckit.tasks`, `/speckit.validate`, `/speckit.audit`, `/speckit.unlock`

**Agents:**
- **Research:** `gemini-2.5-pro`
  - Breadth and exploration
  - Tool use and context gathering
  - Flash mode for quick pre-scans
- **Synthesizer:** `claude-4.5-sonnet`
  - Precision and analysis
  - Stronger coding/agent performance
  - Long autonomous sessions
- **Arbiter:** `gpt-5` or `code`
  - Conflict resolution
  - Escalate with `--reasoning high` on disagreements
  - Policy enforcement

**Performance:**
- Duration: 8-12 minutes
- Cost: ~$0.80-1.00
- Agent mode: Parallel spawn → Consensus synthesis

**Guardrails:** Layered: `gpt-5-codex` prefilter → `gpt-5` policy gate

**Use Case:** Analysis, planning, consensus (no code generation)

**Agent Allocation by Command:**
- `/speckit.new`, `/speckit.specify`, `/speckit.clarify`, `/speckit.analyze`: gemini, claude, code
- `/speckit.plan`, `/speckit.tasks`, `/speckit.validate`, `/speckit.audit`, `/speckit.unlock`: gemini, claude, gpt_pro

---

## Tier 3: Quad Agent (4 agents)

**Command:** `/speckit.implement` (code generation only)

**Agents:**
- **Research:** `gemini-2.5-pro`
  - Retrieve refs, APIs, prior art
  - Context gathering for implementation
- **Code Ensemble (two-vote system):**
  - `gpt-5-codex` (OpenAI implementation)
  - `claude-4.5-sonnet` (Anthropic implementation)
  - Both generate diffs independently
- **Arbiter:** `gpt-5` with `--reasoning high`
  - Signs off merges
  - Selects strongest implementation
  - Combines best elements from both

**Performance:**
- Duration: 15-20 minutes
- Cost: ~$2.00
- Agent mode: Parallel ensemble → Synthesis → Validation

**Guardrails:** `gpt-5-codex` → `gpt-5`

**Use Case:** Code generation with validation (most expensive tier)

**Why Quad?** Code generation benefits from diverse tool stacks (OpenAI + Anthropic) producing stronger diffs than single-agent approaches.

---

## Tier 4: Dynamic (3-5 agents adaptively)

**Command:** `/speckit.auto` (full 6-stage pipeline)

**Strategy:**
- **Most stages:** Use Tier 2 (3 agents)
  - plan, tasks, validate, audit, unlock
- **Code generation:** Use Tier 3 (4 agents)
  - implement stage only
- **Conflict resolution:** Add arbiter agent dynamically
  - +1 agent if consensus fails
  - `gpt-5 --reasoning high` for adjudication

**Orchestration:**
- Automatic stage advancement (no human gates)
- Conflict detection per stage
- Arbiter invoked only when needed (rare, <5% of runs)

**Performance:**
- Duration: 40-60 minutes (6 stages)
- Cost: ~$11 (down from $15 pre-Phase 3)
- Agent mode: Adaptive per stage

**Guardrails:** Full validation stack reused per stage

**Use Case:** Full automation from plan → unlock

**Cost Breakdown:**
- 5 × Tier 2 stages: 5 × $1.00 = $5.00
- 1 × Tier 3 stage (implement): $2.00
- Orchestration overhead: ~$2.00
- Arbiter (if needed): ~$2.00
- **Total:** ~$11 (40% reduction vs arbitrary 5-agent-per-stage)

---

## Model Responsibilities

**Gemini 2.5 Pro (Research Agent):**
- Strengths: Breadth, tool use, wide context windows
- Used in: Tier 2, Tier 3, Tier 4
- Purpose: Exploration, research, context gathering
- Flash mode: Quick pre-scans when full Pro unnecessary

**Claude 4.5 Sonnet (Synthesizer):**
- Strengths: Precision, code quality, autonomous sessions
- Used in: All tiers (2-lite, 2, 3, 4)
- Purpose: Analysis, synthesis, code quality
- Default synthesizer across all commands

**GPT-5 (Arbiter/Validator):**
- Strengths: Conflict resolution, policy enforcement
- Used in: Tier 2, Tier 3, Tier 4
- Purpose: Validation, arbitration, quality checks
- Escalation: `--reasoning high` for complex conflicts

**GPT-5-Codex (Code Generator):**
- Strengths: Code generation, implementation, diffs
- Used in: Tier 3, Tier 4, Guardrails
- Purpose: Code ensemble, prefilter validation
- Combines with Claude for two-vote system

**Code (Claude Code - General Purpose):**
- Strengths: Orchestration, fallback, broad capability
- Used in: All tiers
- Purpose: General-purpose tasks, orchestration
- Fallback when specialized agents unavailable

---

## Phase 3 Improvements (October 2025)

**Cost Optimization:**
- 40% reduction: $15→$11 per full pipeline
- Right-sized agent allocation (0-4 agents per command)
- Native Tier 0 for status queries ($0)

**Performance Gains:**
- Template system: 55% faster generation (13 min vs 30 min)
- Parallel agent spawning: 30% faster than sequential
- Native status: <1s (instant feedback)

**Quality Enhancements:**
- Code ensemble (Tier 3): Two-vote system for stronger implementations
- Proactive quality commands: clarify, analyze, checklist
- Cross-artifact consistency validation

**Strategic Decisions:**
- Claude Sonnet 4.5: Default synthesizer (better autonomy, coding)
- GPT-5: Universal arbiter (escalate with `--reasoning high`)
- Gemini 2.5 Pro: Research agent (Flash for quick scans)
- Tier 0: Pure Rust for zero-cost status

---

## Escalation Rules

**Consensus degraded** (`missing_agents` or conflicts):
- Rerun stage with `gemini-2.5-pro` (thinking budget 0.6)
- Reissue arbiter call with `gpt-5 --reasoning high`
- Document degradation in consensus metadata

**Thinking budget exhausted:**
- Promote `gemini-2.5-flash` to Pro
- Log retry in consensus metadata
- Alert if budget exceeded repeatedly

**Guardrail parsing failure:**
- Retry with `gpt-5-codex`
- If still failing, escalate to `gpt-5` (low effort)
- Tag verdict with `guardrail_escalated=true`

**Agent unavailability:**
- Gemini occasional empty output (1-byte results): Continue with 2/3 agents
- Minimum 2 agents required for consensus
- Document which agents participated
- Proceed with available subset

**Offline mode:**
- Use on-prem fallbacks (documented in operational runbooks)
- Record `"offline": true` in consensus metadata
- Alert monitoring systems

---

## Prompt Metadata Requirements

Every agent response used for consensus must include:

```json
{
  "model": "<provider-model-id>",
  "model_release": "YYYY-MM-DD",
  "prompt_version": "YYYYMMDD-stage-suffix",
  "reasoning_mode": "fast|thinking|auto",
  "consensus": { "agreements": [], "conflicts": [] }
}
```

**Validation:**
- Consensus checker rejects artifacts missing these fields
- Model must match canonical lineup (documented in this file)
- Prompt versions follow `YYYYMMDD-stage-suffix` convention
- Prompts live in `docs/spec-kit/prompts.json`

**Version Updates:**
- Increment version string when prompts change
- Document changes affecting consensus or evidence interpretation
- Maintain backward compatibility where possible

---

## Command → Tier Mapping (Quick Reference)

**Tier 0 (Native):**
- `/speckit.status`

**Tier 2-lite (Dual):**
- `/speckit.checklist`

**Tier 2 (Triple):**
- `/speckit.new`
- `/speckit.specify`
- `/speckit.clarify`
- `/speckit.analyze`
- `/speckit.plan`
- `/speckit.tasks`
- `/speckit.validate`
- `/speckit.audit`
- `/speckit.unlock`

**Tier 3 (Quad):**
- `/speckit.implement`

**Tier 4 (Dynamic):**
- `/speckit.auto`

**Guardrails (Shell):**
- `/spec-ops-*` commands (use gpt-5-codex → gpt-5 layering)

---

## Validation Checklist

**Integration tests cover:**
- [ ] Degraded consensus (missing agents, conflicts)
- [ ] Thinking-budget retries
- [ ] Guardrail parsing parity (`gpt-5-codex` vs `gpt-5`)
- [ ] Tier 0 native status (no agents)
- [ ] Tier 2-lite dual agent (claude + code)
- [ ] Tier 3 code ensemble (gpt-5-codex + claude)
- [ ] Tier 4 dynamic allocation per stage

**Operational monitoring:**
- `/speckit.auto` run summaries include chosen model IDs
- Cost alerts when `gpt-5 --reasoning high` exceeds thresholds
- `gemini-2.5-pro` thinking budget tracking
- Agent availability monitoring
- Cost per pipeline tracking (target: ~$11)

**Evidence requirements:**
- Model metadata in all consensus artifacts
- Prompt versions tracked per stage
- Degradation events logged with context
- Cost breakdown per run

---

## Migration Notes

**Legacy `/spec-*` commands:** Still functional, map to `/speckit.*` equivalents with same tier allocation.

**Backward compatibility:** All existing prompts and configurations work with new tiered strategy.

**Agent config:** Update `~/.code/config.toml` with 5 agent types:
- gemini (Gemini 2.5 Pro)
- claude (Claude 4.5 Sonnet)
- gpt_pro (GPT-5)
- gpt_codex (GPT-5-Codex)
- code (Claude Code)

**Cost tracking:** Update monitoring to track tier usage and cost per command.

---

**Document Version:** 2.0 (Phase 3 tiered strategy)
**Last Updated:** 2025-10-15
**Status:** Current and authoritative
**Owner:** @just-every/automation
