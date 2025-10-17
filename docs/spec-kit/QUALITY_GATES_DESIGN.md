# Intelligent Quality Gates for Spec-Auto Pipeline

**Feature:** T85 - Autonomous Quality Assurance
**Status:** Design Phase
**Created:** 2025-10-16
**Goal:** Integrate clarify/analyze/checklist into /speckit.auto with agent-driven resolution

---

## Problem Statement

**Current State:**
- `/speckit.auto` runs 6 stages: plan → tasks → implement → validate → audit → unlock
- Quality commands (`/speckit.clarify`, `/speckit.analyze`, `/speckit.checklist`) exist separately
- Users must manually run quality checks
- All issues escalated to humans (no auto-resolution)

**Desired State:**
- Quality gates integrated into automation pipeline
- Agents classify issues by confidence and magnitude
- Agents auto-resolve routine issues
- Only escalate high-uncertainty or critical issues to humans
- Fully autonomous for 80% of quality concerns

---

## Quality Gate Insertion Points

### Proposed Enhanced Pipeline

```
Current (6 stages):
  plan → tasks → implement → validate → audit → unlock

Enhanced (6 stages + 4 quality gates):
  QG1: clarify (post-SPEC, pre-plan)
    ↓
  QG2: checklist (post-clarify, pre-plan)
    ↓
  plan
    ↓
  QG3: analyze (post-plan, pre-tasks)
    ↓
  tasks
    ↓
  QG4: analyze (post-tasks, pre-implement)
    ↓
  implement → validate → audit → unlock
```

### Quality Gate 1: Clarify (Pre-Planning)

**When:** After SPEC created/read, before planning begins
**Command:** `/speckit.clarify`
**Purpose:** Resolve ambiguities in requirements before planning

**Agent Analysis:**
```json
{
  "ambiguities": [
    {
      "question": "Should OAuth2 support multiple providers or just one?",
      "confidence": "low",           // low/medium/high
      "magnitude": "critical",        // minor/important/critical
      "resolvability": "need-human",  // auto-fix/suggest-fix/need-human
      "context": "Spec doesn't specify provider count",
      "suggested_resolution": null
    },
    {
      "question": "What's the token expiry time?",
      "confidence": "medium",
      "magnitude": "important",
      "resolvability": "suggest-fix",
      "context": "Industry standard is 3600s",
      "suggested_resolution": "Use 3600s (1 hour) as default, configurable"
    },
    {
      "question": "Should we log failed auth attempts?",
      "confidence": "high",
      "magnitude": "minor",
      "resolvability": "auto-fix",
      "context": "Security best practice",
      "suggested_resolution": "Yes, log to audit trail"
    }
  ],
  "auto_resolved": 1,
  "escalated": 1,
  "total": 3
}
```

**Auto-Resolution Logic:**
```
IF confidence = high AND resolvability = auto-fix:
  → Agent applies fix automatically
  → Log decision to telemetry
  → Continue pipeline

IF confidence = medium AND magnitude = minor AND resolvability = suggest-fix:
  → Agent suggests fix
  → Apply suggestion automatically
  → Log decision
  → Continue

ELSE (low confidence OR critical magnitude OR need-human):
  → Escalate to human
  → Pause pipeline
  → Show question with context
  → Wait for human answer
  → Resume pipeline
```

---

### Quality Gate 2: Checklist (Pre-Planning)

**When:** After clarify, before planning
**Command:** `/speckit.checklist`
**Purpose:** Validate requirement quality scores

**Agent Analysis:**
```json
{
  "requirements": [
    {
      "id": "R1",
      "text": "System shall authenticate users",
      "scores": {
        "specificity": 3.2,      // 0-10 scale
        "testability": 4.1,
        "completeness": 3.8,
        "clarity": 4.5
      },
      "overall": 3.9,            // Average
      "threshold": 6.0,          // Minimum acceptable
      "needs_improvement": true,
      "confidence": "high",
      "resolvability": "auto-fix",
      "suggested_improvement": "System shall authenticate users via OAuth2 with support for Google, GitHub, and Microsoft providers, validating JWT tokens with 1-hour expiry"
    }
  ],
  "below_threshold": 5,
  "auto_improved": 4,
  "escalated": 1
}
```

