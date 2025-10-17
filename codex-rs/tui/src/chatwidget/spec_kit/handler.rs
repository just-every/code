//! Spec-Kit command handlers as free functions
//!
//! These functions are extracted from chatwidget.rs to isolate spec-kit code.
//! Using free functions instead of methods to avoid Rust borrow checker issues.

use super::super::ChatWidget; // Parent module (friend access to private fields)
use super::evidence::EvidenceRepository;
use super::state::{GuardrailWait, SpecAutoPhase};
use crate::app_event::BackgroundPlacement;
use crate::history_cell::HistoryCellType;
use crate::slash_command::{HalMode, SlashCommand};
use crate::spec_prompts::SpecStage;
use crate::spec_status::{SpecStatusArgs, collect_report, degraded_warning, render_dashboard};
use codex_core::protocol::InputItem;

/// Handle /speckit.status command (native dashboard)
pub fn handle_spec_status(widget: &mut ChatWidget, raw_args: String) {
    let trimmed = raw_args.trim();
    let args = match SpecStatusArgs::from_input(trimmed) {
        Ok(args) => args,
        Err(err) => {
            widget.history_push(crate::history_cell::new_error_event(err.to_string()));
            widget.request_redraw();
            return;
        }
    };

    match collect_report(&widget.config.cwd, args) {
        Ok(report) => {
            let mut lines = render_dashboard(&report);
            if let Some(warning) = degraded_warning(&report) {
                lines.insert(1, warning);
            }
            let message = lines.join("\n");
            widget.insert_background_event_with_placement(message, BackgroundPlacement::Tail);
            widget.request_redraw();
        }
        Err(err) => {
            widget.history_push(crate::history_cell::new_error_event(format!(
                "spec-status failed: {err}"
            )));
            widget.request_redraw();
        }
    }
}

/// Halt /speckit.auto pipeline with error message
pub fn halt_spec_auto_with_error(widget: &mut ChatWidget, reason: String) {
    let resume_hint = widget
        .spec_auto_state
        .as_ref()
        .and_then(|state| {
            state.current_stage().map(|stage| {
                format!(
                    "/spec-auto {} --from {}",
                    state.spec_id,
                    stage.command_name()
                )
            })
        })
        .unwrap_or_default();

    widget.history_push(crate::history_cell::PlainHistoryCell::new(
        vec![
            ratatui::text::Line::from("⚠ /spec-auto halted"),
            ratatui::text::Line::from(reason),
            ratatui::text::Line::from(""),
            ratatui::text::Line::from("Resume with:"),
            ratatui::text::Line::from(resume_hint),
        ],
        HistoryCellType::Error,
    ));

    widget.spec_auto_state = None;
}

/// Handle /spec-consensus command (inspect consensus artifacts)
pub fn handle_spec_consensus(widget: &mut ChatWidget, raw_args: String) {
    handle_spec_consensus_impl(widget, raw_args);
}

/// Handle /guardrail.* and /spec-ops-* commands (guardrail validation)
pub fn handle_guardrail(
    widget: &mut ChatWidget,
    command: crate::slash_command::SlashCommand,
    raw_args: String,
    hal_override: Option<crate::slash_command::HalMode>,
) {
    // Delegate to guardrail module implementation
    super::guardrail::handle_guardrail_impl(widget, command, raw_args, hal_override);
}

// === Spec Auto Pipeline Methods ===

/// Handle /speckit.auto command initiation
pub fn handle_spec_auto(
    widget: &mut ChatWidget,
    spec_id: String,
    goal: String,
    resume_from: SpecStage,
    hal_mode: Option<HalMode>,
) {
    let mut header: Vec<ratatui::text::Line<'static>> = Vec::new();
    header.push(ratatui::text::Line::from(format!("/spec-auto {}", spec_id)));
    if !goal.trim().is_empty() {
        header.push(ratatui::text::Line::from(format!("Goal: {}", goal)));
    }
    header.push(ratatui::text::Line::from(format!(
        "Resume from: {}",
        resume_from.display_name()
    )));
    match hal_mode {
        Some(HalMode::Live) => header.push(ratatui::text::Line::from("HAL mode: live")),
        Some(HalMode::Mock) => header.push(ratatui::text::Line::from("HAL mode: mock")),
        None => header.push(ratatui::text::Line::from("HAL mode: mock (default)")),
    }
    widget.history_push(crate::history_cell::PlainHistoryCell::new(
        header,
        HistoryCellType::Notice,
    ));

    widget.spec_auto_state = Some(super::state::SpecAutoState::new(
        spec_id,
        goal,
        resume_from,
        hal_mode,
    ));
    advance_spec_auto(widget);
}

/// Advance spec-auto pipeline to next stage
pub fn advance_spec_auto(widget: &mut ChatWidget) {
    if widget.spec_auto_state.is_none() {
        return;
    }
    if widget
        .spec_auto_state
        .as_ref()
        .and_then(|state| state.waiting_guardrail.as_ref())
        .is_some()
    {
        return;
    }

    enum NextAction {
        PipelineComplete,
        RunGuardrail {
            command: SlashCommand,
            args: String,
            hal_mode: Option<HalMode>,
        },
    }

    loop {
        let next_action = {
            let Some(state) = widget.spec_auto_state.as_mut() else {
                return;
            };

            if state.current_index >= state.stages.len() {
                NextAction::PipelineComplete
            } else {
                let stage = state.stages[state.current_index];
                let hal_mode = state.hal_mode;

                // Check if we should run a quality checkpoint before this stage
                if state.quality_gates_enabled {
                    if let Some(checkpoint) = determine_quality_checkpoint(stage, &state.completed_checkpoints) {
                        // Execute quality checkpoint instead of proceeding to guardrail
                        execute_quality_checkpoint(widget, checkpoint);
                        return;
                    }
                }

                match &state.phase {
                    SpecAutoPhase::Guardrail => {
                        let command = super::state::guardrail_for_stage(stage);
                        let args = state.spec_id.clone();
                        state.waiting_guardrail = Some(GuardrailWait {
                            stage,
                            command,
                            task_id: None,
                        });
                        NextAction::RunGuardrail {
                            command,
                            args,
                            hal_mode,
                        }
                    }
                    SpecAutoPhase::ExecutingAgents { .. } => {
                        return;
                    }
                    SpecAutoPhase::CheckingConsensus => {
                        return;
                    }
                    // Quality gate phases
                    SpecAutoPhase::QualityGateExecuting { .. } => {
                        return; // Waiting for quality gate agents
                    }
                    SpecAutoPhase::QualityGateProcessing { .. } => {
                        return; // Processing results
                    }
                    SpecAutoPhase::QualityGateValidating { .. } => {
                        return; // Waiting for GPT-5 validation responses
                    }
                    SpecAutoPhase::QualityGateAwaitingHuman { .. } => {
                        return; // Waiting for human input
                    }
                }
            }
        };

        match next_action {
            NextAction::PipelineComplete => {
                // Finalize quality gates if enabled
                if let Some(state) = widget.spec_auto_state.as_ref() {
                    if state.quality_gates_enabled && !state.quality_checkpoint_outcomes.is_empty() {
                        finalize_quality_gates(widget);
                    }
                }

                widget.history_push(crate::history_cell::PlainHistoryCell::new(
                    vec![ratatui::text::Line::from("/spec-auto pipeline complete")],
                    HistoryCellType::Notice,
                ));
                widget.spec_auto_state = None;
                return;
            }
            NextAction::RunGuardrail {
                command,
                args,
                hal_mode,
            } => {
                widget.handle_spec_ops_command(command, args, hal_mode);
                return;
            }
        }
    }
}

