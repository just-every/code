//! Quality Gate Handlers (T85)
//!
//! Autonomous quality assurance system integrated into /speckit.auto pipeline.
//! Implements 3-checkpoint validation with confidence-based auto-resolution.
//!
//! MAINT-2: Extracted from handler.rs (925 LOC) for maintainability

use super::super::ChatWidget;
use super::evidence::{EvidenceRepository, FilesystemEvidence};
use super::state::SpecAutoPhase;
use crate::history_cell::HistoryCellType;
use crate::spec_prompts::SpecStage;

/// Handle quality gate agents completing
pub fn on_quality_gate_agents_complete(widget: &mut ChatWidget) {
    // RECURSION GUARD: Check processing flag FIRST, before any history_push
    let (checkpoint, should_process) = {
        let Some(state) = widget.spec_auto_state.as_ref() else {
            return; // No state - silent return
        };

        match &state.phase {
            SpecAutoPhase::QualityGateExecuting { checkpoint, .. } => {
                // Check all guard conditions WITHOUT any history_push calls
                let already_completed = state.completed_checkpoints.contains(checkpoint);
                let already_processing = state.quality_gate_processing == Some(*checkpoint);

                if already_completed || already_processing {
                    return; // Silent return - no recursion
                }

                (*checkpoint, true)
            }
            _ => return, // Wrong phase - silent return
        }
    };

    // Set processing flag IMMEDIATELY (before ANY output)
    if let Some(state) = widget.spec_auto_state.as_mut() {
        state.quality_gate_processing = Some(checkpoint);
    }

    // NOW safe to do history_push - flag is set, won't re-trigger
    widget.history_push(crate::history_cell::PlainHistoryCell::new(
        vec![
            ratatui::text::Line::from("DEBUG: on_quality_gate_agents_complete() PROCESSING"),
        ],
        crate::history_cell::HistoryCellType::Notice,
    ));

    // Extract data after flag is set
    let (spec_id, cwd, gates) = {
        let state = widget.spec_auto_state.as_ref().unwrap();
        let gates = match &state.phase {
            SpecAutoPhase::QualityGateExecuting { gates, .. } => gates.clone(),
            _ => return,
        };
        (state.spec_id.clone(), widget.config.cwd.clone(), gates)
    };

    widget.history_push(crate::history_cell::PlainHistoryCell::new(
        vec![
            ratatui::text::Line::from(format!("Quality Gate: {} - retrieving agent responses from local-memory...", checkpoint.name())),
        ],
        crate::history_cell::HistoryCellType::Notice,
    ));

    // Step 1: Retrieve agent results from local-memory
    let agent_results: Vec<(String, serde_json::Value)> = {
        use serde_json::json;

        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                let manager_arc = handle.block_on(async {
                    widget.mcp_manager.lock().await.as_ref().cloned()
                });

                match manager_arc {
                    Some(manager) => {
                        let args = json!({
                            "query": format!("{} quality-gate", spec_id),
                            "limit": 10,
                            "tags": [format!("quality-gate"), spec_id.clone()],
                            "search_type": "hybrid"
                        });

                        let mcp_result = handle.block_on(async {
                            manager.call_tool(
                                "local-memory",
                                "search",
                                Some(args),
                                Some(std::time::Duration::from_secs(10))
                            ).await
                        });

                        match mcp_result.ok().and_then(|r| crate::spec_prompts::parse_mcp_results_to_local_memory(&r).ok()) {
                            Some(results) if !results.is_empty() => {
                                results.into_iter().filter_map(|mem| {
                                    // Parse JSON content
                                    let json_value: serde_json::Value = serde_json::from_str(&mem.memory.content).ok()?;

                                    // Extract agent name from JSON (all quality gate responses have "agent" field)
                                    let agent = json_value.get("agent")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("unknown")
                                        .to_string();

                                    Some((agent, json_value))
                                }).collect()
                            }
                            _ => {
                                widget.history_push(crate::history_cell::new_error_event(
                                    "No quality gate results found in local-memory".to_string()
                                ));
                                // Clear processing flag on error
                                if let Some(state) = widget.spec_auto_state.as_mut() {
                                    state.quality_gate_processing = None;
                                }
                                return;
                            }
                        }
                    }
                    None => {
                        widget.history_push(crate::history_cell::new_error_event(
                            "MCP manager not available".to_string()
                        ));
                        // Clear processing flag on error
                        if let Some(state) = widget.spec_auto_state.as_mut() {
                            state.quality_gate_processing = None;
                        }
                        return;
                    }
                }
            }
            Err(_) => {
                widget.history_push(crate::history_cell::new_error_event(
                    "No tokio runtime available".to_string()
                ));
                // Clear processing flag on error
                if let Some(state) = widget.spec_auto_state.as_mut() {
                    state.quality_gate_processing = None;
                }
                return;
            }
        }
    };

    if agent_results.is_empty() {
        widget.history_push(crate::history_cell::new_error_event(
            "Failed to retrieve quality gate results from local-memory".to_string()
        ));
        // Clear processing flag on error
        if let Some(state) = widget.spec_auto_state.as_mut() {
            state.quality_gate_processing = None;
        }
        return;
    }

    // Step 2: Parse agent results into QualityIssue objects
    let mut all_agent_issues = Vec::new();

    for (agent_id, agent_result) in &agent_results {
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
            state.quality_gate_processing = None; // Clear processing flag
            state.phase = SpecAutoPhase::Guardrail;
        }
        super::handler::advance_spec_auto(widget);
        return;
    }

    // Step 3: Classify issues using should_auto_resolve() decision matrix
    // Checks confidence + magnitude + resolvability (quality.rs:75-92)
    let mut auto_resolvable = Vec::new();      // should_auto_resolve() = true
    let mut needs_validation = Vec::new();      // Medium confidence, not auto-resolvable
    let mut escalate_to_human = Vec::new();    // Low confidence or manual-only

    for issue in merged_issues {
        if super::quality::should_auto_resolve(&issue) {
            auto_resolvable.push(issue);
        } else if matches!(issue.confidence, super::state::Confidence::Medium) {
            // Medium confidence but didn't pass auto-resolve → needs GPT-5
            needs_validation.push(issue);
        } else {
            // Low confidence or requires human judgment
            escalate_to_human.push(issue);
        }
    }

    widget.history_push(crate::history_cell::PlainHistoryCell::new(
        vec![
            ratatui::text::Line::from(format!(
                "Quality Gate: {} - {} auto-resolvable, {} need GPT-5 validation, {} escalated",
                checkpoint.name(),
                auto_resolvable.len(),
                needs_validation.len(),
                escalate_to_human.len()
            )),
        ],
        crate::history_cell::HistoryCellType::Notice,
    ));

    // Step 4: Auto-resolve auto-resolvable issues
    let mut auto_resolved_list = Vec::new();

    for issue in auto_resolvable {
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

    // Step 5: Handle medium-confidence issues - submit to GPT-5 for validation
    if !needs_validation.is_empty() {
        widget.history_push(crate::history_cell::PlainHistoryCell::new(
            vec![
                ratatui::text::Line::from(format!("Submitting {} medium-confidence issues to GPT-5 for validation...", needs_validation.len())),
            ],
            crate::history_cell::HistoryCellType::Notice,
        ));

        // Submit GPT-5 validation prompts via agent system
        submit_gpt5_validations(widget, &needs_validation, &spec_id, &cwd);

        // Transition to validating phase - wait for GPT-5 responses
        if let Some(state) = widget.spec_auto_state.as_mut() {
            let auto_resolved_issues: Vec<_> = auto_resolved_list.iter().map(|(issue, _)| issue.clone()).collect();

            state.quality_gate_processing = None; // Clear processing flag when transitioning
            state.phase = SpecAutoPhase::QualityGateValidating {
                checkpoint,
                auto_resolved: auto_resolved_issues,
                pending_validations: needs_validation.into_iter().map(|issue| {
                    let (_, majority, _) = super::quality::classify_issue_agreement(&issue.agent_answers);
                    (issue, majority.unwrap_or_default())
                }).collect(),
                completed_validations: std::collections::HashMap::new(),
            };
        }

        return; // Wait for GPT-5 responses
    }

    // No validation issues - handle low-confidence/manual escalations
    let mut all_escalations = Vec::new();
    for issue in escalate_to_human {
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
            state.quality_gate_processing = None; // Clear processing flag when transitioning to modal
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
            state.quality_gate_processing = None; // Clear processing flag
            state.quality_checkpoint_outcomes.push((checkpoint, auto_resolved_list.len(), 0));
            state.phase = SpecAutoPhase::Guardrail;
        }

        super::handler::advance_spec_auto(widget);
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
    super::handler::advance_spec_auto(widget);
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

    // FORK-SPECIFIC (just-every/code): Query local-memory for GPT-5 validation (native MCP, ARCH-004)
    // Agent stores result with stage="gpt5-validation"
    let validation_results: Vec<crate::local_memory_util::LocalMemorySearchResult> = {
        use serde_json::json;

        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                // Await async lock, then clone Arc
                let manager_arc = handle.block_on(async {
                    widget.mcp_manager.lock().await.as_ref().cloned()
                });

                match manager_arc {
                    Some(manager) => {
                        let args = json!({
                            "query": format!("{} gpt5-validation", spec_id),
                            "limit": 10,
                            "tags": [format!("spec:{}", spec_id), "stage:gpt5-validation"],
                            "search_type": "hybrid"
                        });

                        let mcp_result = handle.block_on(async {
                            manager.call_tool(
                                "local-memory",
                                "search",
                                Some(args),
                                Some(std::time::Duration::from_secs(10))
                            ).await
                        });

                        // Parse MCP results or return early
                        match mcp_result.ok().and_then(|r| crate::spec_prompts::parse_mcp_results_to_local_memory(&r).ok()) {
                            Some(results) if !results.is_empty() => results,
                            _ => return,  // GPT-5 not complete or parse failed
                        }
                    }
                    None => return,  // MCP not initialized
                }
            }
            Err(_) => return,  // No tokio runtime
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
            state.quality_gate_processing = None; // Clear processing flag
            state.quality_checkpoint_outcomes.push((
                checkpoint,
                auto_resolved.len() + validated_auto_resolved.len(),
                0,
            ));
            state.phase = SpecAutoPhase::Guardrail;
        }

        super::handler::advance_spec_auto(widget);
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
            state.quality_gate_processing = None; // Clear processing flag when transitioning to modal
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
    super::handler::halt_spec_auto_with_error(
        widget,
        format!("Quality gate {} cancelled by user", checkpoint.name()),
    );
}

