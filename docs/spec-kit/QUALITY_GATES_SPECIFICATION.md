# Quality Gates for Spec-Auto Pipeline - Complete Specification

**Feature:** T85 - Intelligent Quality Assurance Integration
**Status:** Design Complete, Ready for Implementation
**Created:** 2025-10-16
**Decisions:** All locked in via CLEARFRAME process

---

## Design Decisions (Finalized)

| # | Decision Point | Choice | Rationale |
|---|----------------|--------|-----------|
| 1 | Auto-resolution threshold | **Majority (2/3) + GPT-5 validation** | Balances accuracy with automation |
| 2 | Gate placement | **Inline at 3 checkpoints** | Maximum quality coverage |
| 3 | Auto-resolution action | **Modify files immediately** | Real-time application |
| 4 | Review model | **Post-pipeline summary** | No interruptions during auto-resolution |
| 5 | GPT-5 validation context | **Full (SPEC + PRD + reasoning)** | Maximum accuracy |
| 6 | GPT-5 disagreement | **Escalate immediately** | Conservative on uncertain cases |
| 7 | Git handling | **Single commit at pipeline end** | Clean history |
| 8 | Escalation behavior | **Block pipeline until answered** | Safe, no placeholder assumptions |
| 9 | Rollback mechanism | **Manual edit (no infrastructure)** | Simplest approach |

---

## Architecture

### Pipeline Flow

```
/speckit.auto SPEC-KIT-065

┌─────────────────────────────────────┐
│ Checkpoint 1: Pre-Planning          │
├─────────────────────────────────────┤
│ → Clarify gate (3 agents)           │
│   - Identify ambiguities            │
│   - Classify by agreement           │
│   - Auto-resolve unanimous          │
│   - GPT-5 validate 2/3 majority     │
│   - Escalate if GPT-5 disagrees     │
│                                     │
│ → Checklist gate (3 agents)         │
│   - Score requirements (0-10)       │
│   - Auto-improve if fix clear       │
│   - Escalate if unclear             │
│                                     │
│ → Batch escalations                 │
│   [INTERRUPTION: Show N questions]  │
│   [BLOCK: Wait for human answers]   │
│                                     │
│ → Apply auto-resolutions to spec.md │
│ → Apply human answers to spec.md    │
└─────────────────────────────────────┘

┌─────────────────────────────────────┐
│ Plan Stage                          │
│ (Uses updated spec.md)              │
└─────────────────────────────────────┘

┌─────────────────────────────────────┐
│ Checkpoint 2: Post-Plan             │
├─────────────────────────────────────┤
│ → Analyze gate (3 agents)           │
│   - Check plan ↔ spec consistency   │
│   - Auto-fix terminology/minor      │
│   - Escalate missing requirements   │
│                                     │
│ → If escalations:                   │
│   [INTERRUPTION: Show N questions]  │
│   [BLOCK: Wait for answers]         │
│                                     │
│ → Apply to plan.md                  │
└─────────────────────────────────────┘

┌─────────────────────────────────────┐
│ Tasks Stage                         │
│ (Uses updated plan.md)              │
└─────────────────────────────────────┘

┌─────────────────────────────────────┐
│ Checkpoint 3: Post-Tasks            │
├─────────────────────────────────────┤
│ → Analyze gate (3 agents)           │
│   - Check task ↔ requirement map    │
│   - Auto-add obvious missing tasks  │
│   - Escalate coverage gaps          │
│                                     │
│ → If escalations:                   │
│   [INTERRUPTION: Show N questions]  │
│   [BLOCK: Wait for answers]         │
│                                     │
│ → Apply to tasks.md                 │
└─────────────────────────────────────┘

┌─────────────────────────────────────┐
│ Implement → Validate → Audit        │
│ → Unlock Stages                     │
│ (Use updated spec/plan/tasks)       │
└─────────────────────────────────────┘

┌─────────────────────────────────────┐
│ Pipeline Complete                   │
├─────────────────────────────────────┤
│ → Git commit quality gate changes   │
│ → Show review summary               │
│ → Link to telemetry                 │
└─────────────────────────────────────┘
```

