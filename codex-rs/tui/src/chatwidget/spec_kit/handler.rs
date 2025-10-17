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
                    // Quality gate phases - not implemented yet, continue for now
                    SpecAutoPhase::QualityGateExecuting { .. } => {
                        return; // TODO: Implement quality gate execution
                    }
                    SpecAutoPhase::QualityGateProcessing { .. } => {
                        return; // TODO: Implement result processing
                    }
                    SpecAutoPhase::QualityGateAwaitingHuman { .. } => {
                        return; // TODO: Implement human escalation UI
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

    // Update completed agents in state
    let is_quality_gate = if let Some(state) = widget.spec_auto_state.as_mut() {
        match &mut state.phase {
            SpecAutoPhase::ExecutingAgents {
                completed_agents, ..
            } => {
                *completed_agents = completed_names.clone();
                false
            }
            SpecAutoPhase::QualityGateExecuting {
                completed_agents, ..
            } => {
                *completed_agents = completed_names.clone();
                true
            }
            _ => false,
        }
    } else {
        false
    };

    // Check if all expected agents completed
    let all_expected_complete = expected_agents
        .iter()
        .all(|expected| completed_names.contains(&expected.to_lowercase()));

    if all_expected_complete {
        if is_quality_gate {
            // Quality gate agents done - process results
            on_quality_gate_agents_complete(widget);
        } else {
            // Regular stage agents done - trigger consensus check
            if let Some(state) = widget.spec_auto_state.as_mut() {
                state.phase = SpecAutoPhase::CheckingConsensus;
            }
            check_consensus_and_advance_spec_auto(widget);
        }
    } else {
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
            widget.history_push(crate::history_cell::new_error_event(err));
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

    // Step 3: Classify and resolve
    let mut auto_resolved_list = Vec::new();
    let mut escalated_list = Vec::new();

    for issue in merged_issues {
        let resolution = super::quality::resolve_quality_issue(&issue);

        match resolution {
            super::state::Resolution::AutoApply { answer, .. } => {
                // Auto-resolve: apply to file
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

            super::state::Resolution::Escalate { gpt5_reasoning, recommended, .. } => {
                // Build escalated question
                let escalated_q = super::state::EscalatedQuestion {
                    id: issue.id.clone(),
                    gate_type: issue.gate_type,
                    question: issue.description.clone(),
                    context: issue.context.clone(),
                    agent_answers: issue.agent_answers.clone(),
                    gpt5_reasoning,
                    magnitude: issue.magnitude,
                    suggested_options: if let Some(rec) = recommended {
                        vec![rec]
                    } else {
                        Vec::new()
                    },
                };

                escalated_list.push((issue, escalated_q));
            }
        }
    }

    // Track in state
    if let Some(state) = widget.spec_auto_state.as_mut() {
        state.quality_auto_resolved.extend(auto_resolved_list.clone());
        state.quality_checkpoint_outcomes.push((
            checkpoint,
            auto_resolved_list.len(),
            escalated_list.len(),
        ));
    }

    if escalated_list.is_empty() {
        // No escalations - continue pipeline
        widget.history_push(crate::history_cell::PlainHistoryCell::new(
            vec![
                ratatui::text::Line::from(format!("Quality Gate: {} complete - all issues auto-resolved", checkpoint.name())),
            ],
            crate::history_cell::HistoryCellType::Notice,
        ));

        if let Some(state) = widget.spec_auto_state.as_mut() {
            state.completed_checkpoints.insert(checkpoint);
            state.phase = SpecAutoPhase::Guardrail;
        }

        advance_spec_auto(widget);
    } else {
        // Has escalations - show modal
        let escalated_questions: Vec<_> = escalated_list.iter().map(|(_, q)| q.clone()).collect();

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

        // Show modal
        widget.bottom_pane.show_quality_gate_modal(checkpoint, escalated_questions.clone());

        // Transition to awaiting human phase
        if let Some(state) = widget.spec_auto_state.as_mut() {
            let (issues, questions): (Vec<_>, Vec<_>) = escalated_list.into_iter().unzip();
            state.phase = SpecAutoPhase::QualityGateAwaitingHuman {
                checkpoint,
                escalated_issues: issues,
                escalated_questions: questions,
                answers: std::collections::HashMap::new(),
            };
        }
    }
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