/// Handle spec-auto task started event
pub fn on_spec_auto_task_started(widget: &mut ChatWidget, task_id: &str) {
    if let Some(state) = widget.spec_auto_state.as_mut() {
        if let Some(wait) = state.waiting_guardrail.as_mut() {
            if wait.task_id.is_none() {
                wait.task_id = Some(task_id.to_string());
            }
        }
    }
}

/// Handle spec-auto task completion (guardrail finished)
pub fn on_spec_auto_task_complete(widget: &mut ChatWidget, task_id: &str) {
    let (spec_id, stage) = {
        let Some(state) = widget.spec_auto_state.as_mut() else {
            return;
        };
        let Some(wait) = state.waiting_guardrail.take() else {
            return;
        };
        let Some(expected_id) = wait.task_id.as_deref() else {
            state.waiting_guardrail = Some(wait);
            return;
        };
        if expected_id != task_id {
            state.waiting_guardrail = Some(wait);
            return;
        }
        (state.spec_id.clone(), wait.stage)
    };

    match widget.collect_guardrail_outcome(&spec_id, stage) {
        Ok(outcome) => {
            {
                let Some(state) = widget.spec_auto_state.as_mut() else {
                    return;
                };
                let mut prompt_summary = outcome.summary.clone();
                if !outcome.failures.is_empty() {
                    prompt_summary.push_str(" | Failures: ");
                    prompt_summary.push_str(&outcome.failures.join(", "));
                }
                state.pending_prompt_summary = Some(prompt_summary);
            }

            let mut lines: Vec<ratatui::text::Line<'static>> = Vec::new();
            lines.push(ratatui::text::Line::from(format!(
                "[Spec Ops] {} stage: {}",
                stage.display_name(),
                outcome.summary
            )));
            if let Some(path) = &outcome.telemetry_path {
                lines.push(ratatui::text::Line::from(format!(
                    "  Telemetry: {}",
                    path.display()
                )));
            }
            if !outcome.failures.is_empty() {
                for failure in &outcome.failures {
                    lines.push(ratatui::text::Line::from(format!("  • {failure}")));
                }
            }
            widget.history_push(crate::history_cell::PlainHistoryCell::new(
                lines,
                HistoryCellType::Notice,
            ));

            if !outcome.success {
                if stage == SpecStage::Validate {
                    let (exhausted, retry_message) = {
                        let Some(state) = widget.spec_auto_state.as_mut() else {
                            return;
                        };
                        const SPEC_AUTO_MAX_VALIDATE_RETRIES: u32 = 2;
                        if state.validate_retries >= SPEC_AUTO_MAX_VALIDATE_RETRIES {
                            (true, None)
                        } else {
                            state.validate_retries += 1;
                            let insert_at = state.current_index + 1;
                            state.stages.splice(
                                insert_at..insert_at,
                                vec![SpecStage::Implement, SpecStage::Validate],
                            );
                            (
                                false,
                                Some(format!(
                                    "Retrying implementation/validation cycle (attempt {}).",
                                    state.validate_retries + 1
                                )),
                            )
                        }
                    };

                    if exhausted {
                        widget.history_push(crate::history_cell::PlainHistoryCell::new(
                            vec![ratatui::text::Line::from(
                                "Validation failed repeatedly; stopping /spec-auto pipeline.",
                            )],
                            HistoryCellType::Error,
                        ));
                        widget.spec_auto_state = None;
                        return;
                    }

                    if let Some(message) = retry_message {
                        widget.history_push(crate::history_cell::PlainHistoryCell::new(
                            vec![ratatui::text::Line::from(message)],
                            HistoryCellType::Notice,
                        ));
                    }
                } else {
                    widget.history_push(crate::history_cell::new_error_event(
                        "Guardrail step failed; aborting /spec-auto pipeline.".to_string(),
                    ));
                    widget.spec_auto_state = None;
                    return;
                }
            }

            match widget.run_spec_consensus(&spec_id, stage) {
                Ok((consensus_lines, ok)) => {
                    let cell = crate::history_cell::PlainHistoryCell::new(
                        consensus_lines,
                        if ok {
                            HistoryCellType::Notice
                        } else {
                            HistoryCellType::Error
                        },
                    );
                    widget.history_push(cell);
                    if !ok {
                        widget.history_push(crate::history_cell::PlainHistoryCell::new(
                            vec![ratatui::text::Line::from(format!(
                                "/spec-auto paused: resolve consensus for {} before continuing.",
                                stage.display_name()
                            ))],
                            HistoryCellType::Error,
                        ));
                        widget.spec_auto_state = None;
                        return;
                    }
                }
                Err(err) => {
                    widget.history_push(crate::history_cell::new_error_event(format!(
                        "Consensus check failed for {}: {}",
                        stage.display_name(),
                        err
                    )));
                    widget.spec_auto_state = None;
                    return;
                }
            }

            // After guardrail success and consensus check OK, auto-submit multi-agent prompt
            auto_submit_spec_stage_prompt(widget, stage, &spec_id);
        }
        Err(err) => {
            widget.history_push(crate::history_cell::new_error_event(format!(
                "Unable to read telemetry for {}: {}",
                stage.display_name(),
                err
            )));
            widget.spec_auto_state = None;
        }
    }
}

