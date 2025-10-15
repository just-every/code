# Phase 3 Week 1 Day 4 - Testing Plan

**Date:** 2025-10-15
**Purpose:** Validate all /speckit.* commands and tiered model strategy
**Branch:** feat/spec-auto-telemetry
**Prerequisites:** Day 3 documentation complete

---

## Testing Objectives

1. **Command Validation:** Verify all 13 /speckit.* commands work correctly
2. **Tier Validation:** Confirm agent allocation matches tier specifications
3. **Backward Compatibility:** Ensure legacy /spec-* commands still work
4. **Cost Analysis:** Measure actual costs vs predicted (~$11 per pipeline)
5. **Performance Validation:** Confirm speed improvements (templates, parallel spawning)
6. **Evidence Capture:** Verify all telemetry and consensus artifacts created

---

## Test Environment

**Branch:** feat/spec-auto-telemetry
**Agents Required:** All 5 (gemini, claude, gpt_pro, gpt_codex, code)
**Config:** `~/.code/config.toml` with Phase 3 tiered strategy
**Test SPECs:** Use existing SPECs for validation (avoid creating new ones for testing)

---

## Test Suite

### Phase 1: Tier 0 Validation (Native TUI)

**Command:** `/speckit.status`
**Expected Behavior:**
- Response time: <1s
- No agent spawning
- Pure Rust implementation
- Cost: $0
- Displays: Stage completion, artifacts, evidence paths

**Test Cases:**
1. Run `/speckit.status SPEC-KIT-045-mini`
2. Verify instant response
3. Check output shows all 6 stages
4. Confirm artifact paths displayed
5. Validate no agents spawned (check logs)

**Success Criteria:**
- [ ] Response <1s
- [ ] No agent activity in logs
- [ ] Stage status accurate
- [ ] Evidence paths correct

---

### Phase 2: Tier 2-lite Validation (Dual Agent)

**Command:** `/speckit.checklist`
**Expected Behavior:**
- Agents: claude, code (2 total)
- Duration: 5-8 minutes
- Cost: ~$0.35
- Sequential mode: Claude → Code review

**Test Cases:**
1. Run `/speckit.checklist SPEC-KIT-065`
2. Monitor agent spawning (should see 2 agents only)
3. Time execution
4. Check evidence directory for 2 agent outputs
5. Verify synthesis JSON created

**Success Criteria:**
- [ ] Exactly 2 agents spawned
- [ ] Duration 5-8 min
- [ ] Cost ~$0.35
- [ ] Quality scores generated
- [ ] Evidence captured correctly

---

### Phase 3: Tier 2 Validation (Triple Agent)

**Commands:** `/speckit.clarify`, `/speckit.analyze`, `/speckit.plan`, `/speckit.tasks`

**Test Case 3a: /speckit.clarify**
- Agents: gemini, claude, code (3 total)
- Duration: 8-12 minutes
- Cost: ~$0.80
- Purpose: Identify ambiguities

**Steps:**
1. Run `/speckit.clarify SPEC-KIT-070`
2. Monitor 3 agents spawn
3. Time execution
4. Check evidence for 3 outputs + synthesis
5. Verify ambiguities identified

**Success Criteria:**
- [ ] 3 agents spawned (gemini, claude, code)
- [ ] Duration 8-12 min
- [ ] Ambiguities listed
- [ ] Evidence complete

**Test Case 3b: /speckit.analyze**
- Same tier validation
- Purpose: Cross-artifact consistency checking
- Should detect and suggest fixes

**Test Case 3c: /speckit.plan**
- Agents: gemini, claude, gpt_pro (3 total)
- Creates plan.md from PRD
- Consensus synthesis required

**Test Case 3d: /speckit.tasks**
- Agents: gemini, claude, gpt_pro (3 total)
- Creates tasks.md from plan
- Updates SPEC.md table

---

### Phase 4: Tier 3 Validation (Quad Agent)