/// Determine which quality checkpoint should run before the given stage
pub(super) fn determine_quality_checkpoint(
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
pub(super) fn execute_quality_checkpoint(
    widget: &mut ChatWidget,
    checkpoint: super::state::QualityCheckpoint,
) {
    let Some(state) = widget.spec_auto_state.as_ref() else {
        return;
    };

    let spec_id = state.spec_id.clone();
    let cwd = widget.config.cwd.clone();

    widget.history_push(crate::history_cell::PlainHistoryCell::new(
        vec![
            ratatui::text::Line::from(format!("Starting Quality Checkpoint: {}", checkpoint.name())),
        ],
        crate::history_cell::HistoryCellType::Notice,
    ));

    // Build prompts for each gate in the checkpoint
    let gates = checkpoint.gates();

    // Build orchestrator prompt to spawn quality gate agents
    let gate_names: Vec<String> = gates.iter().map(|g| g.command_name().to_string()).collect();

    let orchestrator_prompt = format!(
        r#"Execute Quality Checkpoint: {} for SPEC {}

CRITICAL: Spawn 3 SEPARATE agent_run calls (one per agent with role-specific prompt).

STEP 1: Read docs/spec-kit/prompts.json
Extract prompts for gates: {}

STEP 2: For EACH gate, spawn 3 SEPARATE agents:

For Gemini:
  agent_run(
    models: ["gemini"],
    read_only: true,
    task: <gemini's role-specific prompt from prompts.json>
  )

For Claude:
  agent_run(
    models: ["claude"],
    read_only: true,
    task: <claude's role-specific prompt from prompts.json>
  )

For Code:
  agent_run(
    models: ["code"],
    read_only: true,
    task: <code's role-specific prompt from prompts.json>
  )

STEP 3: Collect all 3 agent_ids, use agent_wait for each

STEP 4: For each completed agent:
  - Read .code/agents/{{agent_id}}/result.txt
  - Store using mcp__local-memory__store_memory:
    content: <JSON from result.txt>,
    tags: ["quality-gate", "{}", "agent:<gemini|claude|code>"],
    domain: "spec-kit",
    importance: 8

STEP 5: Report "Quality gate complete - 3/3 agents stored in local-memory"

CRITICAL: 3 SEPARATE agent_run calls, NOT one batch. Each agent gets its own prompt.

Gates: {}
"#,
        checkpoint.name(),
        spec_id,
        gate_names.join(", "),
        spec_id,
        gate_names.join(", ")
    );

    // Submit orchestrator prompt (like regular stages do)
    let user_msg = super::super::message::UserMessage {
        display_text: format!("Quality Checkpoint: {}", checkpoint.name()),
        ordered_items: vec![codex_core::protocol::InputItem::Text {
            text: orchestrator_prompt
        }],
    };

    widget.submit_user_message(user_msg);

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
    // FORK-SPECIFIC: Add JSON schema and examples (just-every/code)

    let gate_name = match gate {
        super::state::QualityGateType::Clarify => "quality-gate-clarify",
        super::state::QualityGateType::Checklist => "quality-gate-checklist",
        super::state::QualityGateType::Analyze => "quality-gate-analyze",
    };

    // Add schema and examples
    let schema_json = super::schemas::quality_gate_response_schema();
    let schema_str = serde_json::to_string_pretty(&schema_json["schema"])
        .unwrap_or_else(|_| "{}".to_string());

    // Few-shot example
    let example = r#"{
  "issues": [
    {
      "id": "Q1",
      "question": "Authentication method not specified in requirements",
      "answer": "Add OAuth2 authentication section specifying provider and scopes",
      "confidence": "high",
      "magnitude": "important",
      "resolvability": "auto-fix",
      "context": "Security requirements section is missing auth details",
      "suggested_fix": "Add OAuth2 section with provider and scopes",
      "reasoning": "Authentication is critical for security and must be specified before implementation"
    }
  ]
}"#;

    format!(
        r#"Quality Gate: {} at checkpoint {}

Analyze SPEC {} for issues.

CRITICAL: Return ONLY valid JSON matching this exact schema:
{}

Example correct output:
{}

Instructions:
- Find all ambiguities, inconsistencies, or missing requirements
- Each issue needs: id, question, answer, confidence, magnitude, resolvability
- confidence: "high" (certain), "medium" (likely), "low" (unsure)
- magnitude: "critical" (blocks progress), "important" (significant), "minor" (nice-to-have)
- resolvability: "auto-fix" (safe to apply), "suggest-fix" (needs review), "need-human" (judgment required)
- Store this analysis in local-memory using remember command
- If no issues found, return: {{"issues": []}}

See prompts.json["{}"] for detailed context."#,
        gate.command_name(),
        checkpoint.name(),
        spec_id,
        schema_str,
        example,
        gate_name
    )
}

/// Finalize quality gates at pipeline completion
pub(super) fn finalize_quality_gates(widget: &mut ChatWidget) {
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