/// Auto-submit multi-agent prompt for spec stage
pub fn auto_submit_spec_stage_prompt(widget: &mut ChatWidget, stage: SpecStage, spec_id: &str) {
    let goal = widget
        .spec_auto_state
        .as_ref()
        .map(|s| s.goal.clone())
        .unwrap_or_default();

    let mut arg = spec_id.to_string();
    if !goal.trim().is_empty() {
        arg.push(' ');
        arg.push_str(goal.trim());
    }

    match crate::spec_prompts::build_stage_prompt(stage, &arg) {
        Ok(prompt) => {
            let mut lines: Vec<ratatui::text::Line<'static>> = Vec::new();
            lines.push(ratatui::text::Line::from(format!(
                "Auto-executing multi-agent {} for {}",
                stage.display_name(),
                spec_id
            )));
            lines.push(ratatui::text::Line::from(
                "Launching Gemini, Claude, and GPT Pro...",
            ));

            widget.history_push(crate::history_cell::PlainHistoryCell::new(
                lines,
                HistoryCellType::Notice,
            ));

            // Update state to ExecutingAgents phase BEFORE submitting
            let expected_agents: Vec<String> = widget
                .config
                .agents
                .iter()
                .filter(|a| a.enabled)
                .map(|a| a.name.clone())
                .collect();

            if let Some(state) = widget.spec_auto_state.as_mut() {
                state.phase = SpecAutoPhase::ExecutingAgents {
                    expected_agents,
                    completed_agents: std::collections::HashSet::new(),
                };
            }

            // Create and submit user message
            let user_msg = super::super::message::UserMessage {
                display_text: format!("[spec-auto] {} stage for {}", stage.display_name(), spec_id),
                ordered_items: vec![InputItem::Text { text: prompt }],
            };

            widget.submit_user_message(user_msg);
        }
        Err(err) => {
            halt_spec_auto_with_error(
                widget,
                format!("Failed to build {} prompt: {}", stage.display_name(), err),
            );
        }
    }
}

/// Handle all agents completing their tasks
pub fn on_spec_auto_agents_complete(widget: &mut ChatWidget) {
    let Some(state) = widget.spec_auto_state.as_ref() else {
        return;
    };

    // Check which phase we're in
    let expected_agents = match &state.phase {
        SpecAutoPhase::ExecutingAgents {
            expected_agents, ..
        } => expected_agents.clone(),
        SpecAutoPhase::QualityGateExecuting {
            expected_agents, ..
        } => expected_agents.clone(),
        _ => return, // Not in agent execution phase
    };

    // Collect which agents completed successfully
    let mut completed_names = std::collections::HashSet::new();
    for agent_info in &widget.active_agents {
        if matches!(agent_info.status, super::super::AgentStatus::Completed) {
            completed_names.insert(agent_info.name.to_lowercase());
        }
    }

    // Update completed agents in state and determine phase type
    let phase_type = if let Some(state) = widget.spec_auto_state.as_mut() {
        match &mut state.phase {
            SpecAutoPhase::ExecutingAgents {
                completed_agents, ..
            } => {
                *completed_agents = completed_names.clone();
                "regular"
            }
            SpecAutoPhase::QualityGateExecuting {
                completed_agents, ..
            } => {
                *completed_agents = completed_names.clone();
                "quality_gate"
            }
            SpecAutoPhase::QualityGateValidating { .. } => {
                // GPT-5 validation phase - single agent (GPT-5)
                "gpt5_validation"
            }
            _ => "none",
        }
    } else {
        "none"
    };

    // Handle different phase types
    match phase_type {
        "quality_gate" => {
            // Check if all quality gate agents completed
            let all_complete = expected_agents
                .iter()
                .all(|exp| completed_names.contains(&exp.to_lowercase()));

            if all_complete {
                on_quality_gate_agents_complete(widget);
            }
        }
        "gpt5_validation" => {
            // GPT-5 validation completing - check local-memory
            on_gpt5_validations_complete(widget);
        }
        "regular" => {
            // Regular stage agents
            let all_complete = expected_agents
                .iter()
                .all(|exp| completed_names.contains(&exp.to_lowercase()));

            if all_complete {
                if let Some(state) = widget.spec_auto_state.as_mut() {
                    state.phase = SpecAutoPhase::CheckingConsensus;
                }
                check_consensus_and_advance_spec_auto(widget);
            }
        }
        _ => {}
    }

    // Check for failures in any phase
    if !matches!(phase_type, "gpt5_validation") {
        // Check for failed agents
        let has_failures = widget
            .active_agents
            .iter()
            .any(|a| matches!(a.status, super::super::AgentStatus::Failed));

        if has_failures {
            let missing: Vec<_> = expected_agents
                .iter()
                .filter(|exp| !completed_names.contains(&exp.to_lowercase()))
                .map(|s| s.as_str())
                .collect();

            halt_spec_auto_with_error(
                widget,
                format!("Agent execution incomplete. Missing/failed: {:?}", missing),
            );
        }
    }
}