**Auto-Resolution Logic:**
```
FOR each requirement with overall < threshold:
  IF confidence = high AND improvement clearly defined:
    → Agent rewrites requirement
    → Persist to SPEC
    → Log improvement
    → Continue

  IF confidence = medium AND magnitude = important:
    → Agent suggests improvement
    → Apply automatically
    → Continue

  ELSE (low confidence OR critical):
    → Show requirement + scores + suggestion
    → Ask human to approve/modify
    → Pause pipeline
```

---

### Quality Gate 3: Analyze (Post-Plan)

**When:** After plan created, before tasks generation
**Command:** `/speckit.analyze`
**Purpose:** Check plan consistency with SPEC

**Agent Analysis:**
```json
{
  "inconsistencies": [
    {
      "type": "missing_requirement",
      "severity": "critical",
      "description": "SPEC requires OAuth2 but plan doesn't mention it",
      "affected_artifacts": ["plan.md", "spec.md"],
      "confidence": "high",
      "resolvability": "need-human",
      "suggested_fix": "Add OAuth2 implementation to work breakdown step 3"
    },
    {
      "type": "terminology_mismatch",
      "severity": "minor",
      "description": "SPEC uses 'user' but plan uses 'account'",
      "affected_artifacts": ["plan.md:15", "spec.md:8"],
      "confidence": "high",
      "resolvability": "auto-fix",
      "suggested_fix": "Standardize on 'user' throughout plan.md"
    }
  ],
  "auto_resolved": 1,
  "escalated": 1
}
```

**Auto-Resolution Logic:**
```
IF severity = minor AND confidence = high AND resolvability = auto-fix:
  → Agent fixes inconsistency
  → Update affected files
  → Log fix
  → Continue

IF severity = important AND confidence = high AND clear fix:
  → Suggest fix
  → Apply with confirmation
  → Continue

ELSE (critical OR low confidence):
  → Escalate to human
  → Pause pipeline
  → Show inconsistency with context
  → Wait for resolution
```

---

### Quality Gate 4: Analyze (Post-Tasks)

**When:** After tasks created, before implementation
**Command:** `/speckit.analyze`
**Purpose:** Verify tasks cover all requirements

**Agent Analysis:**
```json
{
  "coverage_gaps": [
    {
      "requirement": "R3: Support MFA",
      "missing_task": true,
      "confidence": "high",
      "resolvability": "auto-fix",
      "suggested_task": "T5: Implement TOTP-based MFA with QR code generation"
    }
  ],
  "task_conflicts": [
    {
      "task1": "T2: Create auth endpoints",
      "task2": "T4: Build API routes",
      "conflict": "Overlapping scope",
      "confidence": "medium",
      "resolvability": "suggest-fix",
      "suggested_resolution": "Merge into single task: T2 - Auth API endpoints"
    }
  ],
  "auto_resolved": 2,
  "escalated": 0
}
```

---

## Agent Decision Schema

### Classification Dimensions

**1. Confidence** (How sure are agents about the issue)
- `high` (>90% agent agreement) - Clear, unambiguous
- `medium` (70-90% agreement) - Probable, reasonable assumptions
- `low` (<70% agreement) - Uncertain, conflicting opinions

**2. Magnitude** (Impact of the issue)
- `critical` - Blocks progress, affects core functionality
- `important` - Significant but not blocking
- `minor` - Nice-to-have, cosmetic, minor inconsistency

**3. Resolvability** (Can agents fix it)
- `auto-fix` - Straightforward, well-defined fix
- `suggest-fix` - Fix available but needs validation
- `need-human` - Requires domain knowledge or judgment

### Escalation Decision Matrix

