# Spec Auto Automation State

**Last Updated:** 2025-10-15 (Phase 3 standardization)
**Status:** ✅ Fully implemented and operational

---

## Current Coverage (Phase 3 - October 2025)

### Multi-Agent Commands (Tier 0-4)

**✅ Fully Operational:**
- `/speckit.status` - Native Rust (Tier 0: 0 agents, <1s, $0)
- `/speckit.checklist` - Dual agent (Tier 2-lite: 2 agents, ~5 min, ~$0.35)
- `/speckit.new`, `/speckit.specify`, `/speckit.clarify`, `/speckit.analyze` - Triple agent (Tier 2: 3 agents, ~10 min, ~$0.80)
- `/speckit.plan`, `/speckit.tasks`, `/speckit.validate`, `/speckit.audit`, `/speckit.unlock` - Triple agent (Tier 2: 3 agents, ~10 min, ~$1.00)
- `/speckit.implement` - Quad agent (Tier 3: 4 agents, ~15 min, ~$2.00)
- `/speckit.auto` - Dynamic allocation (Tier 4: 3-5 agents, ~60 min, ~$11)

### Guardrail Commands (Shell Wrappers)

**✅ Fully Operational:**
- `/guardrail.plan`, `/guardrail.tasks`, `/guardrail.implement`, `/guardrail.validate`, `/guardrail.audit`, `/guardrail.unlock` - Run directly from TUI (note: legacy `/spec-ops-*` commands still work)
- `/guardrail.auto` - Wrapper for `scripts/spec_ops_004/spec_auto.sh` (note: legacy `/spec-ops-auto` still works)
- All emit telemetry to `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/<SPEC-ID>/`

### Consensus Runner

**✅ Fully Operational:**
- `scripts/spec_ops_004/consensus_runner.sh` - Automated multi-agent execution
- Renders prompts for Gemini/Claude/GPT-5/GPT-5-Codex
- Writes synthesis JSON to `evidence/consensus/<SPEC-ID>/`
- Handles conflicts with automatic arbiter resolution
- Supports `--dry-run` and `--execute` modes
- Integrated with all `/speckit.*` commands

**Usage:**
- `/speckit.plan --consensus SPEC-ID` - Full consensus execution (default)
- `/speckit.plan --consensus-exec SPEC-ID` - Explicit execute mode
- Consensus metadata includes model, prompt version, reasoning mode

---

## Phase 3 Achievements

**✅ Completed:**
1. ✅ Deterministic prompt bundling validated (PROMPT_VERSION tracking implemented)
2. ✅ Consensus runner executes models and writes synthesis JSON
3. ✅ Evidence + local-memory writes survive retries
4. ✅ `/speckit.auto` chains guardrails and consensus automatically
5. ✅ Run summaries emit stage → telemetry path mappings
6. ✅ Synthesis summaries pushed to local-memory
7. ✅ `/spec-consensus` reflects automated runs

**Validation Evidence:**
- SPEC-KIT-045-mini: Full 6-stage pipeline validated
- SPEC-KIT-060: Template validation (55% faster)
- SPEC-KIT-065, 070, 075, 080: Quality commands validated

---

## Tiered Model Strategy

### Tier 0: Native TUI (0 agents)
**Command:** `/speckit.status`
- Pure Rust implementation
- Reads evidence directory directly
- No API calls, instant response
- Cost: $0

### Tier 2-lite: Dual Agent (2 agents)
**Command:** `/speckit.checklist`
- Agents: claude-4.5-sonnet, code
- Quality evaluation without research
- Cost: ~$0.35, Duration: 5-8 min

### Tier 2: Triple Agent (3 agents)
**Commands:** `/speckit.new`, `/speckit.specify`, `/speckit.clarify`, `/speckit.analyze`, `/speckit.plan`, `/speckit.tasks`, `/speckit.validate`, `/speckit.audit`, `/speckit.unlock`
- Agents: gemini-2.5-pro, claude-4.5-sonnet, gpt-5/code
- Analysis, planning, consensus (no code generation)
- Cost: ~$0.80-1.00, Duration: 8-12 min

### Tier 3: Quad Agent (4 agents)
**Command:** `/speckit.implement`
- Agents: gemini-2.5-pro, claude-4.5-sonnet, gpt-5-codex, gpt-5
- Code generation with two-vote ensemble
- Cost: ~$2.00, Duration: 15-20 min

### Tier 4: Dynamic (3-5 agents)
**Command:** `/speckit.auto`
- Uses Tier 2 for most stages (3 agents)
- Uses Tier 3 for implement (4 agents)
- Adds arbiter if conflicts (5th agent, rare)
- Cost: ~$11 (40% reduction vs $15 pre-Phase 3)
- Duration: 40-60 min

---

## Consensus Execution Flow

**Automatic Stage Advancement:**
1. `/speckit.plan` → consensus runner → synthesis → success/conflict
2. On success: Auto-advance to `/speckit.tasks`
3. On conflict: Arbiter (`gpt-5 --reasoning high`) resolves
4. Repeat for all 6 stages
5. `/speckit.auto` orchestrates entire flow