/// Check consensus and advance to next stage
fn check_consensus_and_advance_spec_auto(widget: &mut ChatWidget) {
    let Some(state) = widget.spec_auto_state.as_ref() else {
        return;
    };

    let Some(current_stage) = state.current_stage() else {
        halt_spec_auto_with_error(widget, "Invalid stage index".to_string());
        return;
    };

    let spec_id = state.spec_id.clone();

    // Show checking status
    widget.history_push(crate::history_cell::PlainHistoryCell::new(
        vec![ratatui::text::Line::from(format!(
            "Checking consensus for {}...",
            current_stage.display_name()
        ))],
        HistoryCellType::Notice,
    ));

    // Run consensus check
    match widget.run_spec_consensus(&spec_id, current_stage) {
        Ok((consensus_lines, consensus_ok)) => {
            widget.history_push(crate::history_cell::PlainHistoryCell::new(
                consensus_lines,
                if consensus_ok {
                    HistoryCellType::Notice
                } else {
                    HistoryCellType::Error
                },
            ));

            if consensus_ok {
                widget.history_push(crate::history_cell::PlainHistoryCell::new(
                    vec![ratatui::text::Line::from(format!(
                        "✓ {} consensus OK - advancing to next stage",
                        current_stage.display_name()
                    ))],
                    HistoryCellType::Notice,
                ));

                // Move to next stage
                if let Some(state) = widget.spec_auto_state.as_mut() {
                    state.phase = SpecAutoPhase::Guardrail;
                    state.current_index += 1;
                }

                // Trigger next stage
                advance_spec_auto(widget);
            } else {
                halt_spec_auto_with_error(
                    widget,
                    format!(
                        "Consensus failed for {} - see evidence above",
                        current_stage.display_name()
                    ),
                );
            }
        }
        Err(err) => {
            halt_spec_auto_with_error(
                widget,
                format!(
                    "Failed to check consensus for {}: {}",
                    current_stage.display_name(),
                    err
                ),
            );
        }
    }
}

// Additional handler functions will be added here in subsequent commits

use super::consensus::parse_consensus_stage;

/// Handle /spec-consensus command implementation
pub fn handle_spec_consensus_impl(widget: &mut ChatWidget, raw_args: String) {
    let trimmed = raw_args.trim();
    if trimmed.is_empty() {
        widget.history_push(crate::history_cell::new_error_event(
            "Usage: /spec-consensus <SPEC-ID> <stage>".to_string(),
        ));
        return;
    }

    let mut parts = trimmed.split_whitespace();
    let Some(spec_id) = parts.next() else {
        widget.history_push(crate::history_cell::new_error_event(
            "Usage: /spec-consensus <SPEC-ID> <stage>".to_string(),
        ));
        return;
    };

    let Some(stage_str) = parts.next() else {
        widget.history_push(crate::history_cell::new_error_event(
            "Usage: /spec-consensus <SPEC-ID> <stage>".to_string(),
        ));
        return;
    };

    let Some(stage) = parse_consensus_stage(stage_str) else {
        widget.history_push(crate::history_cell::new_error_event(format!(
            "Unknown stage '{stage_str}'. Expected plan, tasks, implement, validate, audit, or unlock.",
        )));
        return;
    };

    match widget.run_spec_consensus(spec_id, stage) {
        Ok((lines, ok)) => {
            let cell = crate::history_cell::PlainHistoryCell::new(
                lines,
                if ok {
                    HistoryCellType::Notice
                } else {
                    HistoryCellType::Error
                },
            );
            widget.history_push(cell);
        }
        Err(err) => {
            widget.history_push(crate::history_cell::new_error_event(err.to_string()));
        }
    }
}

// === Quality Gate Handlers (T85) ===