```
┌─────────────┬──────────┬───────────┬──────────┐
│ Confidence  │ Magnitude│Resolvable │ Action   │
├─────────────┼──────────┼───────────┼──────────┤
│ high        │ minor    │ auto-fix  │ AUTO ✅  │
│ high        │ minor    │ suggest   │ AUTO ✅  │
│ high        │ important│ auto-fix  │ AUTO ✅  │
│ high        │ important│ suggest   │ CONFIRM  │
│ high        │ critical │ auto-fix  │ CONFIRM  │
│ high        │ critical │ any       │ ESCALATE │
│ medium      │ minor    │ auto-fix  │ AUTO ✅  │
│ medium      │ minor    │ suggest   │ CONFIRM  │
│ medium      │ important│ any       │ ESCALATE │
│ medium      │ critical │ any       │ ESCALATE │
│ low         │ any      │ any       │ ESCALATE │
├─────────────┴──────────┴───────────┴──────────┤
│ Actions:                                       │
│ AUTO ✅   - Apply fix, log, continue          │
│ CONFIRM  - Show fix, apply with approval      │
│ ESCALATE - Pause, show question, wait         │
└────────────────────────────────────────────────┘
```

---

## Enhanced State Machine

### Extended SpecAutoPhase Enum

```rust
#[derive(Debug, Clone)]
pub enum SpecAutoPhase {
    // Existing phases
    Guardrail,
    ExecutingAgents { ... },
    CheckingConsensus,

    // NEW: Quality gate phases
    QualityGate {
        gate_type: QualityGateType,
        executing_agents: bool,
        collected_results: Vec<AgentQualityResult>,
    },
    QualityGateResolution {
        gate_type: QualityGateType,
        issues: Vec<QualityIssue>,
        auto_resolved: Vec<QualityIssue>,
        escalated: Vec<QualityIssue>,
        awaiting_human: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum QualityGateType {
    ClarifyPrePlan,      // Before planning
    ChecklistPrePlan,    // After clarify
    AnalyzePostPlan,     // After plan created
    AnalyzePostTasks,    // After tasks created
}

#[derive(Debug, Clone)]
pub struct QualityIssue {
    pub id: String,
    pub gate_type: QualityGateType,
    pub issue_type: String,        // "ambiguity", "low_score", "inconsistency"
    pub description: String,
    pub confidence: Confidence,
    pub magnitude: Magnitude,
    pub resolvability: Resolvability,
    pub suggested_fix: Option<String>,
    pub context: String,
    pub affected_artifacts: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Confidence {
    High,    // >90% agent agreement
    Medium,  // 70-90%
    Low,     // <70%
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Magnitude {
    Critical,   // Blocks progress
    Important,  // Significant impact
    Minor,      // Low impact
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Resolvability {
    AutoFix,      // Apply immediately
    SuggestFix,   // Suggest and apply with confirmation
    NeedHuman,    // Requires human judgment
}
```

---

## Enhanced Pipeline Flow

### Current Flow (6 stages)

```
spec-auto start
  ↓
FOR each stage (plan, tasks, implement, validate, audit, unlock):
  1. Run guardrail validation
  2. Execute multi-agent consensus
  3. Check consensus
  4. Advance to next stage
  ↓
spec-auto complete
```

### Enhanced Flow (6 stages + 4 quality gates)

```
spec-auto start
  ↓
→ QG1: Clarify (agents identify ambiguities)
  ├─ Auto-resolve: high confidence, minor issues
  ├─ Escalate: low confidence OR critical issues
  └─ If escalated: Pause → Show questions → Wait → Resume
  ↓
→ QG2: Checklist (agents score requirements)
  ├─ Auto-improve: low scores with clear fixes
  ├─ Escalate: low scores without clear fixes
  └─ Update SPEC with improvements
  ↓
plan stage
  ↓
→ QG3: Analyze Plan (check plan ↔ spec consistency)
  ├─ Auto-fix: minor inconsistencies (terminology, etc.)
  ├─ Escalate: missing requirements, major gaps
  └─ Update plan.md if auto-fixed
  ↓
tasks stage
  ↓
→ QG4: Analyze Tasks (check tasks ↔ requirements)
  ├─ Auto-fix: missing tasks with clear scope
  ├─ Escalate: ambiguous coverage gaps
  └─ Update tasks.md if auto-fixed
  ↓
implement → validate → audit → unlock
  ↓
spec-auto complete
```

---

## Agent Prompt Design

### Quality Gate Prompt Template

