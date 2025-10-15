# `/spec-auto` Full Automation Implementation Plan

> **Goal:** Make `/spec-auto` fully automated with zero user interaction. Only pause for errors or decision-requiring conflicts.

**Status:** Planning phase
**Owner:** Code
**Estimated effort:** 2-3 days
**Target completion:** 2025-10-08

---

## Current State Analysis

### What Works Today

**Guardrail automation (✅):**
```rust
// chatwidget.rs:17172-17182
match state.phase {
    SpecAutoPhase::Guardrail => {
        let command = guardrail_for_stage(stage);
        state.waiting_guardrail = Some(...);
        state.phase = SpecAutoPhase::Prompt;
        NextAction::RunGuardrail { command, args }
    }
}
```
- Guardrails run automatically via `handle_spec_ops_command()`
- Transitions to Prompt phase when complete

**Multi-agent prompt preparation (✅):**
```rust
// chatwidget.rs:17213-17220
NextAction::PresentPrompt { stage, spec_id, goal, summary } => {
    self.present_spec_prompt(stage, &spec_id, &goal, summary);
}
```
- Builds correct multi-agent prompt from `prompts.json`

**Agent execution infrastructure (✅):**
- `submit_user_message()` sends messages to backend (chatwidget.rs:4305)
- `send_user_messages_to_agent()` transmits to orchestrator (chatwidget.rs:5262-5321)
- Multi-agent orchestration already works (tested with `/plan`)

**Agent completion detection (✅):**
```rust
// chatwidget.rs:6758-6773
let all_agents_terminal = !self.agent_runtime.is_empty()
    && self.agent_runtime.values().all(|rt| rt.completed_at.is_some());
```
- TUI already knows when all agents finish
- Tracks agent status via `AgentStatusUpdateEvent` (line 6707)

**Consensus checking (✅):**
```rust
// chatwidget.rs:15114-15304 (run_spec_consensus)
// Already implemented, returns (lines, consensus_ok)
```

---

### What's Broken (Why It Pauses)

**`present_spec_prompt()` inserts instead of submits:**
```rust
// chatwidget.rs:17251-17257
if self.bottom_pane.composer_is_empty() {
    self.bottom_pane.clear_composer();
    self.bottom_pane.insert_str(&prompt);  // ❌ INSERTS, doesn't SUBMIT
    lines.push("Prompt inserted. Review and send to run agents.");  // ❌ WAITS FOR USER
}
```

**No agent completion hook for spec-auto:**
```rust
// chatwidget.rs:6758-6773
if all_agents_terminal {
    // Clears spinner, updates status
    // ❌ DOESN'T check if this is spec-auto
    // ❌ DOESN'T trigger consensus check
    // ❌ DOESN'T advance to next stage
}
```

**State machine doesn't track agent execution:**
```rust
// chatwidget.rs:17063-17081
enum SpecAutoPhase {
    Guardrail,  // ✓ Has this
    Prompt,     // ✓ Has this
    // ❌ MISSING: ExecutingAgents
    // ❌ MISSING: CheckingConsensus
}
```

---

## Implementation Plan

### Phase 1: Enhanced State Machine (Day 1 Morning)

**File:** `codex-rs/tui/src/chatwidget.rs`

**Change 1: Expand SpecAutoPhase enum (line ~17070)**

```rust
#[derive(Debug, Clone)]
enum SpecAutoPhase {
    Guardrail,

    // NEW: Track agent execution
    ExecutingAgents {
        // The request_id when we auto-submitted the multi-agent prompt
        // Used to match AgentStatusUpdate events to this spec-auto run
        request_id: u64,

        // Track which agents we're waiting for
        expected_agents: Vec<String>,  // ["gemini", "claude", "gpt_pro"]

        // Track which agents have completed
        // Populated from AgentStatusUpdateEvent as agents finish
        completed_agents: HashSet<String>,
    },

    // NEW: Checking consensus before advancing
    CheckingConsensus,
}
```

**Change 2: Update SpecAutoState initialization (line ~17084)**

```rust
impl SpecAutoState {
    fn new(spec_id: String, goal: String, resume_from: SpecStage) -> Self {
        // ... existing code ...
        Self {
            spec_id,
            goal,
            stages,
            current_index: start_index,
            phase: SpecAutoPhase::Guardrail,  // Start same as before
            waiting_guardrail: None,
            validate_retries: 0,
            pending_prompt_summary: None,
        }
    }

    // NEW: Get current stage
    fn current_stage(&self) -> Option<SpecStage> {
        self.stages.get(self.current_index).copied()
    }
}
```