/// Handle quality gate agents completing
pub fn on_quality_gate_agents_complete(widget: &mut ChatWidget) {
    let Some(state) = widget.spec_auto_state.as_ref() else {
        return;
    };

    // Only proceed if in QualityGateExecuting phase
    let (checkpoint, results, gates) = match &state.phase {
        SpecAutoPhase::QualityGateExecuting { checkpoint, results, gates, .. } => {
            (*checkpoint, results.clone(), gates.clone())
        }
        _ => return,
    };

    let spec_id = state.spec_id.clone();
    let cwd = widget.config.cwd.clone();

    // Step 1: Parse agent results into QualityIssue objects
    let mut all_agent_issues = Vec::new();

    for (agent_id, agent_result) in &results {
        for gate in &gates {
            match super::quality::parse_quality_issue_from_agent(agent_id, agent_result, *gate) {
                Ok(issues) => all_agent_issues.push(issues),
                Err(err) => {
                    widget.history_push(crate::history_cell::new_error_event(format!(
                        "Failed to parse {} results from {}: {}",
                        gate.command_name(),
                        agent_id,
                        err
                    )));
                }
            }
        }
    }

    // Step 2: Merge issues from multiple agents by ID
    let merged_issues = super::quality::merge_agent_issues(all_agent_issues);

    widget.history_push(crate::history_cell::PlainHistoryCell::new(
        vec![
            ratatui::text::Line::from(format!("Quality Gate: {} - found {} issues from {} gates", checkpoint.name(), merged_issues.len(), gates.len())),
        ],
        crate::history_cell::HistoryCellType::Notice,
    ));

    if merged_issues.is_empty() {
        // No issues found - continue to next stage
        if let Some(state) = widget.spec_auto_state.as_mut() {
            state.completed_checkpoints.insert(checkpoint);
            state.phase = SpecAutoPhase::Guardrail;
        }
        advance_spec_auto(widget);
        return;
    }

    // Step 3: Classify issues by confidence
    let mut unanimous_issues = Vec::new();     // 3/3 agreement - auto-resolve
    let mut majority_issues = Vec::new();      // 2/3 agreement - needs GPT-5
    let mut no_consensus_issues = Vec::new();  // 0-1/3 agreement - escalate

    for issue in merged_issues {
        match issue.confidence {
            super::state::Confidence::High => unanimous_issues.push(issue),
            super::state::Confidence::Medium => majority_issues.push(issue),
            super::state::Confidence::Low => no_consensus_issues.push(issue),
        }
    }

    widget.history_push(crate::history_cell::PlainHistoryCell::new(
        vec![
            ratatui::text::Line::from(format!(
                "Quality Gate: {} - {} unanimous, {} need GPT-5 validation, {} escalated",
                checkpoint.name(),
                unanimous_issues.len(),
                majority_issues.len(),
                no_consensus_issues.len()
            )),
        ],
        crate::history_cell::HistoryCellType::Notice,
    ));

    // Step 4: Auto-resolve unanimous issues
    let mut auto_resolved_list = Vec::new();

    for issue in unanimous_issues {
        let (_, majority_answer, _) = super::quality::classify_issue_agreement(&issue.agent_answers);
        let answer = majority_answer.unwrap_or_else(|| "unknown".to_string());

        let spec_dir = cwd.join(format!("docs/{}", spec_id));
        match super::quality::apply_auto_resolution(&issue, &answer, &spec_dir) {
            Ok(outcome) => {
                widget.history_push(crate::history_cell::PlainHistoryCell::new(
                    vec![
                        ratatui::text::Line::from(format!("✅ Auto-resolved: {} → {}", issue.description, answer)),
                    ],
                    crate::history_cell::HistoryCellType::Notice,
                ));

                auto_resolved_list.push((issue.clone(), answer.clone()));

                // Track modified file
                if let Some(state) = widget.spec_auto_state.as_mut() {
                    let file_name = outcome.file_path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    if !state.quality_modifications.contains(&file_name) {
                        state.quality_modifications.push(file_name);
                    }
                }
            }
            Err(err) => {
                widget.history_push(crate::history_cell::new_error_event(format!(
                    "Failed to apply auto-resolution for '{}': {}",
                    issue.description, err
                )));
            }
        }
    }

    // Track auto-resolved in state
    if let Some(state) = widget.spec_auto_state.as_mut() {
        state.quality_auto_resolved.extend(auto_resolved_list.clone());
    }

    // Step 5: Handle majority issues - submit to GPT-5 for validation
    if !majority_issues.is_empty() {
        widget.history_push(crate::history_cell::PlainHistoryCell::new(
            vec![
                ratatui::text::Line::from(format!("Submitting {} majority answers to GPT-5 for validation...", majority_issues.len())),
            ],
            crate::history_cell::HistoryCellType::Notice,
        ));

        // Submit GPT-5 validation prompts via agent system
        submit_gpt5_validations(widget, &majority_issues, &spec_id, &cwd);

        // Transition to validating phase - wait for GPT-5 responses
        if let Some(state) = widget.spec_auto_state.as_mut() {
            let auto_resolved_issues: Vec<_> = auto_resolved_list.iter().map(|(issue, _)| issue.clone()).collect();

            state.phase = SpecAutoPhase::QualityGateValidating {
                checkpoint,
                auto_resolved: auto_resolved_issues,
                pending_validations: majority_issues.into_iter().map(|issue| {
                    let (_, majority, _) = super::quality::classify_issue_agreement(&issue.agent_answers);
                    (issue, majority.unwrap_or_default())
                }).collect(),
                completed_validations: std::collections::HashMap::new(),
            };
        }

        return; // Wait for GPT-5 responses
    }

    // No majority issues - handle no consensus escalations
    let mut all_escalations = Vec::new();
    for issue in no_consensus_issues {
        all_escalations.push((issue, None));
    }

    if !all_escalations.is_empty() {
        // Build escalated questions from all sources
        let (escalated_issues, escalated_questions): (Vec<_>, Vec<_>) = all_escalations
            .into_iter()
            .map(|(issue, validation_opt)| {
                let question = super::state::EscalatedQuestion {
                    id: issue.id.clone(),
                    gate_type: issue.gate_type,
                    question: issue.description.clone(),
                    context: issue.context.clone(),
                    agent_answers: issue.agent_answers.clone(),
                    gpt5_reasoning: validation_opt.as_ref().map(|v: &super::state::GPT5ValidationResult| v.reasoning.clone()),
                    magnitude: issue.magnitude,
                    suggested_options: validation_opt
                        .and_then(|v: super::state::GPT5ValidationResult| v.recommended_answer)
                        .into_iter()
                        .collect(),
                };
                (issue, question)
            })
            .unzip();

        widget.history_push(crate::history_cell::PlainHistoryCell::new(
            vec![
                ratatui::text::Line::from(format!(
                    "Quality Gate: {} - {} auto-resolved, {} need your input",
                    checkpoint.name(),
                    auto_resolved_list.len(),
                    escalated_questions.len()
                )),
            ],
            crate::history_cell::HistoryCellType::Notice,
        ));

        widget.bottom_pane.show_quality_gate_modal(checkpoint, escalated_questions.clone());

        if let Some(state) = widget.spec_auto_state.as_mut() {
            state.phase = SpecAutoPhase::QualityGateAwaitingHuman {
                checkpoint,
                escalated_issues,
                escalated_questions,
                answers: std::collections::HashMap::new(),
            };
        }
    } else {
        // All issues auto-resolved - continue pipeline
        widget.history_push(crate::history_cell::PlainHistoryCell::new(
            vec![
                ratatui::text::Line::from(format!("Quality Gate: {} complete - all issues auto-resolved", checkpoint.name())),
            ],
            crate::history_cell::HistoryCellType::Notice,
        ));

        if let Some(state) = widget.spec_auto_state.as_mut() {
            state.completed_checkpoints.insert(checkpoint);
            state.quality_checkpoint_outcomes.push((checkpoint, auto_resolved_list.len(), 0));
            state.phase = SpecAutoPhase::Guardrail;
        }

        advance_spec_auto(widget);
    }
}

/// Submit GPT-5 validation prompts via existing agent system
fn submit_gpt5_validations(
    widget: &mut ChatWidget,
    majority_issues: &[super::state::QualityIssue],
    spec_id: &str,
    cwd: &std::path::Path,
) {
    // Read SPEC content for validation context
    let spec_path = cwd.join(format!("docs/{}/spec.md", spec_id));
    let spec_content = std::fs::read_to_string(&spec_path).unwrap_or_default();

    let prd_path = cwd.join(format!("docs/{}/PRD.md", spec_id));
    let prd_content = std::fs::read_to_string(&prd_path).ok();

    // Submit ONE combined validation prompt (not separate per issue)
    // This way we get one agent response with all validations
    let mut validation_prompts = Vec::new();

    for (idx, issue) in majority_issues.iter().enumerate() {
        let (_, majority_answer, dissent) = super::quality::classify_issue_agreement(&issue.agent_answers);

        validation_prompts.push(format!(
            "Issue {}: {}\nMajority answer: {}\nDissent: {}\n",
            idx + 1,
            issue.description,
            majority_answer.as_deref().unwrap_or("unknown"),
            dissent.as_deref().unwrap_or("N/A")
        ));
    }

    let combined_prompt = format!(
        "You are validating {} quality gate issues for SPEC {}.\n\nSPEC Content:\n{}\n\n{}\n\nValidate each issue and output JSON array:\n[\n  {{\n    \"issue_index\": 1,\n    \"agrees_with_majority\": boolean,\n    \"reasoning\": string,\n    \"recommended_answer\": string|null,\n    \"confidence\": \"high\"|\"medium\"|\"low\"\n  }}\n]\n\nIssues:\n{}",
        majority_issues.len(),
        spec_id,
        spec_content,
        prd_content.as_deref().unwrap_or(""),
        validation_prompts.join("\n")
    );

    // Submit via existing agent system
    // Agent will store result in local-memory with stage="gpt5-validation"
    widget.submit_prompt_with_display(
        format!("[GPT-5 Validation] {}", spec_id),
        combined_prompt,
    );
}