**For Clarify Gate:**
```
You are analyzing SPEC ${SPEC_ID} for ambiguities before planning begins.

Your task:
1. Identify ambiguous or unclear requirements
2. Classify each by:
   - confidence: how certain are you this is ambiguous? (high/medium/low)
   - magnitude: impact if unresolved? (critical/important/minor)
   - resolvability: can you fix it? (auto-fix/suggest-fix/need-human)
3. For resolvable issues, provide suggested_resolution
4. For unresolvable issues, formulate precise question for human

Output JSON:
{
  "ambiguities": [
    {
      "question": string,
      "confidence": "high" | "medium" | "low",
      "magnitude": "critical" | "important" | "minor",
      "resolvability": "auto-fix" | "suggest-fix" | "need-human",
      "context": string,
      "affected_requirements": [string],
      "suggested_resolution": string | null,
      "reasoning": string
    }
  ],
  "agent": "${AGENT_NAME}",
  "stage": "clarify-gate"
}

Auto-Resolution Guidelines:
- auto-fix: Industry standards, obvious answers (e.g., "log errors" → yes)
- suggest-fix: Reasonable defaults with rationale (e.g., "token expiry" → 3600s)
- need-human: Business decisions, architectural choices, trade-offs

Only escalate to human if:
- confidence = low OR
- magnitude = critical OR
- resolvability = need-human
```

---

## Implementation Architecture

### 1. Extended State Types

```rust
// state.rs additions

#[derive(Debug, Clone, PartialEq)]
pub enum QualityGateType {
    ClarifyPrePlan,
    ChecklistPrePlan,
    AnalyzePostPlan,
    AnalyzePostTasks,
}

impl QualityGateType {
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::ClarifyPrePlan | Self::ChecklistPrePlan => "clarify",
            Self::AnalyzePostPlan | Self::AnalyzePostTasks => "analyze",
        }
    }

    pub fn stage_name(&self) -> &'static str {
        match self {
            Self::ClarifyPrePlan => "clarify-pre-plan",
            Self::ChecklistPrePlan => "checklist-pre-plan",
            Self::AnalyzePostPlan => "analyze-post-plan",
            Self::AnalyzePostTasks => "analyze-post-tasks",
        }
    }
}

#[derive(Debug, Clone)]
pub struct QualityIssue {
    pub id: String,
    pub gate_type: QualityGateType,
    pub issue_type: String,
    pub description: String,
    pub confidence: Confidence,
    pub magnitude: Magnitude,
    pub resolvability: Resolvability,
    pub suggested_fix: Option<String>,
    pub context: String,
    pub affected_artifacts: Vec<String>,
    pub agent_reasoning: String,
}

#[derive(Debug, Clone)]
pub struct QualityGateOutcome {
    pub gate_type: QualityGateType,
    pub total_issues: usize,
    pub auto_resolved: usize,
    pub escalated: usize,
    pub escalated_issues: Vec<QualityIssue>,
    pub telemetry_path: Option<PathBuf>,
}

impl SpecAutoPhase {
    // Add new phase constructors
    pub fn quality_gate(gate_type: QualityGateType) -> Self {
        Self::QualityGate {
            gate_type,
            executing_agents: true,
            collected_results: Vec::new(),
        }
    }

    pub fn quality_resolution(
        gate_type: QualityGateType,
        outcome: QualityGateOutcome,
    ) -> Self {
        Self::QualityGateResolution {
            gate_type,
            issues: outcome.escalated_issues.clone(),
            auto_resolved: Vec::new(), // Will be populated
            escalated: outcome.escalated_issues,
            awaiting_human: !outcome.escalated_issues.is_empty(),
        }
    }
}
```

---

### 2. Quality Gate Handler