**Effort:** 30 minutes
**Risk:** Low - data structure changes only

---

### Phase 2: Auto-Submit Mechanism (Day 1 Afternoon)

**Change 3: Modify present_spec_prompt to auto-submit (line ~17226)**

```rust
fn present_spec_prompt(
    &mut self,
    stage: SpecStage,
    spec_id: &str,
    goal: &str,
    summary: Option<String>,
) {
    let mut arg = spec_id.to_string();
    if !goal.trim().is_empty() {
        arg.push(' ');
        arg.push_str(goal.trim());
    }

    match spec_prompts::build_stage_prompt(stage, &arg) {
        Ok(prompt) => {
            let mut lines: Vec<ratatui::text::Line<'static>> = Vec::new();
            lines.push(ratatui::text::Line::from(format!(
                "Executing multi-agent {} for {}",
                stage.display_name(),
                spec_id
            )));
            if let Some(ref summary_text) = summary {
                lines.push(ratatui::text::Line::from(summary_text.clone()));
            }
            lines.push(ratatui::text::Line::from(
                "Launching Gemini, Claude, and GPT Pro..."
            ));

            self.history_push(crate::history_cell::PlainHistoryCell::new(
                lines,
                crate::history_cell::HistoryCellType::Notice,
            ));

            // NEW: Auto-submit instead of insert
            self.auto_submit_for_spec_auto(prompt, stage);
        }
        Err(err) => {
            self.halt_spec_auto_with_error(format!(
                "Failed to build {} prompt: {}",
                stage.display_name(),
                err
            ));
        }
    }
}
```

**Change 4: Add auto-submit helper (new function)**

```rust
fn auto_submit_for_spec_auto(&mut self, prompt: String, stage: SpecStage) {
    // Get the next request ID before submitting
    let request_id = self.current_request_index;

    // Update spec-auto state to track this agent execution
    if let Some(state) = self.spec_auto_state.as_mut() {
        state.phase = SpecAutoPhase::ExecutingAgents {
            request_id,
            expected_agents: vec![
                "gemini".to_string(),
                "claude".to_string(),
                "gpt_pro".to_string(),
            ],
            completed_agents: HashSet::new(),
        };
    }

    // Create user message
    let user_msg = UserMessage {
        display_text: format!(
            "/spec-{} auto-submitted by /spec-auto pipeline",
            stage.command_name()
        ),
        ordered_items: vec![InputItem::Text { text: prompt }],
    };

    // Submit via normal flow (adds to history, sends to backend)
    self.queued_user_messages.push(user_msg);
    self.flush_queued_user_messages();
}

fn flush_queued_user_messages(&mut self) {
    if self.queued_user_messages.is_empty() {
        return;
    }
    let batch = std::mem::take(&mut self.queued_user_messages);
    self.send_user_messages_to_agent(batch);
}
```

**Effort:** 2 hours
**Risk:** Medium - need to ensure request_id tracking is correct

---

### Phase 3: Agent Completion Detection (Day 1 Evening)

**Change 5: Hook into AgentStatusUpdateEvent handler (line 6758-6773)**

```rust
// In EventMsg::AgentStatusUpdate handler, AFTER existing all_agents_terminal check:

if all_agents_terminal {
    // ... existing spinner/status logic ...

    // NEW: Check if this is part of spec-auto pipeline
    self.on_spec_auto_agents_complete();
}
```

**Change 6: Add agent completion callback (new function)**