/// Build GPT-5 validation prompt
fn build_gpt5_validation_prompt(
    question: &str,
    spec_content: &str,
    prd_content: Option<&str>,
    agent_answers: &std::collections::HashMap<String, String>,
    majority_answer: &str,
    dissent: Option<&str>,
) -> String {
    let prd_section = if let Some(prd) = prd_content {
        format!("PRD Content:\n{}\n\n", prd)
    } else {
        String::new()
    };

    // Extract individual agent answers
    let gemini = agent_answers.get("gemini").map(|s| s.as_str()).unwrap_or("N/A");
    let claude = agent_answers.get("claude").map(|s| s.as_str()).unwrap_or("N/A");
    let code = agent_answers.get("code").map(|s| s.as_str()).unwrap_or("N/A");

    let dissent_section = if let Some(d) = dissent {
        format!("Dissenting Reasoning: {}\n\n", d)
    } else {
        String::new()
    };

    format!(
        r#"You are validating a majority answer (2/3 agents agreed) for a quality gate issue.

SPEC Content:
{}

{}Question: {}

Agent Answers:
- Agent 1: {}
- Agent 2: {}
- Agent 3: {}

Majority Answer: {}
{}Your Task:
1. Analyze SPEC intent and requirements context
2. Evaluate if majority answer aligns with SPEC goals
3. Consider if dissenting reasoning reveals valid concern
4. Determine if majority should be applied or escalated

Output JSON:
{{
  "agrees_with_majority": boolean,
  "reasoning": string (detailed analysis),
  "recommended_answer": string|null (if you disagree),
  "confidence": "high"|"medium"|"low",
  "critical_flag": boolean (true if issue is architectural/critical)
}}

If the dissenting view makes a valid point about SPEC intent, reject the majority."#,
        spec_content,
        prd_section,
        question,
        gemini,
        claude,
        code,
        majority_answer,
        dissent_section
    )
}

/// Handle quality gate answers submitted by user
pub fn on_quality_gate_answers(
    widget: &mut ChatWidget,
    checkpoint: super::state::QualityCheckpoint,
    answers: std::collections::HashMap<String, String>,
) {
    let Some(state) = widget.spec_auto_state.as_ref() else {
        return;
    };

    let spec_id = state.spec_id.clone();
    let cwd = widget.config.cwd.clone();

    // Get escalated issues from state
    let escalated_issues = match &state.phase {
        SpecAutoPhase::QualityGateAwaitingHuman { escalated_issues, .. } => escalated_issues.clone(),
        _ => {
            widget.history_push(crate::history_cell::new_error_event(
                "Not in QualityGateAwaitingHuman phase".to_string(),
            ));
            return;
        }
    };

    widget.history_push(crate::history_cell::PlainHistoryCell::new(
        vec![
            ratatui::text::Line::from(format!(
                "Quality Gate: {} - applying {} human answers",
                checkpoint.name(),
                answers.len()
            )),
        ],
        crate::history_cell::HistoryCellType::Notice,
    ));

    // Apply each answer to its corresponding issue
    let mut applied_answers = Vec::new();

    for issue in &escalated_issues {
        if let Some(answer) = answers.get(&issue.id) {
            let spec_dir = cwd.join(format!("docs/{}", spec_id));

            match super::quality::apply_auto_resolution(issue, answer, &spec_dir) {
                Ok(outcome) => {
                    widget.history_push(crate::history_cell::PlainHistoryCell::new(
                        vec![
                            ratatui::text::Line::from(format!("✅ Applied: {} → {}", issue.description, answer)),
                        ],
                        crate::history_cell::HistoryCellType::Notice,
                    ));

                    applied_answers.push((issue.clone(), answer.clone()));

                    // Track modified file
                    if let Some(state) = widget.spec_auto_state.as_mut() {
                        let file_name = outcome.file_path.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown")
                            .to_string();
                        if !state.quality_modifications.contains(&file_name) {
                            state.quality_modifications.push(file_name);
                        }
                    }
                }
                Err(err) => {
                    widget.history_push(crate::history_cell::new_error_event(format!(
                        "Failed to apply answer for '{}': {}",
                        issue.description, err
                    )));
                }
            }
        }
    }

    // Track answered questions in state
    if let Some(state) = widget.spec_auto_state.as_mut() {
        state.quality_escalated.extend(applied_answers);
    }

    // Mark checkpoint complete and transition to next stage
    if let Some(state) = widget.spec_auto_state.as_mut() {
        state.completed_checkpoints.insert(checkpoint);
        state.phase = SpecAutoPhase::Guardrail;
    }

    // Continue pipeline
    advance_spec_auto(widget);
}