```rust
// handler.rs additions

pub fn execute_quality_gate(
    widget: &mut ChatWidget,
    spec_id: &str,
    gate_type: QualityGateType,
) -> QualityGateOutcome {
    // 1. Submit quality gate prompt to agents
    let command = match gate_type {
        QualityGateType::ClarifyPrePlan | QualityGateType::ChecklistPrePlan => "clarify",
        QualityGateType::AnalyzePostPlan | QualityGateType::AnalyzePostTasks => "analyze",
    };

    // 2. Format subagent command with quality gate instructions
    let prompt = format_quality_gate_prompt(spec_id, gate_type);

    // 3. Submit to orchestrator
    widget.submit_prompt_with_display(
        format!("Quality Gate: {}", gate_type.stage_name()),
        prompt,
    );

    // State machine will:
    // - Wait for agent responses
    // - Collect QualityIssue results
    // - Apply auto-resolution logic
    // - Return outcome
}

fn format_quality_gate_prompt(spec_id: &str, gate_type: QualityGateType) -> String {
    // Load SPEC context
    // Add quality gate instructions
    // Include escalation guidelines
    // Include auto-resolution rules
    // Return formatted prompt
}

pub fn process_quality_gate_results(
    widget: &mut ChatWidget,
    gate_type: QualityGateType,
    agent_results: Vec<Value>,
) -> QualityGateOutcome {
    // 1. Parse agent JSON responses into QualityIssue structs
    let issues = parse_quality_issues(agent_results);

    // 2. Classify each issue
    let (auto_resolved, escalated) = classify_issues(&issues);

    // 3. Apply auto-resolutions
    for issue in &auto_resolved {
        apply_auto_fix(widget, issue);
    }

    // 4. Return outcome
    QualityGateOutcome {
        gate_type,
        total_issues: issues.len(),
        auto_resolved: auto_resolved.len(),
        escalated: escalated.len(),
        escalated_issues: escalated,
        telemetry_path: persist_quality_telemetry(gate_type, &issues),
    }
}

fn classify_issues(issues: &[QualityIssue]) -> (Vec<QualityIssue>, Vec<QualityIssue>) {
    let mut auto_resolved = Vec::new();
    let mut escalated = Vec::new();

    for issue in issues {
        if should_auto_resolve(issue) {
            auto_resolved.push(issue.clone());
        } else {
            escalated.push(issue.clone());
        }
    }

    (auto_resolved, escalated)
}

fn should_auto_resolve(issue: &QualityIssue) -> bool {
    use {Confidence::*, Magnitude::*, Resolvability::*};

    match (issue.confidence, issue.magnitude, issue.resolvability) {
        // Auto-resolve: High confidence + minor/important + auto-fix
        (High, Minor, AutoFix) => true,
        (High, Minor, SuggestFix) => true,
        (High, Important, AutoFix) => true,

        // Auto-resolve: Medium confidence + minor + auto-fix
        (Medium, Minor, AutoFix) => true,

        // Escalate everything else
        _ => false,
    }
}

fn apply_auto_fix(widget: &mut ChatWidget, issue: &QualityIssue) {
    // 1. Log the fix
    widget.push_background(
        format!(
            "Quality Gate: Auto-resolved {} issue: {}",
            issue.magnitude, issue.description
        ),
        BackgroundPlacement::Tail,
    );

    // 2. Apply the fix (update SPEC, plan.md, tasks.md, etc.)
    if let Some(fix) = &issue.suggested_fix {
        // Apply fix to affected artifacts
        // This might involve file edits, SPEC updates, etc.
        // Log to telemetry
    }

    // 3. Persist decision to evidence
    persist_auto_resolution(issue);
}
```

---

### 3. Human Escalation UI

**When escalation occurs:**

```
╔══════════════════════════════════════════════════════════╗
║ Quality Gate: Clarify (Pre-Plan)                        ║
╟──────────────────────────────────────────────────────────╢
║ Status: 3 ambiguities found                              ║
║ Auto-resolved: 2 (logged to telemetry)                   ║
║ Escalated: 1 (requires your input)                       ║
╟──────────────────────────────────────────────────────────╢
║ ❓ Question 1 of 1                                       ║
║                                                          ║
║ Should OAuth2 support multiple providers or just one?    ║
║                                                          ║
║ Context:                                                 ║
║ • SPEC mentions "OAuth2" but doesn't specify providers   ║
║ • Plan step 3 assumes "provider selection UI"            ║
║ • Industry practice: most apps support 2-5 providers     ║
║                                                          ║
║ Confidence: LOW (agents disagree on intent)              ║
║ Magnitude: CRITICAL (affects architecture)               ║
║                                                          ║
║ Affected: spec.md:12, plan.md:8                          ║
║                                                          ║
║ Options:                                                 ║
║ [1] Multiple providers (Google, GitHub, Microsoft)       ║
║ [2] Single provider (specify which)                      ║
║ [3] Provide custom answer                                ║
║ [4] Skip this clarification                              ║
║                                                          ║
║ Your choice [1-4]:                                       ║
╚══════════════════════════════════════════════════════════╝
```

