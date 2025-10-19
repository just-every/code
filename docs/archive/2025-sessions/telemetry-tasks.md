# Telemetry Tasks - Multi-Model Strategy Validation

> Purpose: Add comprehensive telemetry to validate model selection strategy and optimize consensus workflow based on empirical data.

## Phase 1: Foundation (Weeks 1-2)

### Task 1.1: Extend Telemetry Schema v2
**Owner:** Code
**Dependencies:** None
**Effort:** 2-3 days

Add per-stage and per-SPEC metrics to telemetry schema:

```json
{
  "schemaVersion": "2.0",
  "command": "spec-plan",
  "specId": "SPEC-KIT-XXX",
  "sessionId": "...",
  "timestamp": "...",

  "consensus": {
    "agents": [
      {
        "agent": "gemini",
        "model_id": "gemini-2.5-pro",
        "reasoning_mode": "thinking",
        "prompt_tokens": 1234,
        "completion_tokens": 567,
        "latency_ms": 4532,
        "cost_usd": 0.12
      },
      {
        "agent": "claude",
        "model_id": "claude-4.5-sonnet",
        "reasoning_mode": "auto",
        "prompt_tokens": 1234,
        "completion_tokens": 789,
        "latency_ms": 3210,
        "cost_usd": 0.15
      },
      {
        "agent": "gpt_pro",
        "model_id": "gpt-5",
        "reasoning_mode": "high",
        "prompt_tokens": 2345,
        "completion_tokens": 890,
        "latency_ms": 8765,
        "cost_usd": 0.45,
        "arbiter_override": false,
        "override_reason": null
      }
    ],
    "disagreement_detected": false,
    "disagreement_points": [],
    "escalation_triggered": false,
    "escalation_reason": null,
    "synthesis_status": "ok",
    "total_cost_usd": 0.72,
    "total_latency_ms": 16507
  },

  "quality_metrics": {
    "completeness_score": null,
    "human_review_score": null,
    "automated_checks_passed": 12,
    "automated_checks_failed": 0
  }
}
```

**Acceptance Criteria:**
- [x] Schema v2 documented in `docs/spec-kit/telemetry-schema-v2.md`
- [x] Backwards compatible with schema v1
- [ ] Validator script updated: `scripts/spec_ops_004/validate_telemetry.py`
- [ ] Example fixtures added for testing

---

### Task 1.2: Instrument Consensus Runner
**Owner:** Code
**Dependencies:** Task 1.1
**Effort:** 3-4 days

Modify `consensus_runner.sh` to capture and emit telemetry:

**Changes needed:**
1. Track start/end time per agent call
2. Parse token counts from Codex CLI output
3. Capture raw token counts (cost calculation optional)
4. Detect disagreements by comparing agent outputs
5. Record arbiter overrides when GPT changes synthesis
6. Write telemetry alongside synthesis JSON

**Implementation:**
```bash
# In run_agent() after model execution:
extract_metrics() {
  local output_file="$1"
  local start_time="$2"
  local end_time="$3"

  # Parse Codex CLI --json output for usage stats
  prompt_tokens=$(jq '.usage.prompt_tokens' "$output_file")
  completion_tokens=$(jq '.usage.completion_tokens' "$output_file")

  # Record metrics
  echo "$metrics_json" >> "${telemetry_file}"
}
```

**Acceptance Criteria:**
- [x] Telemetry JSON written to `evidence/consensus/<SPEC-ID>/<stage>_<timestamp>_telemetry.json`
- [x] All agent metrics captured (tokens, cost, latency)
- [x] Disagreement detection logic implemented
- [x] Unit tests for metric extraction functions

---

## Phase 2: Collection & Analysis (Weeks 3-4)

### Task 2.1: Telemetry Aggregation Tool
**Owner:** Code
**Dependencies:** Task 1.2
**Effort:** 2-3 days

Build tool to aggregate telemetry across multiple SPECs:

**Script:** `scripts/spec_ops_004/analyze_telemetry.py`

```python
# Usage examples:
# Show cost breakdown by stage
./analyze_telemetry.py --metric cost --group-by stage

# Find disagreement patterns
./analyze_telemetry.py --metric disagreements --min-count 1

# Compare single-model vs consensus runs
./analyze_telemetry.py --compare SPEC-A SPEC-B --metric quality

# Generate cost report
./analyze_telemetry.py --report cost --output cost-report.html
```