**Expected:**
- 3 potential interruption points
- ~5 questions total (batched at checkpoints)
- 12-17 auto-resolutions applied
- 40 minutes added to 60-minute pipeline = 100 minutes total

---

## Resolution Logic (Exact Algorithm)

```rust
fn resolve_quality_issue(issue: &QualityIssue) -> Resolution {
    let agent_answers = &issue.agent_answers;  // [gemini, claude, code]

    // Count agreement
    let unique_answers: HashSet<_> = agent_answers.iter().collect();
    let agreement_count = agent_answers.len() - unique_answers.len() + 1;

    match agreement_count {
        // All 3 agents agree
        3 => Resolution::AutoApply {
            answer: agent_answers[0].clone(),
            confidence: Confidence::High,
            reason: "Unanimous (3/3 agents)",
            validation: None,
        },

        // 2 out of 3 agree (majority)
        2 => {
            let majority_answer = find_majority(agent_answers);
            let dissent = find_dissent(agent_answers);

            // Ask GPT-5 to validate with full context
            let gpt5_result = validate_with_gpt5(
                issue,
                majority_answer,
                dissent,
                full_context: true  // SPEC + PRD + all agent reasoning
            );

            if gpt5_result.agrees_with_majority {
                Resolution::AutoApply {
                    answer: majority_answer,
                    confidence: Confidence::Medium,
                    reason: "Majority (2/3) + GPT-5 validated",
                    validation: Some(gpt5_result.reasoning),
                }
            } else {
                Resolution::Escalate {
                    reason: "GPT-5 rejected majority",
                    all_answers: agent_answers.clone(),
                    gpt5_reasoning: gpt5_result.reasoning,
                    recommended: gpt5_result.recommended_answer,
                }
            }
        },

        // No consensus (0 or 1 agree)
        _ => Resolution::Escalate {
            reason: "No agent consensus",
            all_answers: agent_answers.clone(),
            gpt5_reasoning: None,
            recommended: None,
        }
    }
}
```

**Result distribution (based on experiment):**
- Auto-apply: ~55%
- Escalate: ~45%

---

## State Machine Extensions

### New Phase Types

```rust
#[derive(Debug, Clone)]
pub enum SpecAutoPhase {
    // Existing
    Guardrail,
    ExecutingAgents { ... },
    CheckingConsensus,

    // NEW: Quality gate phases
    QualityGateExecuting {
        checkpoint: QualityCheckpoint,
        gates: Vec<QualityGateType>,
        active_gates: HashSet<QualityGateType>,
        results: HashMap<QualityGateType, Vec<AgentQualityResult>>,
    },

    QualityGateProcessing {
        checkpoint: QualityCheckpoint,
        auto_resolved: Vec<QualityIssue>,
        escalated: Vec<QualityIssue>,
    },

    QualityGateAwaitingHuman {
        checkpoint: QualityCheckpoint,
        questions: Vec<EscalatedQuestion>,
        current_question_index: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QualityCheckpoint {
    PrePlanning,   // Clarify + Checklist
    PostPlan,      // Analyze consistency
    PostTasks,     // Analyze coverage
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QualityGateType {
    Clarify,
    Checklist,
    Analyze,
}
```

---

## Telemetry Schema

### Quality Gate Telemetry File

**File:** `quality-gate-{checkpoint}_{timestamp}.json`