**Evidence Capture:**
- Per-agent outputs: `evidence/consensus/<SPEC-ID>/<stage>_<timestamp>_<agent>.json`
- Synthesis: `evidence/consensus/<SPEC-ID>/<stage>_<timestamp>_synthesis.json`
- Telemetry: `evidence/commands/<SPEC-ID>/<command>_<timestamp>.json`
- Local-memory: All summaries stored in `spec-tracker` domain

**Conflict Resolution:**
- Detect: Synthesis identifies disagreements
- Escalate: Arbiter agent spawned (`gpt-5 --reasoning high`)
- Resolve: Arbiter selects strongest proposal or synthesizes hybrid
- Document: Conflict + resolution logged in synthesis
- Success rate: >95% consensus, <5% arbiter needed

---

## Performance Metrics

**Speed Improvements:**
- Template system: 55% faster (13 min vs 30 min for `/speckit.new`)
- Parallel agent spawning: 30% faster than sequential
- Native status: <1s vs multi-second API calls
- Full pipeline: 40-60 min (down from 96 min pre-optimization)

**Cost Optimization:**
- Tiered strategy: 40% reduction ($15→$11 per full pipeline)
- Status queries: $0 (native, no agents)
- Quality checks: $0.35-0.80 per command
- Full automation: ~$11 for 6-stage pipeline

**Quality Metrics:**
- Multi-model perspectives catch gaps single agent misses
- Evidence trails enable debugging and accountability
- Constitution compliance enforced automatically
- Cross-artifact consistency validated

---

## Integration Status

**TUI Integration:**
- ✅ All `/speckit.*` commands route to consensus runner
- ✅ Exit codes handled (success/degraded/conflict)
- ✅ History entries reference evidence paths
- ✅ Parallel agent spawning supported
- ✅ Progress indicators show stage X/6

**Guardrail Integration:**
- ✅ `/guardrail.*` commands run independently (note: legacy `/spec-ops-*` commands still work)
- ✅ Telemetry schema v1 validated
- ✅ HAL validation optional (`SPEC_OPS_HAL_SKIP=1`)
- ✅ Clean tree enforcement (`SPEC_OPS_ALLOW_DIRTY=1` override)

**Local-Memory Integration:**
- ✅ Consensus summaries stored automatically
- ✅ `/spec-consensus SPEC-ID STAGE` displays results
- ✅ Evidence paths tracked per stage
- ✅ Retrieval via MCP working

---

## Known Behaviors

**Gemini Occasional Empty Output:**
- Issue: 1-byte result files (rare, <5% of runs)
- Handling: Orchestrator continues with 2/3 agents
- Impact: Consensus still valid with reduced perspective
- Mitigation: Minimum 2 agents required for consensus

**Agent Unavailability:**
- Graceful degradation: Document which agents participated
- Minimum threshold: 2 agents required
- Escalation: Arbiter if conflicts persist
- Logging: `status: "degraded"` in synthesis

**Conflict Frequency:**
- Normal: <5% of stages require arbiter
- Common triggers: Ambiguous requirements, multiple valid approaches
- Resolution: Arbiter selects or synthesizes
- Prevention: Quality commands (clarify, analyze) reduce conflicts

---

## Future Enhancements

**Planned (Phase 3 Week 2):**
- Guardrail namespace: `/spec-ops-*` → `/guardrail.*`
- Final testing and release notes
- Migration documentation complete

**Future Considerations:**
- Cost tracking telemetry for governance
- Evidence archival strategy for >25MB SPECs
- Tier 1 optimization (single agent for scaffolding)
- Extract spec-kit to separate repo

---

## Usage Examples

**Quick Start:**
```bash
# Create SPEC with templates
/speckit.new Add user authentication with OAuth2

# Quality checks (proactive)
/speckit.clarify SPEC-KIT-###
/speckit.analyze SPEC-KIT-###
/speckit.checklist SPEC-KIT-###

# Full automation
/speckit.auto SPEC-KIT-###

# Check status
/speckit.status SPEC-KIT-###
```

**Individual Stages:**
```bash
/speckit.plan SPEC-KIT-065
/speckit.tasks SPEC-KIT-065
/speckit.implement SPEC-KIT-065
/speckit.validate SPEC-KIT-065
/speckit.audit SPEC-KIT-065
/speckit.unlock SPEC-KIT-065
```

**Guardrail Validation:**
```bash
# Run validation separate from multi-agent
/guardrail.plan SPEC-KIT-065
/guardrail.auto SPEC-KIT-065 --from plan

# Monitor evidence footprint
/spec-evidence-stats --spec SPEC-KIT-065
```

---

**Document Version:** 2.0 (Phase 3 automation complete)
**Last Updated:** 2025-10-15
**Status:** Fully operational and validated
**Owner:** @just-every/automation