```rust
fn on_spec_auto_agents_complete(&mut self) {
    let Some(state) = self.spec_auto_state.as_ref() else { return; };

    // Only proceed if we're in ExecutingAgents phase
    let (request_id, expected_agents) = match &state.phase {
        SpecAutoPhase::ExecutingAgents { request_id, expected_agents, .. } => {
            (*request_id, expected_agents)
        }
        _ => return,  // Not in agent execution phase
    };

    // Verify this completion is for our request
    // (Could check request_id matches current_request_index, but all_agents_terminal
    //  already implies this is the latest request)

    // Track which agents completed
    let mut completed = HashSet::new();
    for agent_info in &self.active_agents {
        if matches!(agent_info.status, AgentStatus::Completed) {
            completed.insert(agent_info.name.clone());
        }
    }

    // Update state
    if let Some(state) = self.spec_auto_state.as_mut() {
        if let SpecAutoPhase::ExecutingAgents { completed_agents, .. } = &mut state.phase {
            *completed_agents = completed;
        }
    }

    // Check if we have all expected agents
    let all_expected_complete = expected_agents.iter()
        .all(|expected| completed.contains(expected));

    if all_expected_complete {
        // All agents done - move to consensus check
        if let Some(state) = self.spec_auto_state.as_mut() {
            state.phase = SpecAutoPhase::CheckingConsensus;
        }

        // Trigger consensus check and advance
        self.check_consensus_and_advance_spec_auto();
    } else {
        // Some agents failed or missing
        let missing: Vec<_> = expected_agents.iter()
            .filter(|a| !completed.contains(*a))
            .map(|s| s.as_str())
            .collect();

        self.halt_spec_auto_with_error(format!(
            "Agent execution incomplete. Missing: {:?}",
            missing
        ));
    }
}
```

**Effort:** 3 hours
**Risk:** Medium - need to correctly identify agent names and match them

---

### Phase 4: Consensus Check & Advance (Day 2 Morning)

**Change 7: Add consensus check callback (new function)**

```rust
fn check_consensus_and_advance_spec_auto(&mut self) {
    let Some(state) = self.spec_auto_state.as_ref() else { return; };

    let Some(current_stage) = state.current_stage() else {
        self.halt_spec_auto_with_error("Invalid stage index".to_string());
        return;
    };

    let spec_id = state.spec_id.clone();

    // Show checking status
    self.history_push(crate::history_cell::PlainHistoryCell::new(
        vec![ratatui::text::Line::from(format!(
            "Checking consensus for {}...",
            current_stage.display_name()
        ))],
        crate::history_cell::HistoryCellType::Notice,
    ));

    // Run consensus check (reuse existing logic)
    match self.run_spec_consensus(&spec_id, current_stage) {
        Ok((consensus_lines, consensus_ok)) => {
            // Show consensus results
            self.history_push(crate::history_cell::PlainHistoryCell::new(
                consensus_lines,
                if consensus_ok {
                    crate::history_cell::HistoryCellType::Notice
                } else {
                    crate::history_cell::HistoryCellType::Error
                },
            ));

            if consensus_ok {
                // Consensus OK - advance to next stage
                self.history_push(crate::history_cell::PlainHistoryCell::new(
                    vec![ratatui::text::Line::from(format!(
                        "✓ {} consensus OK - advancing",
                        current_stage.display_name()
                    ))],
                    crate::history_cell::HistoryCellType::Notice,
                ));

                // Move to next stage
                if let Some(state) = self.spec_auto_state.as_mut() {
                    state.phase = SpecAutoPhase::Guardrail;
                    state.current_index += 1;
                }

                // Trigger next stage
                self.advance_spec_auto();
            } else {
                // Consensus degraded/conflict - HALT
                self.halt_spec_auto_with_error(format!(
                    "Consensus {} for {} - see evidence above",
                    "failed",
                    current_stage.display_name()
                ));
            }
        }
        Err(err) => {
            // Error reading/parsing consensus - HALT
            self.halt_spec_auto_with_error(format!(
                "Failed to check consensus for {}: {}",
                current_stage.display_name(),
                err
            ));
        }
    }
}
```

**Change 8: Add halt helper (new function)**

```rust
fn halt_spec_auto_with_error(&mut self, reason: String) {
    self.history_push(crate::history_cell::PlainHistoryCell::new(
        vec![
            ratatui::text::Line::from("⚠ /spec-auto halted"),
            ratatui::text::Line::from(reason),
            ratatui::text::Line::from(""),
            ratatui::text::Line::from("Resolve the issue and re-run with:"),
            ratatui::text::Line::from(format!(
                "/spec-auto {} --from <stage>",
                self.spec_auto_state.as_ref().map(|s| s.spec_id.as_str()).unwrap_or("SPEC-ID")
            )),
        ],
        crate::history_cell::HistoryCellType::Error,
    ));

    // Clear state to stop pipeline
    self.spec_auto_state = None;
}
```

**Effort:** 2 hours
**Risk:** Low - simple integration of existing consensus logic

---

### Phase 5: Edge Cases & Error Handling (Day 2 Afternoon)

**Scenarios to handle:**