```json
{
  "command": "quality-gate",
  "specId": "SPEC-KIT-065",
  "checkpoint": "pre-planning",
  "gates": ["clarify", "checklist"],
  "timestamp": "2025-10-16T20:00:00Z",
  "schemaVersion": "v1.1",

  "results": {
    "clarify": {
      "total_issues": 5,
      "auto_resolved": 3,
      "escalated": 2,
      "agent_roster": ["gemini", "claude", "code"],

      "auto_resolved_details": [
        {
          "issue_id": "CLR-1",
          "question": "Should we log failed auth attempts?",
          "agent_answers": {
            "gemini": "yes - security best practice",
            "claude": "yes - OWASP guideline",
            "code": "yes - audit trail"
          },
          "agreement": "unanimous",
          "applied_answer": "yes",
          "spec_modification": "spec.md:23 - Added logging requirement",
          "timestamp": "2025-10-16T20:02:15Z"
        }
      ],

      "escalated_details": [
        {
          "issue_id": "CLR-2",
          "question": "OAuth2 provider count?",
          "agent_answers": {
            "gemini": "multiple (2-3)",
            "claude": "single initially",
            "code": "multiple (Google + GitHub)"
          },
          "agreement": "none",
          "gpt5_validation": null,
          "human_answer": "multiple (Google, GitHub, Microsoft)",
          "spec_modification": "spec.md:12 - Added provider list",
          "timestamp": "2025-10-16T20:05:42Z"
        }
      ]
    },

    "checklist": {
      "total_requirements": 8,
      "below_threshold": 2,
      "auto_improved": 2,
      "escalated": 0,

      "improvements": [
        {
          "requirement_id": "R1",
          "original": "System shall authenticate users",
          "score_before": 3.2,
          "improved": "System shall authenticate users via OAuth2...",
          "score_after": 7.8,
          "agreement": "unanimous",
          "spec_modification": "spec.md:8"
        }
      ]
    }
  },

  "summary": {
    "total_issues": 7,
    "auto_resolved": 5,
    "escalated": 2,
    "files_modified": ["spec.md"],
    "human_time_seconds": 180
  }
}
```

---

## Implementation Breakdown

### Phase 1: State Machine & Types (6-8 hours)

**Files to modify:**
- `tui/src/chatwidget/spec_kit/state.rs` - Add quality gate types
- `tui/src/chatwidget/spec_kit/handler.rs` - Extend state machine

**New types:**
```rust
pub enum QualityCheckpoint { PrePlanning, PostPlan, PostTasks }
pub enum QualityGateType { Clarify, Checklist, Analyze }
pub struct QualityIssue { ... }
pub enum Confidence { High, Medium, Low }
pub enum Magnitude { Critical, Important, Minor }
pub enum Resolvability { AutoFix, SuggestFix, NeedHuman }
pub struct QualityGateOutcome { ... }
pub enum Resolution { AutoApply, Escalate }
```

**State transitions:**
```
Guardrail → QualityGateExecuting → QualityGateProcessing →
  ├─ QualityGateAwaitingHuman (if escalations) → Apply answers → Next stage
  └─ Next stage (if no escalations)
```

---

### Phase 2: Agent Prompts (4-6 hours)

**Files to modify:**
- `docs/spec-kit/prompts.json` - Add quality gate prompts

**New prompts:**
```json
"quality-gate-clarify": {
  "gemini": { "prompt": "Analyze SPEC for ambiguities. Output JSON with question, confidence, magnitude, resolvability, suggested_resolution" },
  "claude": { ... },
  "code": { ... }
}

"quality-gate-checklist": { ... }
"quality-gate-analyze": { ... }
```

**Each prompt includes:**
- Instructions for structured JSON output
- Classification guidelines (confidence/magnitude/resolvability)
- Auto-resolution rules
- Examples of each classification level

---

### Phase 3: Resolution Logic (8-10 hours)

**Files to create/modify:**
- `tui/src/chatwidget/spec_kit/quality.rs` (new module)

**Functions:**
```rust
pub fn execute_quality_checkpoint(
    ctx: &mut impl SpecKitContext,
    checkpoint: QualityCheckpoint,
    spec_id: &str,
) -> QualityCheckpointOutcome;

pub fn classify_issue_agreement(
    agent_answers: &[String]
) -> (usize, String);  // (count, majority_answer)

pub fn validate_with_gpt5(
    issue: &QualityIssue,
    majority_answer: &str,
    spec_content: &str,
    prd_content: Option<&str>,
) -> GPT5ValidationResult;

pub fn apply_auto_resolution(
    ctx: &mut impl SpecKitContext,
    issue: &QualityIssue,
    answer: &str,
) -> Result<FileModification>;

pub fn batch_escalations(
    issues: Vec<QualityIssue>
) -> Vec<EscalatedQuestion>;
```