/// Handle GPT-5 validations completing
pub fn on_gpt5_validations_complete(widget: &mut ChatWidget) {
    let Some(state) = widget.spec_auto_state.as_ref() else {
        return;
    };

    let (checkpoint, auto_resolved, pending_validations) = match &state.phase {
        SpecAutoPhase::QualityGateValidating {
            checkpoint,
            auto_resolved,
            pending_validations,
            ..
        } => (
            *checkpoint,
            auto_resolved.clone(),
            pending_validations.clone(),
        ),
        _ => return,
    };

    let spec_id = state.spec_id.clone();
    let cwd = widget.config.cwd.clone();

    // Query local-memory for GPT-5 validation result
    // Agent stores result with stage="gpt5-validation"
    let validation_results = match crate::local_memory_util::search_by_stage(
        &spec_id,
        "gpt5-validation",
        10,
    ) {
        Ok(results) if !results.is_empty() => results,
        _ => {
            // GPT-5 hasn't completed yet or failed
            return;
        }
    };

    // Parse the first result (should be the GPT-5 validation array)
    let validation_json: serde_json::Value = match serde_json::from_str(&validation_results[0].memory.content) {
        Ok(json) => json,
        Err(err) => {
            widget.history_push(crate::history_cell::new_error_event(format!(
                "Failed to parse GPT-5 validation JSON: {}",
                err
            )));
            return;
        }
    };

    // Expect array of validations
    let validation_array = match validation_json.as_array() {
        Some(arr) => arr,
        None => {
            widget.history_push(crate::history_cell::new_error_event(
                "GPT-5 validation response was not an array".to_string(),
            ));
            return;
        }
    };

    widget.history_push(crate::history_cell::PlainHistoryCell::new(
        vec![
            ratatui::text::Line::from(format!("GPT-5 validation complete: {} results processed", validation_array.len())),
        ],
        crate::history_cell::HistoryCellType::Notice,
    ));

    // Process validation results
    let mut validated_auto_resolved = Vec::new();
    let mut validation_rejected = Vec::new();

    for validation_item in validation_array {
        let issue_index = validation_item["issue_index"].as_u64().unwrap_or(0) as usize;

        // Match to pending validation (issue_index is 1-based in prompt)
        if issue_index == 0 || issue_index > pending_validations.len() {
            continue;
        }

        let (issue, majority_answer) = &pending_validations[issue_index - 1];
        let agrees = validation_item["agrees_with_majority"].as_bool().unwrap_or(false);

        let validation = super::state::GPT5ValidationResult {
            agrees_with_majority: agrees,
            reasoning: validation_item["reasoning"]
                .as_str()
                .unwrap_or("No reasoning")
                .to_string(),
            recommended_answer: validation_item["recommended_answer"]
                .as_str()
                .map(String::from),
            confidence: match validation_item["confidence"].as_str() {
                Some("high") => super::state::Confidence::High,
                Some("medium") => super::state::Confidence::Medium,
                _ => super::state::Confidence::Low,
            },
        };

        if agrees {
            // GPT-5 validated - auto-apply
            let spec_dir = cwd.join(format!("docs/{}", spec_id));
            match super::quality::apply_auto_resolution(issue, majority_answer, &spec_dir) {
                Ok(outcome) => {
                    widget.history_push(crate::history_cell::PlainHistoryCell::new(
                        vec![
                            ratatui::text::Line::from(format!("✅ GPT-5 validated: {} → {}", issue.description, majority_answer)),
                        ],
                        crate::history_cell::HistoryCellType::Notice,
                    ));

                    validated_auto_resolved.push((issue.clone(), majority_answer.clone()));

                    if let Some(state) = widget.spec_auto_state.as_mut() {
                        let file_name = outcome.file_path.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown")
                            .to_string();
                        if !state.quality_modifications.contains(&file_name) {
                            state.quality_modifications.push(file_name);
                        }
                    }
                }
                Err(err) => {
                    widget.history_push(crate::history_cell::new_error_event(format!(
                        "Failed to apply GPT-5 validated resolution: {}", err
                    )));
                }
            }
        } else {
            // GPT-5 rejected - escalate
            validation_rejected.push((issue.clone(), validation));
        }
    }

    // Track validated resolutions
    if let Some(state) = widget.spec_auto_state.as_mut() {
        state.quality_auto_resolved.extend(validated_auto_resolved.clone());
    }

    if validation_rejected.is_empty() {
        // All validations accepted - continue pipeline
        widget.history_push(crate::history_cell::PlainHistoryCell::new(
            vec![
                ratatui::text::Line::from(format!("Quality Gate: {} complete - all validations accepted", checkpoint.name())),
            ],
            crate::history_cell::HistoryCellType::Notice,
        ));

        if let Some(state) = widget.spec_auto_state.as_mut() {
            state.completed_checkpoints.insert(checkpoint);
            state.quality_checkpoint_outcomes.push((
                checkpoint,
                auto_resolved.len() + validated_auto_resolved.len(),
                0,
            ));
            state.phase = SpecAutoPhase::Guardrail;
        }

        advance_spec_auto(widget);
    } else {
        // Some validations rejected - escalate those issues
        let escalated_questions: Vec<_> = validation_rejected.iter().map(|(issue, validation)| {
            super::state::EscalatedQuestion {
                id: issue.id.clone(),
                gate_type: issue.gate_type,
                question: issue.description.clone(),
                context: issue.context.clone(),
                agent_answers: issue.agent_answers.clone(),
                gpt5_reasoning: Some(validation.reasoning.clone()),
                magnitude: issue.magnitude,
                suggested_options: validation.recommended_answer.clone().into_iter().collect(),
            }
        }).collect();

        widget.history_push(crate::history_cell::PlainHistoryCell::new(
            vec![
                ratatui::text::Line::from(format!(
                    "Quality Gate: {} - {} GPT-5 validated, {} rejected (escalating)",
                    checkpoint.name(),
                    validated_auto_resolved.len(),
                    validation_rejected.len()
                )),
            ],
            crate::history_cell::HistoryCellType::Notice,
        ));

        widget.bottom_pane.show_quality_gate_modal(checkpoint, escalated_questions.clone());

        if let Some(state) = widget.spec_auto_state.as_mut() {
            state.phase = SpecAutoPhase::QualityGateAwaitingHuman {
                checkpoint,
                escalated_issues: validation_rejected.into_iter().map(|(issue, _)| issue).collect(),
                escalated_questions,
                answers: std::collections::HashMap::new(),
            };
        }
    }
}

/// Handle quality gate cancelled by user
pub fn on_quality_gate_cancelled(
    widget: &mut ChatWidget,
    checkpoint: super::state::QualityCheckpoint,
) {
    halt_spec_auto_with_error(
        widget,
        format!("Quality gate {} cancelled by user", checkpoint.name()),
    );
}