**Command:** `/speckit.implement`
**Expected Behavior:**
- Agents: gemini, claude, gpt_codex, gpt_pro (4 total)
- Duration: 15-20 minutes
- Cost: ~$2.00
- Code ensemble: gpt_codex + claude (two-vote system)
- Arbiter: gpt_pro signs off

**Test Cases:**
1. Identify completed SPEC with plan.md and tasks.md ready
2. Run `/speckit.implement SPEC-KIT-###`
3. Monitor all 4 agents spawn
4. Time execution
5. Verify code ensemble (2 implementation proposals)
6. Check arbiter synthesis
7. Validate code quality

**Success Criteria:**
- [ ] 4 agents spawned
- [ ] Duration 15-20 min
- [ ] Two code proposals generated
- [ ] Arbiter selects/synthesizes
- [ ] Code validation passes (fmt, clippy)
- [ ] Evidence shows all 4 agent outputs

---

### Phase 5: Tier 4 Validation (Dynamic Multi-Agent)

**Command:** `/speckit.auto`
**Expected Behavior:**
- Dynamic 3-5 agents per stage
- Duration: 40-60 minutes
- Cost: ~$11 (40% reduction vs $15)
- Uses Tier 2 for most stages
- Uses Tier 3 for implement
- Adds arbiter if conflicts (<5%)

**Test Cases:**
1. Select small SPEC for full pipeline test
2. Run `/speckit.auto SPEC-KIT-###`
3. Monitor agent allocation per stage:
   - plan: 3 agents (gemini, claude, gpt_pro)
   - tasks: 3 agents
   - implement: 4 agents (gemini, claude, gpt_codex, gpt_pro)
   - validate: 3 agents
   - audit: 3 agents
   - unlock: 3 agents
4. Time full pipeline
5. Track costs per stage
6. Check automatic advancement
7. Verify evidence for all stages

**Success Criteria:**
- [ ] All 6 stages complete
- [ ] Agent counts match tier specs
- [ ] Total duration 40-60 min
- [ ] Total cost ~$11
- [ ] Automatic stage advancement works
- [ ] No manual intervention required
- [ ] Evidence complete for all stages

---

### Phase 6: Backward Compatibility Testing

**Legacy Commands:** `/new-spec`, `/spec-plan`, `/spec-tasks`, `/spec-auto`, `/spec-status`

**Test Cases:**
1. Run `/spec-status SPEC-KIT-045-mini` (legacy)
2. Verify works identically to `/speckit.status`
3. Run `/spec-plan SPEC-KIT-### --dry-run` (legacy)
4. Confirm routes to /speckit.plan internally
5. Check evidence paths match new namespace

**Success Criteria:**
- [ ] All legacy commands work
- [ ] Identical behavior to /speckit.* equivalents
- [ ] Evidence paths consistent
- [ ] No errors or warnings
- [ ] Deprecated but functional

---

### Phase 7: Cost Analysis

**Objective:** Validate 40% cost reduction claim ($15→$11)

**Methodology:**
1. Review evidence from /speckit.auto run
2. Calculate cost per stage:
   - 5 × Tier 2 stages: 5 × ~$1.00 = ~$5.00
   - 1 × Tier 3 stage: ~$2.00
   - Orchestration overhead: ~$2.00
   - Arbiter (if needed): ~$2.00
3. Sum total costs
4. Compare to baseline ($15 with 5 agents all stages)

**Success Criteria:**
- [ ] Total cost ~$11 ± $1
- [ ] 40% reduction vs baseline achieved
- [ ] Cost breakdown documented
- [ ] No unexpected agent spawns

---

### Phase 8: Performance Metrics

**Template Speed (Already Validated):**
- Baseline: 30 min for /new-spec equivalent
- Template: 13 min for /speckit.new
- Improvement: 55% faster ✅ (SPEC-KIT-060)

**Additional Metrics:**
1. Parallel agent spawning (vs sequential)
2. Native status query (<1s)
3. Full pipeline (40-60 min vs 96 min baseline)

**Validation:**
- [ ] Parallel spawning 30% faster (confirmed via logs)
- [ ] Status queries <1s (measured)
- [ ] Full pipeline 40-60 min (measured)