**Features:**
- Parse all telemetry JSON files in evidence directories
- Aggregate metrics by SPEC, stage, model, date range
- Generate summary statistics (mean, median, p95)
- Identify outliers and anomalies
- Export to CSV/JSON for further analysis

**Acceptance Criteria:**
- [ ] CLI tool with subcommands for common queries
- [ ] HTML report generation
- [ ] CSV export for spreadsheet analysis
- [ ] Documentation with usage examples

---

### Task 2.2: Disagreement Analyzer
**Owner:** Code
**Dependencies:** Task 1.2
**Effort:** 2-3 days

Tool to categorize and analyze model disagreements:

**Script:** `scripts/spec_ops_004/analyze_disagreements.py`

**Analysis dimensions:**
1. **Frequency:** How often do models disagree?
2. **Severity:** Cosmetic vs. structural vs. logical disagreements
3. **Stage patterns:** Which stages have most disagreements?
4. **Model pairs:** Which model combinations disagree most?
5. **Resolution:** How often does arbiter side with each model?

**Output:**
```yaml
disagreement_summary:
  total_consensus_runs: 15
  runs_with_disagreements: 4
  disagreement_rate: 0.267

  by_stage:
    spec-plan: 2
    spec-tasks: 1
    spec-implement: 0
    spec-validate: 1
    spec-audit: 0
    spec-unlock: 0

  by_model_pair:
    gemini_vs_claude: 3
    gemini_vs_gpt: 2
    claude_vs_gpt: 1

  arbiter_decisions:
    sided_with_gemini: 1
    sided_with_claude: 2
    sided_with_gpt: 0
    synthesized_new: 1

  disagreement_types:
    structural: 2  # Different plan structure
    scope: 1        # Different task breakdown
    technical: 1    # Different implementation approach
```

**Acceptance Criteria:**
- [ ] Categorizes disagreements by type
- [ ] Tracks arbiter decision patterns
- [ ] Identifies high-disagreement stages
- [ ] Generates actionable insights

---

### Task 2.3: Quality Metrics Dashboard
**Owner:** Code
**Dependencies:** Task 2.1
**Effort:** 3-4 days

Web dashboard for telemetry visualization:

**Tech stack:** Simple Python + Flask + Chart.js (or static HTML + D3.js)

**Views:**
1. **Cost Overview**
   - Total spend by SPEC, stage, model
   - Trend over time
   - Cost per quality point (if quality data available)

2. **Performance Metrics**
   - Latency by model and reasoning mode
   - Token usage patterns
   - Cache hit rates (if applicable)

3. **Consensus Health**
   - Disagreement rates over time
   - Escalation frequency
   - Arbiter override patterns

4. **Model Comparison**
   - Side-by-side model performance
   - Cost vs. quality scatter plots
   - Recommendation: which models to use when

**Acceptance Criteria:**
- [ ] Dashboard accessible via `scripts/spec_ops_004/dashboard.py`
- [ ] Auto-refreshes from latest telemetry
- [ ] Exportable charts (PNG/SVG)
- [ ] Shareable reports

---

## Phase 3: Validation Experiments (Weeks 5-6)

### Task 3.1: Single vs. Multi-Model Comparison
**Owner:** Gemini + Claude + GPT (consensus)
**Dependencies:** Phase 2 complete
**Effort:** 1 week

**Experiment design:**
- Select 3 representative SPECs (simple, medium, complex)
- Run each SPEC twice:
  - **Variant A:** Claude-only (research + synthesis + self-critique)
  - **Variant B:** Full 3-model consensus (Gemini + Claude + GPT)
- Blind review both outputs
- Measure: quality, cost, time, error detection

**Quality criteria:**
- Completeness: All requirements addressed
- Correctness: Technically sound recommendations
- Clarity: Well-structured, easy to follow
- Actionability: Clear next steps

**Telemetry to capture:**
```json
{
  "experiment": "single_vs_multi_model",
  "spec_id": "SPEC-EXP-001",
  "variant": "A",  // or "B"
  "models_used": ["claude"],
  "total_cost_usd": 2.34,
  "total_time_seconds": 145,
  "quality_scores": {
    "completeness": 8.5,
    "correctness": 9.0,
    "clarity": 8.0,
    "actionability": 8.5,
    "overall": 8.5
  },
  "errors_detected": 2,
  "reviewer": "human_1"
}
```