/// Determine which quality checkpoint should run before the given stage
fn determine_quality_checkpoint(
    stage: SpecStage,
    completed: &std::collections::HashSet<super::state::QualityCheckpoint>,
) -> Option<super::state::QualityCheckpoint> {
    use super::state::QualityCheckpoint;

    match stage {
        SpecStage::Plan => {
            if !completed.contains(&QualityCheckpoint::PrePlanning) {
                Some(QualityCheckpoint::PrePlanning)
            } else {
                None
            }
        }
        SpecStage::Tasks => {
            if !completed.contains(&QualityCheckpoint::PostPlan) {
                Some(QualityCheckpoint::PostPlan)
            } else {
                None
            }
        }
        SpecStage::Implement => {
            if !completed.contains(&QualityCheckpoint::PostTasks) {
                Some(QualityCheckpoint::PostTasks)
            } else {
                None
            }
        }
        _ => None,  // No checkpoints for Validate, Audit, Unlock
    }
}

/// Execute quality checkpoint by starting quality gate agents
fn execute_quality_checkpoint(
    widget: &mut ChatWidget,
    checkpoint: super::state::QualityCheckpoint,
) {
    let Some(state) = widget.spec_auto_state.as_ref() else {
        return;
    };

    let spec_id = state.spec_id.clone();

    widget.history_push(crate::history_cell::PlainHistoryCell::new(
        vec![
            ratatui::text::Line::from(format!("Starting Quality Checkpoint: {}", checkpoint.name())),
        ],
        crate::history_cell::HistoryCellType::Notice,
    ));

    // Build prompts for each gate in the checkpoint
    let gates = checkpoint.gates();

    // Submit prompts to agents (gemini, claude, code)
    for gate in gates {
        let prompt = build_quality_gate_prompt(&spec_id, *gate, checkpoint);

        // Format as subagent command and submit
        let _formatted = codex_core::slash_commands::format_subagent_command(
            gate.command_name(),
            &spec_id,
            None,
            None,
        );

        // Override with quality gate prompt
        widget.submit_prompt_with_display(
            format!("Quality Gate: {} - {}", checkpoint.name(), gate.command_name()),
            prompt,
        );
    }

    // Transition to quality gate executing phase
    if let Some(state) = widget.spec_auto_state.as_mut() {
        state.phase = SpecAutoPhase::QualityGateExecuting {
            checkpoint,
            gates: gates.to_vec(),
            active_gates: gates.iter().copied().collect(),
            expected_agents: vec!["gemini".to_string(), "claude".to_string(), "code".to_string()],
            completed_agents: std::collections::HashSet::new(),
            results: std::collections::HashMap::new(),
        };
    }
}

/// Build quality gate prompt for a specific gate
fn build_quality_gate_prompt(
    spec_id: &str,
    gate: super::state::QualityGateType,
    checkpoint: super::state::QualityCheckpoint,
) -> String {
    // Load prompt template from prompts.json
    // For now, simplified version

    let gate_name = match gate {
        super::state::QualityGateType::Clarify => "quality-gate-clarify",
        super::state::QualityGateType::Checklist => "quality-gate-checklist",
        super::state::QualityGateType::Analyze => "quality-gate-analyze",
    };

    format!(
        "Quality Gate: {} at checkpoint {}\n\nAnalyze SPEC {} for issues.\n\nSee prompts.json[\"{}\"] for full instructions.",
        gate.command_name(),
        checkpoint.name(),
        spec_id,
        gate_name
    )
}

/// Finalize quality gates at pipeline completion
fn finalize_quality_gates(widget: &mut ChatWidget) {
    let Some(state) = widget.spec_auto_state.as_ref() else {
        return;
    };

    let spec_id = state.spec_id.clone();
    let cwd = widget.config.cwd.clone();
    let auto_resolved = state.quality_auto_resolved.clone();
    let escalated = state.quality_escalated.clone();
    let modified_files = state.quality_modifications.clone();
    let checkpoint_outcomes = state.quality_checkpoint_outcomes.clone();

    // Step 1: Persist telemetry for each checkpoint
    let repo = super::evidence::FilesystemEvidence::new(cwd.clone(), None);

    for (checkpoint, _auto_count, _esc_count) in &checkpoint_outcomes {
        // Build telemetry JSON
        let telemetry = super::quality::build_quality_checkpoint_telemetry(
            &spec_id,
            *checkpoint,
            &auto_resolved,
            &escalated,
        );

        match repo.write_quality_checkpoint_telemetry(&spec_id, *checkpoint, &telemetry) {
            Ok(path) => {
                widget.history_push(crate::history_cell::PlainHistoryCell::new(
                    vec![
                        ratatui::text::Line::from(format!("📊 Telemetry: {}", path.display())),
                    ],
                    HistoryCellType::Notice,
                ));
            }
            Err(err) => {
                widget.history_push(crate::history_cell::new_error_event(format!(
                    "Failed to write telemetry for {}: {}",
                    checkpoint.name(),
                    err
                )));
            }
        }
    }

    // Step 2: Create git commit if there are modifications
    if !modified_files.is_empty() {
        let commit_msg = super::quality::build_quality_gate_commit_message(
            &spec_id,
            &checkpoint_outcomes,
            &modified_files,
        );

        // Execute git commit
        let git_result = std::process::Command::new("git")
            .current_dir(&cwd)
            .args(&["add", "docs/"])
            .output();

        if let Ok(add_output) = git_result {
            if add_output.status.success() {
                let commit_result = std::process::Command::new("git")
                    .current_dir(&cwd)
                    .args(&["commit", "-m", &commit_msg])
                    .output();

                match commit_result {
                    Ok(output) if output.status.success() => {
                        widget.history_push(crate::history_cell::PlainHistoryCell::new(
                            vec![
                                ratatui::text::Line::from("✅ Quality gate changes committed"),
                            ],
                            HistoryCellType::Notice,
                        ));
                    }
                    Ok(output) => {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        widget.history_push(crate::history_cell::new_error_event(format!(
                            "Git commit failed: {}",
                            stderr
                        )));
                    }
                    Err(err) => {
                        widget.history_push(crate::history_cell::new_error_event(format!(
                            "Failed to run git commit: {}",
                            err
                        )));
                    }
                }
            }
        }
    }

    // Step 3: Show review summary
    let summary_lines = super::quality::build_quality_gate_summary(
        &auto_resolved,
        &escalated,
        &modified_files,
    );

    widget.history_push(crate::history_cell::PlainHistoryCell::new(
        summary_lines,
        HistoryCellType::Notice,
    ));
}