**1. Agent timeout (no completion after N minutes)**
```rust
// In AgentStatusUpdateEvent handler
// If spec-auto is running and agents haven't completed in 10 minutes, warn user

fn check_spec_auto_timeout(&mut self) {
    let Some(state) = self.spec_auto_state.as_ref() else { return; };

    if let SpecAutoPhase::ExecutingAgents { request_id, .. } = &state.phase {
        // Check if any agent has been running > 10 minutes
        let timeout_threshold = Duration::from_secs(600);
        let now = Instant::now();

        for (agent_id, runtime) in &self.agent_runtime {
            if let Some(started) = runtime.started_at {
                if runtime.completed_at.is_none() && now.duration_since(started) > timeout_threshold {
                    self.history_push(crate::history_cell::PlainHistoryCell::new(
                        vec![ratatui::text::Line::from(format!(
                            "⚠ Agent {} running for >10 minutes - may be stuck",
                            agent_id
                        ))],
                        crate::history_cell::HistoryCellType::Warning,
                    ));
                }
            }
        }
    }
}
```

**2. Guardrail failure (non-zero exit)**
```rust
// Already handled in existing code
// Just ensure halt_spec_auto_with_error is called
```

**3. User interruption (Ctrl+C or /quit during execution)**
```rust
// On quit/cancel:
if self.spec_auto_state.is_some() {
    self.history_push(crate::history_cell::PlainHistoryCell::new(
        vec![ratatui::text::Line::from("/spec-auto interrupted by user")],
        crate::history_cell::HistoryCellType::Warning,
    ));
    self.spec_auto_state = None;
}
```

**4. Evidence write failure (disk full, permissions)**
```rust
// In telemetry hook, bubble up errors:
if let Err(err) = self.write_consensus_evidence(...) {
    self.halt_spec_auto_with_error(format!("Evidence write failed: {}", err));
}
```

**Effort:** 3 hours
**Risk:** Low - mostly defensive checks

---

### Phase 6: Request ID Tracking (Day 2 Evening)

**Challenge:** Need to associate agent responses with the specific spec-auto request.

**Solution approach:**

**Option A: Track current request globally**
```rust
// In SpecAutoPhase::ExecutingAgents, store request_id
// When agents complete, verify they're for this request_id
// Problem: How do we get request_id from AgentStatusUpdateEvent?
```

**Option B: Simpler - Assume latest agents are for spec-auto**
```rust
// If spec_auto_state is in ExecutingAgents phase
// AND all_agents_terminal is true
// THEN these agents must be for our spec-auto run
// Because: spec-auto submissions are sequential, can't have overlapping agent runs
```

**Recommendation: Option B** - Simpler, valid assumption for sequential execution.

**Implementation:**
```rust
fn on_spec_auto_agents_complete(&mut self) {
    let Some(state) = self.spec_auto_state.as_ref() else { return; };

    // Only proceed if we're in ExecutingAgents phase
    if !matches!(state.phase, SpecAutoPhase::ExecutingAgents { .. }) {
        return;
    }

    // Since spec-auto is sequential, these must be our agents
    // Just verify we got the expected ones

    let mut completed_names = HashSet::new();
    for agent in &self.active_agents {
        if matches!(agent.status, AgentStatus::Completed) {
            // Normalize agent names (might be "Gemini" or "gemini")
            completed_names.insert(agent.name.to_lowercase());
        }
    }

    // Update state
    if let Some(state) = self.spec_auto_state.as_mut() {
        if let SpecAutoPhase::ExecutingAgents { expected_agents, completed_agents, .. } = &mut state.phase {
            *completed_agents = completed_names.clone();

            // Check if all expected present
            let all_present = expected_agents.iter()
                .all(|exp| completed_names.contains(&exp.to_lowercase()));

            if all_present {
                // Move to consensus check
                state.phase = SpecAutoPhase::CheckingConsensus;
                self.check_consensus_and_advance_spec_auto();
            }
            // If not all present, wait (might be phased completion)
        }
    }
}
```

**Effort:** 2 hours
**Risk:** Medium - agent name matching might need normalization

---

### Phase 7: Integration & Testing (Day 3)