**Acceptance Criteria:**
- [ ] 6 SPEC runs completed (3 SPECs × 2 variants)
- [ ] Blind review scores collected
- [ ] Statistical analysis of quality delta
- [ ] Cost/benefit analysis documented
- [ ] Recommendation: keep or simplify consensus

---

### Task 3.2: Gemini Necessity Test
**Owner:** Gemini + Claude (consensus)
**Dependencies:** Task 3.1
**Effort:** 3-4 days

**Experiment design:**
- Run spec-plan stage only
- Variant A: Gemini (research) → Claude (synthesis)
- Variant B: Claude (research + synthesis)
- Compare research depth and plan quality

**Specific tests:**
1. **Context window utilization:** Does Gemini's 2M context get used?
2. **Research breadth:** Does Gemini find more relevant context?
3. **Tool use comparison:** Equal tool calling capabilities?
4. **Output quality:** Does research quality affect plan quality?

**Acceptance Criteria:**
- [ ] Token usage analysis shows if 2M context is needed
- [ ] Research artifact comparison (breadth, depth, relevance)
- [ ] Quality scores for resulting plans
- [ ] Recommendation: keep Gemini or consolidate to Claude

---

### Task 3.3: Arbiter Value Measurement
**Owner:** Claude + GPT (consensus)
**Dependencies:** Task 3.1
**Effort:** 3-4 days

**Experiment design:**
- Run spec-implement and spec-validate stages
- Variant A: Claude self-critique (no arbiter)
- Variant B: GPT arbitration (current approach)
- Measure error detection rates

**Specific tests:**
1. **Override frequency:** How often does GPT change Claude's output?
2. **Override value:** Do GPT's changes improve quality?
3. **Error detection:** Does GPT catch bugs Claude missed?
4. **False positives:** Does GPT introduce errors?

**Tracking:**
```json
{
  "experiment": "arbiter_value",
  "stage": "spec-implement",
  "variant": "B",
  "arbiter_invoked": true,
  "arbiter_override": true,
  "override_type": "structural",
  "override_description": "Changed task breakdown from 8 tasks to 5 consolidated tasks",
  "reviewer_assessment": "improvement",  // or "neutral" or "degradation"
  "errors_caught": 1,
  "false_positives": 0
}
```

**Acceptance Criteria:**
- [ ] Override frequency measured
- [ ] Quality impact of overrides assessed
- [ ] Stage-specific recommendations (which stages need arbiter?)
- [ ] Recommendation: universal arbiter or stage-specific

---

### Task 3.4: Guardrail Optimization
**Owner:** Code
**Dependencies:** Task 3.1
**Effort:** 2-3 days