**GPT-5 validation prompt:**
```
SPEC Content:
[Full spec.md]

PRD Content:
[Full PRD.md if exists]

Question: "{question}"

Agent Answers:
- Gemini (agrees): "{answer}" - Reasoning: "{reasoning}"
- Claude (agrees): "{answer}" - Reasoning: "{reasoning}"
- Code (disagrees): "{dissenting_answer}" - Reasoning: "{dissent_reasoning}"

Majority answer: "{majority_answer}"
Dissenting view: "{dissent_reasoning}"

Your task:
1. Analyze the SPEC's intent and requirements context
2. Evaluate whether the majority answer aligns with SPEC goals
3. Consider if the dissenting reasoning reveals a valid concern
4. Determine if majority answer should be applied or escalated

Output JSON:
{
  "agrees_with_majority": boolean,
  "reasoning": string (your analysis),
  "recommended_answer": string (if you disagree with majority),
  "confidence": "high" | "medium" | "low"
}
```

---

### Phase 4: File Modification Engine (6-8 hours)

**Files to create:**
- `tui/src/chatwidget/spec_kit/file_modifier.rs` (new module)

**Functions:**
```rust
pub fn apply_spec_modification(
    spec_path: &Path,
    modification: &SpecModification,
) -> Result<ModificationOutcome>;

pub enum SpecModification {
    AddRequirement { after_line: usize, text: String },
    UpdateRequirement { line: usize, new_text: String },
    AddSection { section: String, content: String },
    ReplaceText { pattern: String, replacement: String },
}

pub struct ModificationOutcome {
    file: PathBuf,
    changes: Vec<LineChange>,
    backup_path: Option<PathBuf>,
}
```

**Safety:**
- Create backup before modification
- Validate file structure after changes
- Log all modifications to telemetry
- Support rollback via backup restoration

---

### Phase 5: Escalation UI (8-10 hours)

**Files to modify:**
- `tui/src/chatwidget/mod.rs` - Add quality gate modal
- `tui/src/bottom_pane/mod.rs` - Quality question view

**UI Components:**
```rust
pub struct QualityGateModal {
    checkpoint: QualityCheckpoint,
    questions: Vec<EscalatedQuestion>,
    current_index: usize,
    answers: HashMap<String, String>,
}

pub struct EscalatedQuestion {
    id: String,
    gate_type: QualityGateType,
    question: String,
    context: String,
    agent_answers: HashMap<String, String>,
    gpt5_reasoning: Option<String>,
    magnitude: Magnitude,
    suggested_options: Vec<String>,
}
```

**Rendering:**
- Show checkpoint name
- Show N questions total
- Show current question with full context
- Show agent reasoning
- Show GPT-5 reasoning if applicable
- Input field for answer
- Progress indicator (Q1/N, Q2/N, etc.)

---

### Phase 6: Telemetry & Logging (4-6 hours)

**Files to modify:**
- `tui/src/chatwidget/spec_kit/evidence.rs` - Add quality gate telemetry

**Telemetry functions:**
```rust
pub fn persist_quality_gate_telemetry(
    repo: &dyn EvidenceRepository,
    spec_id: &str,
    checkpoint: QualityCheckpoint,
    outcome: &QualityCheckpointOutcome,
) -> Result<PathBuf>;

pub fn read_quality_gate_telemetry(
    repo: &dyn EvidenceRepository,
    spec_id: &str,
    checkpoint: QualityCheckpoint,
) -> Result<Option<QualityCheckpointOutcome>>;
```

**Git commit message template:**
```
quality-gates: auto-resolved {N} issues, {M} human-answered

Checkpoint 1 (Pre-Planning):
- Clarify: 3 auto-resolved, 2 human-answered
- Checklist: 2 auto-improved, 0 escalated

Checkpoint 2 (Post-Plan):
- Analyze: 2 auto-fixed, 1 human-answered

Checkpoint 3 (Post-Tasks):
- Analyze: 1 auto-fixed, 0 escalated

Files modified: spec.md, plan.md, tasks.md
Telemetry: quality-gate-pre-planning_{timestamp}.json
           quality-gate-post-plan_{timestamp}.json
           quality-gate-post-tasks_{timestamp}.json
```