**Test 1: Single-stage execution**
```rust
#[test]
fn spec_auto_executes_plan_stage_automatically() {
    let workspace = TempDir::new().unwrap();
    setup_spec_fixture(&workspace, "SPEC-AUTO-TEST");

    let mut chat = make_chat_with_workspace(&workspace);

    // Mock guardrail success
    mock_guardrail_command_success("spec-ops-plan");

    // Mock agent responses
    mock_agent_status_updates(vec![
        ("gemini", AgentStatus::Running),
        ("claude", AgentStatus::Running),
        ("gpt_pro", AgentStatus::Running),
    ]);

    // Trigger /spec-auto
    chat.handle_spec_auto_command(SpecAutoInvocation {
        spec_id: "SPEC-AUTO-TEST".to_string(),
        goal: "test".to_string(),
        resume_from: SpecStage::Plan,
    });

    // Verify auto-submitted (not inserted into composer)
    assert!(chat.bottom_pane.composer_is_empty(), "Should auto-submit, not insert");

    // Simulate agent completion
    mock_agent_status_updates(vec![
        ("gemini", AgentStatus::Completed),
        ("claude", AgentStatus::Completed),
        ("gpt_pro", AgentStatus::Completed),
    ]);

    // Trigger status update event
    chat.handle_agent_status_update(...);

    // Verify consensus check was triggered
    assert!(evidence_dir.join("spec-plan_*_synthesis.json").exists());
}
```

**Test 2: Multi-stage pipeline**
```rust
#[test]
fn spec_auto_advances_through_multiple_stages() {
    // Mock successful plan → tasks → implement
    // Verify each stage executes without user intervention
    // Verify consensus checked between each stage
}
```

**Test 3: Halt on conflict**
```rust
#[test]
fn spec_auto_halts_when_consensus_conflicts() {
    // Run plan stage
    // Mock synthesis with conflicts
    // Verify pipeline halts
    // Verify tasks stage never executes
}
```

**Test 4: Halt on missing agent**
```rust
#[test]
fn spec_auto_halts_when_agent_fails() {
    // Mock 2 agents complete, 1 fails
    // Verify pipeline halts with clear error
}
```

**Effort:** 4-6 hours
**Risk:** High - integration tests may reveal edge cases

---

## Modified Code Locations Summary

| File | Lines Modified | Changes |
|------|----------------|---------|
| `chatwidget.rs` | 17063-17081 | Expand SpecAutoPhase enum |
| `chatwidget.rs` | 17084-17108 | Add SpecAutoState helpers |
| `chatwidget.rs` | 17226-17260 | Modify present_spec_prompt (auto-submit) |
| `chatwidget.rs` | New ~50 lines | Add auto_submit_for_spec_auto() |
| `chatwidget.rs` | New ~30 lines | Add flush_queued_user_messages() helper |
| `chatwidget.rs` | 6758-6773 | Hook all_agents_terminal → spec-auto callback |
| `chatwidget.rs` | New ~60 lines | Add on_spec_auto_agents_complete() |
| `chatwidget.rs` | New ~70 lines | Add check_consensus_and_advance_spec_auto() |
| `chatwidget.rs` | New ~25 lines | Add halt_spec_auto_with_error() |
| `chatwidget.rs` | New ~200 lines | Add integration tests |

**Total lines added/modified:** ~500 lines across chatwidget.rs

---

## State Machine Flow Diagram

```
┌─────────────────────────────────────────────────────────────┐
│ /spec-auto SPEC-ID --from <stage>                          │
└───────────────────────┬─────────────────────────────────────┘
                        ▼
            ┌───────────────────────┐
            │ SpecAutoPhase:        │
            │ Guardrail             │
            └───────┬───────────────┘
                    ▼
            ┌───────────────────────┐
            │ Run /spec-ops-<stage> │
            │ (shell command)        │
            └───────┬───────────────┘
                    ▼
          Success? ─┬─ No → HALT with error
                    │
                   Yes
                    ▼
            ┌───────────────────────┐
            │ SpecAutoPhase:        │
            │ ExecutingAgents       │
            │ {request_id,          │
            │  expected_agents,     │
            │  completed_agents}    │
            └───────┬───────────────┘
                    ▼
            ┌───────────────────────┐
            │ Auto-submit           │
            │ multi-agent prompt    │
            │ (no user interaction) │
            └───────┬───────────────┘
                    ▼
            ┌───────────────────────┐
            │ Agents execute:       │
            │ Gemini → Claude → GPT │
            └───────┬───────────────┘
                    ▼
            ┌───────────────────────┐
            │ Monitor via           │
            │ AgentStatusUpdate     │
            │ events                │
            └───────┬───────────────┘
                    ▼
      All complete? ─┬─ No → Wait (or timeout)
                     │
                    Yes
                     ▼
            ┌───────────────────────┐
            │ on_spec_auto_agents_  │
            │ complete() triggered  │
            └───────┬───────────────┘
                    ▼
            ┌───────────────────────┐
            │ SpecAutoPhase:        │
            │ CheckingConsensus     │
            └───────┬───────────────┘
                    ▼
            ┌───────────────────────┐
            │ run_spec_consensus()  │
            │ - Read synthesis.json │
            │ - Check status        │
            └───────┬───────────────┘
                    ▼
        Consensus OK? ─┬─ No → HALT with evidence path
                       │
                      Yes
                       ▼
            ┌───────────────────────┐
            │ Phase = Guardrail     │
            │ current_index++       │
            └───────┬───────────────┘
                    ▼
            ┌───────────────────────┐
            │ advance_spec_auto()   │
            │ (loop to next stage)  │
            └───────┬───────────────┘
                    ▼
          More stages? ─┬─ No → Pipeline Complete ✓
                        │
                       Yes → Back to Guardrail phase
```