**Experiment design:**
Test three guardrail approaches:
- **Variant A:** Current (GPT-Codex prefilter → GPT policy)
- **Variant B:** Same-model (Claude validates Claude's output)
- **Variant C:** Pure schema validation (JSON schema + structural checks)

**Metrics:**
- False positive rate (valid output rejected)
- False negative rate (invalid output accepted)
- Latency
- Cost
- Maintenance burden

**Acceptance Criteria:**
- [ ] All three variants tested on 10 SPECs
- [ ] Error detection rates compared
- [ ] Cost/latency measurements
- [ ] Recommendation: simplify or keep current approach

---

## Phase 4: Optimization (Weeks 7-8)

### Task 4.1: Implement Findings
**Owner:** Code
**Dependencies:** All Phase 3 tasks
**Effort:** 1 week

Based on experimental results, optimize the model strategy:

**Potential changes:**
1. Remove Gemini if Claude performs equally well
2. Remove arbiter from technical stages if self-critique suffices
3. Simplify guardrails to schema validation
4. Add stage-specific model routing

**Implementation:**
- Update `docs/spec-kit/model-strategy.md` with data-driven recommendations
- Modify `consensus_runner.sh` with optimized flow
- Update cost projections
- Document reasoning for changes

**Acceptance Criteria:**
- [ ] Model strategy updated based on evidence
- [ ] Cost projections revised
- [ ] Performance benchmarks updated
- [ ] Changelog entry documenting optimization rationale

---

### Task 4.2: Adaptive Model Selection (Optional)
**Owner:** Code
**Dependencies:** Task 4.1
**Effort:** 1-2 weeks

Implement dynamic model selection based on SPEC characteristics:

**Classification:**
```yaml
simple_spec:
  indicators: [small scope, single domain, <10 tasks]
  models: [claude]  # Single-model sufficient

medium_spec:
  indicators: [moderate scope, 2-3 domains, 10-30 tasks]
  models: [claude, gpt]  # Synthesis + arbiter

complex_spec:
  indicators: [large scope, multi-domain, >30 tasks, high risk]
  models: [gemini, claude, gpt]  # Full consensus
```

**Auto-classification:**
- Analyze SPEC.md requirements
- Count tasks, domains, external dependencies
- Assess risk level (user-provided or inferred)
- Select minimum model set needed

**Acceptance Criteria:**
- [ ] SPEC classifier implemented
- [ ] Dynamic routing in consensus_runner.sh
- [ ] Cost savings measured vs. always-full-consensus
- [ ] Quality maintained (no degradation on complex SPECs)

---

### Task 4.3: Telemetry-Driven Alerts
**Owner:** Code
**Dependencies:** Task 2.1
**Effort:** 2-3 days

Add automated alerts for anomalies:

**Alert conditions:**
```yaml
high_cost_alert:
  condition: stage_cost > p95_baseline * 1.5
  action: notify, recommend retry with cheaper model

high_disagreement_alert:
  condition: disagreement_rate > 0.5
  action: notify, recommend human review

quality_degradation_alert:
  condition: automated_checks_failed > 3
  action: halt pipeline, require manual intervention

escalation_frequency_alert:
  condition: escalations_per_spec > 3
  action: review prompts, may indicate unclear requirements
```

**Delivery:**
- Log warnings during `/spec-ops-auto` runs
- Optional: webhook to Slack/Discord
- Daily summary report

**Acceptance Criteria:**
- [ ] Alert rules configurable in `config.toml`
- [ ] Alert history logged
- [ ] False positive rate <5%
- [ ] Actionable guidance included in alerts

---

## Success Metrics

### Quantitative Goals
- [ ] **Cost reduction:** 20-40% through optimization (if multi-model not justified)
- [ ] **Latency improvement:** 30-50% through model consolidation
- [ ] **Quality maintenance:** <5% degradation vs. baseline
- [ ] **Telemetry coverage:** 100% of consensus runs instrumented

### Qualitative Goals
- [ ] **Data-driven decisions:** Model strategy backed by empirical evidence
- [ ] **Clear documentation:** Future model changes follow documented experiment process
- [ ] **Operational visibility:** Dashboard shows consensus health at a glance
- [ ] **Cost predictability:** Accurate cost projections per SPEC complexity

---

## Timeline Summary

| Phase | Duration | Key Deliverables |
|-------|----------|------------------|
| Phase 1: Foundation | 2 weeks | Schema v2, instrumented runner, cost calculator |
| Phase 2: Analysis | 2 weeks | Aggregation tools, dashboard, analyzers |
| Phase 3: Experiments | 2 weeks | Comparison data, recommendations |
| Phase 4: Optimization | 2 weeks | Optimized strategy, adaptive routing, alerts |
| **Total** | **8 weeks** | **Validated, optimized multi-model consensus** |

---

## Dependencies & Risks

### External Dependencies
- Access to all three model APIs (Gemini, Claude, GPT)
- Codex CLI `--json` output includes usage statistics
- Pricing data remains stable (check monthly)

### Risks
1. **Experiment bias:** Human reviewers may prefer certain model outputs. Mitigation: blind reviews, multiple reviewers.
2. **Insufficient sample size:** 3-5 SPECs may not be statistically significant. Mitigation: expand to 10+ SPECs if results unclear.
3. **Model updates:** Providers update models during experiment. Mitigation: lock model versions, document which version tested.
4. **Cost overruns:** Running duplicate experiments doubles costs. Mitigation: budget approval before Phase 3, use test SPECs not production.

---

## Next Steps

1. **Immediate:** Review this plan with stakeholders
2. **Week 1:** Start Task 1.1 (Schema v2)
3. **Week 2:** Instrument consensus runner (Task 1.2)
4. **Week 4:** Begin experiments (Phase 3)
5. **Week 8:** Ship optimized model strategy (Phase 4)

**Owner:** Spec Kit maintainers (feat/spec-auto-telemetry)
**Last updated:** 2025-10-04