---

### Phase 7: Testing (10-12 hours)

**Unit tests:**
- Test agreement classification (3/3, 2/3, 1/3, 0/3)
- Test resolution logic (unanimous, majority, escalate)
- Test GPT-5 validation mock
- Test file modification (add/update/replace)
- Test telemetry persistence

**Integration tests:**
- Test full checkpoint execution with mocks
- Test escalation batching
- Test git commit creation
- Test rollback via file restore

**E2E validation:**
- Run quality gates on test SPEC
- Verify auto-resolutions applied correctly
- Verify escalations shown properly
- Verify git commit created

---

## Total Implementation Effort

| Phase | Hours | Description |
|-------|-------|-------------|
| 1. State machine | 6-8 | Types, enums, state transitions |
| 2. Agent prompts | 4-6 | prompts.json entries, formatting |
| 3. Resolution logic | 8-10 | Classification, GPT-5 validation, decision logic |
| 4. File modifications | 6-8 | Safe file updates, backups |
| 5. Escalation UI | 8-10 | Modal, question view, input handling |
| 6. Telemetry | 4-6 | Logging, git commits |
| 7. Testing | 10-12 | Unit, integration, E2E |
| **Total** | **46-60 hours** | **~1.5 weeks focused work** |

---

## Cost Analysis

**Per pipeline run:**
- 4 quality gates × 3 agents = 12 agent calls
- ~8 min per gate = 32-40 min total
- GPT-5 validations: ~2-3 per pipeline (for 2/3 majorities)

**At 30 SPECs/month:**
- Agent cost: 30 × $11 (pipeline) + 30 × $3 (quality gates) = $420/month
- GPT-5 validations: 30 × 3 × $0.50 = $45/month
- **Total: ~$465/month** (vs $330 without quality gates)

**Added cost: $135/month**

At 30 SPECs/month, that's **$4.50 per SPEC** for quality assurance.

Acceptable if it prevents 1 bad SPEC from shipping.

---

## Risks (Real Talk)

**1. False Positives (5% estimated)**
- 30 SPECs × 17 auto-resolutions = 510 auto-modifications/month
- 5% error rate = **~25 wrong modifications/month**
- You catch these during post-pipeline review
- But if you don't review carefully, they ship

**Mitigation:** Make review summary very clear about what changed

**2. GPT-5 Validation Errors**
- GPT-5 might validate incorrect majority answers
- Or reject correct majority answers
- No data on GPT-5 validation accuracy yet

**Mitigation:** Track false positive/negative rates, tune prompts

**3. Interruption Fatigue**
- 3 checkpoints × ~1-2 questions each = 3-6 interruptions per pipeline
- At complex SPECs (like SPEC-025), could be 8-10 interruptions
- Users might disable quality gates to avoid interruptions

**Mitigation:** Make escalations very clear and valuable

**4. Implementation Complexity**
- State machine gets 3 new phases
- New UI modal for questions
- File modification engine
- GPT-5 integration
- All must work reliably

**Risk:** Bugs in quality gates could corrupt SPECs or halt pipelines

---

## My Honest Assessment

**You're committing to:**
- 46-60 hours of implementation
- $135/month additional cost
- 3 interruption points per pipeline
- Post-pipeline review burden
- 5% false positive risk

**You get:**
- 55% of quality issues auto-resolved
- Issues caught earlier in pipeline
- Higher quality SPECs automatically
- Reduced rework from missed ambiguities

**At 30+ SPECs/month, this math probably works.**

But this is a significant feature with real complexity and ongoing maintenance burden.

---

## FINAL QUESTION

**Do you want me to implement this?**

**YES:** I'll start with Phase 1 (state machine, ~6-8 hours)
**NO:** We document this design as T85 for future consideration
**DEFER:** You think about it, I work on something else (T78 integration tests? T79 service traits?)

**What's your call?**