---

## Halt Conditions (When Pipeline Stops)

1. **Guardrail failure** - Shell command returns non-zero
2. **Agent execution failure** - One or more agents fail/timeout
3. **Missing agents** - Expected 3, got <3
4. **Consensus conflict** - synthesis.json has non-empty conflicts array
5. **Consensus degraded** - synthesis.json status = "degraded"
6. **Evidence write error** - Can't persist telemetry/synthesis
7. **User interruption** - Ctrl+C or /quit during execution

**All halt conditions show:**
- Clear error message
- Evidence file paths (if available)
- Resume command: `/spec-auto SPEC-ID --from <stage>`

---

## Integration Points

### With Existing Telemetry (T23)

**Line:** chatwidget.rs:2531-2698 (try_capture_spec_kit_telemetry)

**Current behavior:**
- Writes evidence when SPEC_KIT_TELEMETRY_ENABLED=1
- Called during agent status updates

**Integration:**
- Telemetry hook continues to work as-is
- Evidence files written automatically
- Consensus check reads these files

**No changes needed** - telemetry already works.

### With Consensus Checking (T24)

**Line:** chatwidget.rs:15114-15304 (run_spec_consensus)

**Current behavior:**
- Reads synthesis.json from evidence dir
- Returns (lines, consensus_ok)

**Integration:**
- Called from check_consensus_and_advance_spec_auto()
- Halt if consensus_ok == false
- Continue if consensus_ok == true

**No changes needed** - consensus checking already works.

### With Guardrail Commands

**Line:** chatwidget.rs:17209-17211 (RunGuardrail action)

**Current behavior:**
- Calls handle_spec_ops_command()
- Returns when command completes
- Advances to Prompt phase

**Integration:**
- Continue using existing flow
- Guardrail → Prompt → Auto-submit → ExecutingAgents

**No changes needed** - guardrail execution already works.

---

## Risks & Mitigations

### Risk 1: Agent Name Matching

**Problem:** Agent names might be "Gemini" vs "gemini" vs "gemini-2.5-pro"

**Mitigation:**
```rust
fn normalize_agent_name(name: &str) -> String {
    // Extract base name before any version/variant suffix
    name.to_lowercase()
        .split('-')
        .next()
        .unwrap_or(name)
        .to_string()
}

// Use normalized names for matching:
expected_agents: vec!["gemini", "claude", "gpt"]
```

### Risk 2: Race Condition (Agents Complete Before State Updated)

**Problem:** all_agents_terminal fires before we set ExecutingAgents phase

**Mitigation:**
- Set phase BEFORE calling send_user_messages_to_agent()
- Agents can't complete before message is sent
- Sequential execution prevents race

### Risk 3: Multiple /spec-auto Sessions

**Problem:** User runs two `/spec-auto` commands in different TUI instances

**Mitigation:**
- Evidence writes use timestamps (no collision)
- Each TUI has independent spec_auto_state
- Git conflicts would prevent simultaneous SPEC.md updates

### Risk 4: Incomplete Evidence (Telemetry Hook Fails)

**Problem:** Agents complete but telemetry hook doesn't write evidence