**After human answers:**
- Update SPEC with clarification
- Log decision to telemetry
- Continue pipeline

---

### 4. Telemetry Schema

**Quality gate telemetry file:**

```json
{
  "command": "quality-gate",
  "specId": "SPEC-KIT-065",
  "gateType": "clarify-pre-plan",
  "timestamp": "2025-10-16T20:00:00Z",
  "schemaVersion": "v1",
  "agents": ["gemini", "claude", "code"],
  "results": {
    "total_issues": 5,
    "auto_resolved": 3,
    "escalated": 2,
    "auto_resolved_details": [
      {
        "issue_id": "Q1",
        "description": "Should we log failed auth attempts?",
        "confidence": "high",
        "magnitude": "minor",
        "resolution": "Yes, log to audit trail (security best practice)",
        "applied_fix": "Added logging requirement to spec.md:15"
      }
    ],
    "escalated_details": [
      {
        "issue_id": "Q2",
        "description": "OAuth2 provider count?",
        "confidence": "low",
        "magnitude": "critical",
        "human_answer": "Multiple providers (Google, GitHub, Microsoft)",
        "applied_fix": "Updated spec.md:12 with provider list"
      }
    ]
  },
  "artifacts": [
    "spec.md (updated with 3 auto-resolutions, 2 human answers)",
    "quality-gate-clarify_2025-10-16T20:00:00Z.json"
  ]
}
```

---

## Configuration

### Quality Gate Settings

```rust
// config.rs additions

#[derive(Debug, Clone)]
pub struct QualityGateConfig {
    pub enabled: bool,
    pub auto_resolve_threshold: AutoResolveThreshold,
    pub gates: Vec<QualityGateType>,
}

#[derive(Debug, Clone, Copy)]
pub enum AutoResolveThreshold {
    Conservative,  // Only auto-fix high confidence + minor
    Balanced,      // Auto-fix high confidence + minor/important
    Aggressive,    // Auto-fix medium+ confidence (more automation)
}

impl Default for QualityGateConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_resolve_threshold: AutoResolveThreshold::Balanced,
            gates: vec![
                QualityGateType::ClarifyPrePlan,
                QualityGateType::ChecklistPrePlan,
                QualityGateType::AnalyzePostPlan,
                QualityGateType::AnalyzePostTasks,
            ],
        }
    }
}
```

**User control:**
```bash
# Run with quality gates (default)
/speckit.auto SPEC-KIT-065

# Disable quality gates
/speckit.auto SPEC-KIT-065 --no-quality-gates

# Conservative auto-resolution
/speckit.auto SPEC-KIT-065 --quality conservative

# Aggressive auto-resolution (fewer escalations)
/speckit.auto SPEC-KIT-065 --quality aggressive
```

---

## Benefits Analysis

### Automation Improvement

**Before:**
- User runs `/speckit.auto`
- Pipeline executes blindly
- Quality issues discovered later
- Manual quality checks needed

**After:**
- Pipeline self-checks quality
- 80% of issues auto-resolved
- Only critical/uncertain issues escalated
- Higher quality output automatically

### Time Savings

**Scenario: SPEC with 5 ambiguities**

**Manual approach:**
- Run `/speckit.auto` (60 min)
- Discover ambiguities during planning
- Stop, run `/speckit.clarify` (10 min)
- Answer 5 questions (15 min)
- Re-run planning (10 min)
- **Total:** 95 minutes

**Quality Gate approach:**
- Run `/speckit.auto` with gates
- Clarify gate runs (10 min)
- Agents auto-resolve 3 issues (0 min human time)
- Human answers 2 critical questions (6 min)
- Pipeline continues (50 min remaining stages)
- **Total:** 66 minutes, **30% savings**