---

### Phase 9: Evidence Validation

**Directory Structure:**
```
docs/SPEC-OPS-004-integrated-coder-hooks/evidence/
├── commands/<SPEC-ID>/
│   ├── speckit-plan_*.json (guardrail telemetry)
│   ├── speckit-tasks_*.json
│   └── ...
└── consensus/<SPEC-ID>/
    ├── plan_*_gemini.json
    ├── plan_*_claude.json
    ├── plan_*_gpt_pro.json
    ├── plan_*_synthesis.json
    └── ...
```

**Validation:**
1. Check all telemetry files present
2. Verify schema v1 compliance
3. Confirm consensus synthesis for each stage
4. Validate model metadata in all files
5. Check local-memory entries created

**Success Criteria:**
- [ ] All telemetry files present
- [ ] Schema validation passes
- [ ] Consensus synthesis complete
- [ ] Model metadata correct
- [ ] Local-memory synced

---

## Known Behaviors to Verify

### Gemini Empty Output Handling
**Expected:** Occasional 1-byte result files (<5% of runs)
**Handling:** Orchestrator continues with 2/3 agents
**Validation:**
- [ ] If occurs, verify graceful degradation
- [ ] Check synthesis reports "degraded" status
- [ ] Confirm consensus still valid

### Conflict Resolution
**Expected:** <5% of stages require arbiter
**Handling:** Arbiter (`gpt-5 --reasoning high`) resolves
**Validation:**
- [ ] If occurs, verify arbiter spawned
- [ ] Check synthesis shows conflict + resolution
- [ ] Confirm exit code 0 after resolution

---

## Risk Mitigation

**Pre-Test Checklist:**
- [ ] Git tree clean (commit Day 3 docs first)
- [ ] All 5 agents configured in config.toml
- [ ] Agent credentials valid
- [ ] Local-memory MCP accessible
- [ ] Sufficient API credits for testing

**Rollback Plan:**
If major issues discovered:
1. Document specific failures
2. Check if config issue vs code issue
3. Revert to legacy /spec-* if needed
4. File issues for investigation
5. Do NOT merge to master

---

## Test Execution Order

**Recommended sequence:**
1. ✅ Tier 0 (status) - Quick validation
2. ✅ Backward compatibility - Ensure no breaking changes
3. ✅ Tier 2-lite (checklist) - Single command validation
4. ✅ Tier 2 (clarify, analyze) - Quality commands
5. ✅ Tier 2 (plan, tasks) - Core stages
6. ✅ Tier 3 (implement) - Code generation
7. ✅ Tier 4 (auto) - Full pipeline (longest test)
8. ✅ Cost analysis - Post-execution review
9. ✅ Evidence validation - Final verification

**Estimated Total Testing Time:** 3-4 hours

---

## Success Criteria Summary

**All tests must pass:**
- [ ] All 13 /speckit.* commands functional
- [ ] Agent allocation matches tier specifications
- [ ] Costs ~$11 per full pipeline (40% reduction)
- [ ] Performance meets targets (templates 55% faster)
- [ ] Backward compatibility maintained
- [ ] Evidence captured correctly
- [ ] No regressions vs pre-Phase 3

**Documentation Updates After Testing:**
- [ ] Update SPEC.md with Day 4 completion
- [ ] Update RESTART.md with test results
- [ ] Document any issues found
- [ ] Update cost analysis if different from prediction

---

## Post-Test Actions

**If all tests pass:**
1. Commit Day 3 documentation updates
2. Commit testing results
3. Update SPEC.md: Day 4 complete
4. Prepare for Day 5 (cleanup, final docs)

**If issues found:**
1. Document failures in detail
2. Triage severity (blocker vs minor)
3. Fix blockers before proceeding
4. Update documentation if behavior differs
5. Re-test after fixes

---

**Document Version:** 1.0
**Created:** 2025-10-15
**Owner:** @just-every/automation
**Status:** Ready for execution