**Mitigation:**
```rust
// In check_consensus_and_advance_spec_auto():
match self.run_spec_consensus(&spec_id, current_stage) {
    Err(err) if err.contains("No synthesis found") => {
        self.halt_spec_auto_with_error(format!(
            "Evidence not written - check SPEC_KIT_TELEMETRY_ENABLED=1"
        ));
    }
    // ... other cases ...
}
```

---

## Testing Strategy

### Unit Tests (Per-Function)

- `test_auto_submit_sets_executing_phase()`
- `test_agent_completion_triggers_consensus_check()`
- `test_consensus_ok_advances_to_next_stage()`
- `test_consensus_conflict_halts_pipeline()`
- `test_missing_agent_halts_pipeline()`

### Integration Tests (Full Flow)

- `test_spec_auto_completes_full_pipeline()`
- `test_spec_auto_halts_on_first_conflict()`
- `test_spec_auto_halts_on_guardrail_failure()`
- `test_spec_auto_resumes_from_specific_stage()`

### Manual Validation (Real Usage)

1. Run `/spec-auto SPEC-KIT-DEMO --from plan`
2. Watch TUI for:
   - Auto-submission confirmations
   - Agent execution progress
   - Consensus checks between stages
   - Successful advancement or halt
3. Verify evidence files written for each stage
4. Test conflict injection and halt behavior

---

## Timeline Breakdown

### Day 1: State Machine & Auto-Submit
- **Morning (3h):** Phase 1 - Enhanced state machine
- **Afternoon (3h):** Phase 2 - Auto-submit mechanism
- **Evening (2h):** Phase 3 - Agent completion detection
- **Total:** 8 hours

### Day 2: Consensus & Edge Cases
- **Morning (2h):** Phase 4 - Consensus check & advance
- **Afternoon (3h):** Phase 5 - Edge case handling
- **Evening (2h):** Phase 6 - Request ID tracking
- **Total:** 7 hours

### Day 3: Testing & Validation
- **Morning (3h):** Unit tests
- **Afternoon (3h):** Integration tests
- **Evening (2h):** Manual E2E validation
- **Total:** 8 hours

**Grand total:** 23 hours (~3 days focused work)

---

## Success Criteria

### Must Have (MVP)
- [ ] `/spec-auto SPEC-ID` runs all stages without user input
- [ ] Evidence written for each stage (per-agent + synthesis + telemetry)
- [ ] Pipeline halts on consensus conflict with clear error
- [ ] Pipeline halts on missing agents
- [ ] User sees progress updates (which stage running, agents executing)
- [ ] Final message: "Pipeline complete" or "Halted at <stage>"

### Nice to Have (V2)
- [ ] Progress indicator (Stage 2/6: tasks - agents running)
- [ ] Estimated time remaining
- [ ] Pauseable/resumable (Ctrl+Z to pause, /spec-auto resume)
- [ ] Parallel stage execution (where possible)
- [ ] Cost tracking display during execution

---

## Alternative: Simpler Approach (If Timeline Too Long)

**Use `/spec-ops-auto` wrapper that calls consensus_runner.sh:**

**Modify:** `scripts/spec_ops_004/spec_auto.sh`

```bash
# After each guardrail stage:
"${COMMAND_DIR}/spec_ops_plan.sh" "${SPEC_ID}"

# Add consensus execution
if [[ "${CONSENSUS_ENABLED:-1}" == "1" ]]; then
  "${SCRIPT_DIR}/consensus_runner.sh" \
    --stage spec-plan \
    --spec "${SPEC_ID}" \
    --execute || exit 1

  # Check synthesis and halt if needed
  python3 "${SCRIPT_DIR}/check_synthesis.py" \
    --spec "${SPEC_ID}" \
    --stage spec-plan || exit 1
fi

# Continue to next stage...
```

**Pros:**
- 4-6 hours work instead of 3 days
- Fully automated immediately
- Uses existing bash infrastructure

**Cons:**
- Evidence written by bash, not TUI telemetry hooks
- Less integrated with TUI
- Two separate execution paths (bash vs TUI)

---

## Recommendation

**For 3-day implementation:**
- Proceed with TUI-native automation (Phases 1-7)
- Clean, integrated, uses telemetry hooks
- Better long-term architecture

**For ship-fast approach:**
- Enhance `/spec-ops-auto` bash script
- Working in 1 day
- Can migrate to TUI later

**Your call: Clean architecture (3 days) or quick win (1 day)?**