### Quality Improvement

**Metrics to track:**
- % of issues auto-resolved (target: 80%)
- % of escalated issues that were truly critical (target: 90%+)
- Pipeline completion rate without human intervention (target: 70%)
- False positive rate (auto-fixes that were wrong) (target: <5%)

---

## Implementation Phases

### Phase 1: Foundation (4-6 hours)

**Tasks:**
1. Extend `SpecAutoPhase` enum with quality gate phases
2. Add `QualityIssue`, `QualityGateType`, `Confidence`, `Magnitude`, `Resolvability` types
3. Add quality gate prompts to prompts.json
4. Update state machine in handler.rs

**Deliverables:**
- State types defined
- Prompts ready
- State machine extended

---

### Phase 2: Agent Integration (6-8 hours)

**Tasks:**
1. Implement `execute_quality_gate()` in handler.rs
2. Implement `process_quality_gate_results()`
3. Implement `classify_issues()` with decision matrix
4. Implement `should_auto_resolve()` logic
5. Add telemetry persistence

**Deliverables:**
- Quality gates execute
- Agent results parsed
- Classification logic working
- Telemetry captured

---

### Phase 3: Auto-Resolution (4-6 hours)

**Tasks:**
1. Implement `apply_auto_fix()` for each issue type
2. Add SPEC/plan/tasks file updates
3. Add logging for auto-resolutions
4. Add rollback support (in case auto-fix is wrong)

**Deliverables:**
- Auto-fixes applied
- Files updated
- Audit trail complete

---

### Phase 4: Human Escalation (6-8 hours)

**Tasks:**
1. Design escalation UI modal
2. Implement pause/resume pipeline
3. Implement question presentation
4. Capture human answers
5. Apply human resolutions
6. Resume pipeline

**Deliverables:**
- Escalation UI works
- Pipeline pauses/resumes
- Human answers captured

---

### Phase 5: Testing & Validation (8-10 hours)

**Tasks:**
1. Unit tests for classification logic
2. Integration tests for quality gates
3. Test auto-resolution for each issue type
4. Test escalation flow
5. Validate telemetry schema

**Deliverables:**
- 20+ new unit tests
- Integration tests
- E2E validation

---

## Total Effort Estimate

**Total:** 28-38 hours (1 week of focused work)

**Breakdown:**
- Foundation: 4-6 hours
- Agent integration: 6-8 hours
- Auto-resolution: 4-6 hours
- Human escalation: 6-8 hours
- Testing: 8-10 hours

---

## Risks & Considerations

### Risks

**1. False Positives (Auto-Fixes Wrong)**
- **Risk:** Agents apply incorrect fixes automatically
- **Mitigation:** Conservative threshold by default, comprehensive logging, easy rollback
- **Severity:** MEDIUM

**2. Over-Escalation (Too Many Questions)**
- **Risk:** Pipeline stops frequently, annoys users
- **Mitigation:** Tune thresholds, track false positive rate, allow aggressive mode
- **Severity:** LOW

**3. Under-Escalation (Missed Critical Issues)**
- **Risk:** Agent auto-fixes something that needed human judgment
- **Mitigation:** Conservative classification for critical magnitude, audit trail review
- **Severity:** HIGH (but unlikely with proper thresholds)

**4. Pipeline Complexity**
- **Risk:** State machine becomes complex, hard to debug
- **Mitigation:** Comprehensive telemetry, clear state transitions, good tests
- **Severity:** MEDIUM

### Considerations

**Performance:**
- Each quality gate adds 8-12 min (3 agents)
- 4 gates = 32-48 min additional time
- **BUT:** Saves rework time (plan → clarify → re-plan)
- Net impact: Likely faster overall for complex SPECs

**User Experience:**
- More autonomous (less human intervention)
- Fewer "oops, should've clarified that" moments
- Clearer escalations (only when truly needed)
- Better visibility into what agents are fixing

**Configurability:**
- Must allow disabling gates (`--no-quality-gates`)
- Must allow threshold tuning (conservative/balanced/aggressive)
- Must allow selective gates (e.g., only clarify, skip analyze)

---

## Success Criteria

**For Phase 1 (MVP):**
- ✅ Clarify gate integrated into pipeline
- ✅ At least 50% of issues auto-resolved
- ✅ Escalated issues are actually critical
- ✅ Telemetry captures all decisions
- ✅ Pipeline completes faster for complex SPECs

**For Full Implementation:**
- ✅ All 4 quality gates working
- ✅ 80% auto-resolution rate
- ✅ <5% false positive rate
- ✅ 90%+ of escalations are valid
- ✅ User satisfaction (fewer interruptions for trivial questions)

---

## Example: Full Pipeline with Quality Gates

```bash
$ /speckit.auto SPEC-KIT-065 Add OAuth2 authentication

→ Starting spec-auto pipeline for SPEC-KIT-065
→ Mode: With quality gates (balanced threshold)

[Stage 0: Quality Gate - Clarify]
→ Analyzing SPEC for ambiguities...
→ Found 5 ambiguities
  ✅ Auto-resolved: "Log failed attempts?" → Yes (security best practice)
  ✅ Auto-resolved: "Token format?" → JWT (industry standard)
  ✅ Auto-resolved: "HTTPS required?" → Yes (security requirement)
  ⏸️  Escalated: "OAuth2 provider count?" (critical decision)

┌─ Question 1 of 2 ─────────────────────────────────────┐
│ Should OAuth2 support multiple providers or just one? │
│                                                        │
│ Context: SPEC doesn't specify, plan assumes multi     │
│ Magnitude: CRITICAL (affects architecture)            │
│                                                        │
│ [1] Multiple providers                                │
│ [2] Single provider                                   │
│ Your choice: 1                                        │
└────────────────────────────────────────────────────────┘

→ Updated spec.md with clarifications
→ Clarify gate: 3 auto-resolved, 2 answered by human

[Stage 1: Quality Gate - Checklist]
→ Scoring requirements...
→ Found 2 requirements below threshold (6.0)
  ✅ Auto-improved: R1 (3.9 → 7.2) - Added specificity
  ✅ Auto-improved: R3 (4.5 → 6.8) - Added testability criteria
→ Checklist gate: 2 auto-improved, 0 escalated

[Stage 2: Plan]
→ Running guardrail validation...
→ Executing multi-agent planning...
→ Plan created

[Stage 3: Quality Gate - Analyze Plan]
→ Checking plan ↔ spec consistency...
→ Found 1 inconsistency
  ✅ Auto-fixed: Terminology mismatch ('user' vs 'account')
→ Analyze gate: 1 auto-fixed, 0 escalated

[Stage 4: Tasks]
→ Running guardrail validation...
→ Executing multi-agent task generation...
→ Tasks created

[Stage 5: Quality Gate - Analyze Tasks]
→ Checking task coverage...
→ All requirements covered ✅
→ Analyze gate: 0 issues

[Stage 6-9: Implement → Validate → Audit → Unlock]
→ Continuing pipeline...

✅ Pipeline complete
→ Quality gates: 6 auto-resolved, 2 human answers
→ Time: 68 minutes (vs 95 without gates)
→ Quality: Higher (issues caught early)
```

---

## Recommendation

### This is a MAJOR Enhancement

**Value Proposition:**
- 🤖 More autonomous automation
- ⏱️ Time savings (catch issues early)
- ✨ Higher quality outputs
- 🎯 Only escalate what truly needs human judgment

**Complexity:**
- New state machine phases
- Agent decision logic
- UI for escalations
- Comprehensive testing needed

**Effort:** 28-38 hours (1 week)

**ROI:** HIGH for complex SPECs with ambiguities

**Recommendation:**
1. **Phase 1 MVP:** Start with just Clarify gate (8-10 hours)
2. **Validate:** Test with real SPECs, tune thresholds
3. **Phase 2:** Add Checklist and Analyze gates if MVP succeeds
4. **Phase 3:** Add configuration options

**Start small, prove value, expand** 🚀

Want me to start implementing Phase 1 (Clarify gate MVP)?