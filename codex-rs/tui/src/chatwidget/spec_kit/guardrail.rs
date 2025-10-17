//! Guardrail validation infrastructure for spec-kit automation
//!
//! This module handles guardrail script validation, telemetry parsing,
//! schema compliance checking, and outcome evaluation.

use super::super::ChatWidget;
use super::super::agent_install::wrap_command;
use super::error::{Result, SpecKitError};
use super::state::{
    GuardrailEvaluation, GuardrailOutcome, expected_guardrail_command, require_object,
    require_string_field, validate_guardrail_evidence,
};
use crate::app_event::BackgroundPlacement;
use crate::slash_command::{HalMode, SlashCommand};
use crate::spec_prompts::{self, SpecAgent, SpecStage};
use codex_core::protocol::Op;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub fn validate_guardrail_schema(stage: SpecStage, telemetry: &Value) -> Vec<String> {
    let mut failures = Vec::new();

    match telemetry.get("command").and_then(|value| value.as_str()) {
        Some(command) if command == expected_guardrail_command(stage) => {}
        Some(command) => failures.push(format!(
            "Unexpected command '{}' (expected {})",
            command,
            expected_guardrail_command(stage)
        )),
        None => failures.push("Missing required string field command".to_string()),
    }

    require_string_field(telemetry, &["specId"], &mut failures);
    require_string_field(telemetry, &["sessionId"], &mut failures);
    require_string_field(telemetry, &["timestamp"], &mut failures);

    match stage {
        SpecStage::Validate | SpecStage::Audit => {
            if let Some(value) = telemetry.get("artifacts") {
                if !value.is_array() {
                    failures.push("Field artifacts must be an array when present".to_string());
                }
            }
        }
        _ => match telemetry.get("artifacts") {
            Some(Value::Array(arr)) => {
                if arr.is_empty() {
                    failures.push("Telemetry artifacts array is empty".to_string());
                }
            }
            Some(_) => failures.push("Field artifacts must be an array".to_string()),
            None => failures.push("Missing required array field artifacts".to_string()),
        },
    }

    match stage {
        SpecStage::Plan => {
            if require_object(telemetry, &["baseline"], &mut failures).is_some() {
                require_string_field(telemetry, &["baseline", "mode"], &mut failures);
                require_string_field(telemetry, &["baseline", "artifact"], &mut failures);
                require_string_field(telemetry, &["baseline", "status"], &mut failures);
            }
            if require_object(telemetry, &["hooks"], &mut failures).is_some() {
                require_string_field(telemetry, &["hooks", "session.start"], &mut failures);
            }
        }
        SpecStage::Tasks => {
            if require_object(telemetry, &["tool"], &mut failures).is_some() {
                require_string_field(telemetry, &["tool", "status"], &mut failures);
            }
        }
        SpecStage::Implement => {
            require_string_field(telemetry, &["lock_status"], &mut failures);
            require_string_field(telemetry, &["hook_status"], &mut failures);
        }
        SpecStage::Validate | SpecStage::Audit => {
            match telemetry.get("scenarios") {
                Some(Value::Array(scenarios)) if !scenarios.is_empty() => {
                    for (idx, scenario) in scenarios.iter().enumerate() {
                        if !scenario.is_object() {
                            failures.push(format!("Scenario #{} must be an object", idx + 1));
                            continue;
                        }
                        require_string_field(scenario, &["name"], &mut failures);
                        if let Some(status) =
                            require_string_field(scenario, &["status"], &mut failures)
                        {
                            const ALLOWED: &[&str] = &["passed", "failed", "skipped"];
                            if !ALLOWED.contains(&status) {
                                failures.push(format!(
                                    "Scenario status must be one of {:?} (got '{}')",
                                    ALLOWED, status
                                ));
                            }
                        }
                    }
                }
                Some(Value::Array(_)) => {
                    failures.push("Scenarios array must not be empty".to_string())
                }
                Some(_) => failures.push("Field scenarios must be an array of objects".to_string()),
                None => failures.push("Missing required array field scenarios".to_string()),
            }

            if let Some(hal_value) = telemetry.get("hal") {
                if let Some(summary_value) = hal_value.get("summary") {
                    if let Some(summary) = summary_value.as_object() {
                        if let Some(status) = summary.get("status").and_then(|s| s.as_str()) {
                            const ALLOWED: &[&str] = &["passed", "failed", "skipped"];
                            if !ALLOWED.contains(&status) {
                                failures.push(format!(
                                    "Field hal.summary.status must be one of {:?} (got '{}')",
                                    ALLOWED, status
                                ));
                            }
                        } else {
                            failures.push(
                                "Missing required string field hal.summary.status".to_string(),
                            );
                        }

                        if let Some(failed_checks) = summary.get("failed_checks") {
                            match failed_checks.as_array() {
                                Some(entries) => {
                                    for (idx, entry) in entries.iter().enumerate() {
                                        match entry.as_str() {
                                            Some(text) if !text.trim().is_empty() => {}
                                            _ => failures.push(format!(
                                                "hal.summary.failed_checks[{}] must be a non-empty string",
                                                idx
                                            )),
                                        }
                                    }
                                }
                                None => failures.push(
                                    "Field hal.summary.failed_checks must be an array of strings"
                                        .to_string(),
                                ),
                            }
                        }

                        if let Some(artifacts) = summary.get("artifacts") {
                            match artifacts.as_array() {
                                Some(entries) => {
                                    for (idx, entry) in entries.iter().enumerate() {
                                        match entry.as_str() {
                                            Some(text) if !text.trim().is_empty() => {}
                                            _ => failures.push(format!(
                                                "hal.summary.artifacts[{}] must be a non-empty string",
                                                idx
                                            )),
                                        }
                                    }
                                }
                                None => failures.push(
                                    "Field hal.summary.artifacts must be an array of strings"
                                        .to_string(),
                                ),
                            }
                        }
                    } else {
                        failures.push("Field hal.summary must be an object".to_string());
                    }
                } else {
                    failures.push("Missing required object field hal.summary".to_string());
                }
            }
        }
        SpecStage::Unlock => {
            require_string_field(telemetry, &["unlock_status"], &mut failures);
        }
    }

    failures
}

pub fn evaluate_guardrail_value(stage: SpecStage, value: &Value) -> GuardrailEvaluation {
    match stage {
        SpecStage::Plan => {
            let baseline = value
                .get("baseline")
                .and_then(|b| b.get("status"))
                .and_then(|s| s.as_str())
                .unwrap_or("unknown");
            let hook = value
                .get("hooks")
                .and_then(|h| h.get("session.start"))
                .and_then(|s| s.as_str())
                .unwrap_or("unknown");
            let baseline_ok = matches!(baseline, "passed" | "skipped");
            let hook_ok = hook == "ok";
            let success = baseline_ok && hook_ok;
            let mut failures = Vec::new();
            if !baseline_ok {
                failures.push(format!("Baseline audit status: {baseline}"));
            }
            if !hook_ok {
                failures.push(format!("session.start hook: {hook}"));
            }
            let summary = format!("Baseline {baseline}, session.start {hook}");
            GuardrailEvaluation {
                success,
                summary,
                failures,
            }
        }
        SpecStage::Tasks => {
            let status = value
                .get("tool")
                .and_then(|t| t.get("status"))
                .and_then(|s| s.as_str())
                .unwrap_or("unknown");
            let success = status == "ok";
            let failures = if success {
                Vec::new()
            } else {
                vec![format!("tasks hook status: {status}")]
            };
            GuardrailEvaluation {
                success,
                summary: format!("Tasks automation status: {status}"),
                failures,
            }
        }
        SpecStage::Implement => {
            let lock_status = value
                .get("lock_status")
                .and_then(|s| s.as_str())
                .unwrap_or("unknown");
            let hook_status = value
                .get("hook_status")
                .and_then(|s| s.as_str())
                .unwrap_or("unknown");
            let success = lock_status == "locked" && hook_status == "ok";
            let mut failures = Vec::new();
            if lock_status != "locked" {
                failures.push(format!("SPEC lock status: {lock_status}"));
            }
            if hook_status != "ok" {
                failures.push(format!("file_after_write hook: {hook_status}"));
            }
            GuardrailEvaluation {
                success,
                summary: format!("Lock status {lock_status}, file hook {hook_status}"),
                failures,
            }
        }
        SpecStage::Validate | SpecStage::Audit => {
            let mut failures = Vec::new();
            let scenarios = value
                .get("scenarios")
                .and_then(|s| s.as_array())
                .cloned()
                .unwrap_or_default();
            let mut total = 0usize;
            let mut passed = 0usize;
            for scenario in scenarios {
                let name = scenario
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("unknown");
                let status = scenario
                    .get("status")
                    .and_then(|s| s.as_str())
                    .unwrap_or("unknown");
                total += 1;
                if status == "passed" || status == "skipped" {
                    if status == "passed" {
                        passed += 1;
                    }
                    continue;
                }
                failures.push(format!("{name}: {status}"));
            }
            let success = failures.is_empty();
            let mut summary = if total == 0 {
                "No validation scenarios reported".to_string()
            } else {
                format!("{} of {} scenarios passed", passed, total)
            };

            if let Some(hal_summary) = value
                .get("hal")
                .and_then(|hal| hal.get("summary"))
                .and_then(|summary| summary.as_object())
            {
                if let Some(status) = hal_summary.get("status").and_then(|s| s.as_str()) {
                    summary = format!("{summary}; HAL {status}");
                    if status == "failed" {
                        if let Some(checks) = hal_summary
                            .get("failed_checks")
                            .and_then(|list| list.as_array())
                        {
                            let joined = checks
                                .iter()
                                .filter_map(|v| v.as_str())
                                .filter(|text| !text.trim().is_empty())
                                .collect::<Vec<_>>()
                                .join(", ");
                            if !joined.is_empty() {
                                failures.push(format!("HAL failed checks: {joined}"));
                            }
                        }
                    }
                }
            }

            GuardrailEvaluation {
                success,
                summary,
                failures,
            }
        }
        SpecStage::Unlock => {
            let status = value
                .get("unlock_status")
                .and_then(|s| s.as_str())
                .unwrap_or("unknown");
            let success = status == "unlocked";
            let failures = if success {
                Vec::new()
            } else {
                vec![format!("Unlock status: {status}")]
            };
            GuardrailEvaluation {
                success,
                summary: format!("Unlock status: {status}"),
                failures,
            }
        }
    }
}

/// Read latest spec-ops telemetry file for spec/stage
pub fn read_latest_spec_ops_telemetry(
    cwd: &Path,
    spec_id: &str,
    stage: SpecStage,
) -> Result<(PathBuf, Value)> {
    let evidence_dir = cwd
        .join("docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands")
        .join(spec_id);
    let prefix = super::state::spec_ops_stage_prefix(stage);
    let entries = std::fs::read_dir(&evidence_dir).map_err(|e| SpecKitError::DirectoryRead {
        path: evidence_dir.clone(),
        source: e,
    })?;

    let mut latest: Option<(PathBuf, SystemTime)> = None;
    for entry_res in entries {
        let entry = entry_res.map_err(|e| SpecKitError::DirectoryRead {
            path: evidence_dir.clone(),
            source: e,
        })?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with(prefix) {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        if latest
            .as_ref()
            .map(|(_, ts)| modified > *ts)
            .unwrap_or(true)
        {
            latest = Some((path.clone(), modified));
        }
    }

    let (path, _) = latest.ok_or_else(|| SpecKitError::NoTelemetryFound {
        spec_id: spec_id.to_string(),
        stage: stage.command_name().to_string(),
        pattern: format!("{}*", prefix),
        directory: evidence_dir.clone(),
    })?;

    let mut file = std::fs::File::open(&path).map_err(|e| SpecKitError::FileRead {
        path: path.clone(),
        source: e,
    })?;
    let mut buf = String::new();
    std::io::Read::read_to_string(&mut file, &mut buf).map_err(|e| SpecKitError::FileRead {
        path: path.clone(),
        source: e,
    })?;
    let value: Value =
        serde_json::from_str(&buf).map_err(|e| SpecKitError::JsonParse { path: path.clone(), source: e })?;
    Ok((path, value))
}

/// Collect guardrail outcome by reading telemetry and validating
pub fn collect_guardrail_outcome(
    cwd: &Path,
    spec_id: &str,
    stage: SpecStage,
) -> Result<GuardrailOutcome> {
    let (path, value) = read_latest_spec_ops_telemetry(cwd, spec_id, stage)?;
    let mut evaluation = evaluate_guardrail_value(stage, &value);
    let schema_failures = validate_guardrail_schema(stage, &value);
    if !schema_failures.is_empty() {
        evaluation.failures.extend(schema_failures);
        evaluation.success = false;
    }
    if matches!(
        stage,
        SpecStage::Plan
            | SpecStage::Tasks
            | SpecStage::Implement
            | SpecStage::Audit
            | SpecStage::Unlock
    ) {
        let (evidence_failures, artifact_count) = validate_guardrail_evidence(cwd, stage, &value);
        if artifact_count > 0 {
            evaluation.summary = format!("{} | {} artifacts", evaluation.summary, artifact_count);
        }
        if !evidence_failures.is_empty() {
            evaluation.failures.extend(evidence_failures);
            evaluation.success = false;
        }
    }
    Ok(GuardrailOutcome {
        success: evaluation.success,
        summary: evaluation.summary,
        telemetry_path: Some(path),
        failures: evaluation.failures,
    })
}

/// Map slash command to spec stage for multi-agent followup (if applicable)
fn spec_stage_for_multi_agent_followup(command: SlashCommand) -> Option<SpecStage> {
    match command {
        SlashCommand::SpecOpsPlan => Some(SpecStage::Plan),
        SlashCommand::SpecOpsTasks => Some(SpecStage::Tasks),
        SlashCommand::SpecOpsImplement => Some(SpecStage::Implement),
        SlashCommand::SpecOpsValidate => Some(SpecStage::Validate),
        SlashCommand::SpecOpsAudit => Some(SpecStage::Audit),
        SlashCommand::SpecOpsUnlock => Some(SpecStage::Unlock),
        _ => None,
    }
}

/// Implementation for guardrail command handler (extracted from ChatWidget)
pub fn handle_guardrail_impl(
    widget: &mut ChatWidget,
    command: SlashCommand,
    raw_args: String,
    hal_override: Option<HalMode>,
) {
    let Some(meta) = command.spec_ops() else {
        return;
    };
    let trimmed = raw_args.trim();
    let is_stats = meta.script == "evidence_stats.sh";

    let mut spec_id = String::new();
    let mut remainder = String::new();
    let mut spec_task = String::new();
    let mut hal_from_args: Option<HalMode> = None;

    if !is_stats {
        if trimmed.is_empty() {
            widget.history_push(crate::history_cell::new_error_event(format!(
                "`/{}` requires a SPEC ID (e.g. `/{} SPEC-OPS-005`).",
                command.command(),
                command.command()
            )));
            widget.request_redraw();
            return;
        }

        let mut tokens = trimmed.split_whitespace().peekable();
        spec_id = tokens.next().unwrap().to_string();

        let mut remainder_tokens: Vec<String> = Vec::new();
        while let Some(token) = tokens.next() {
            if token == "--hal" || token == "--hal-mode" {
                let Some(value) = tokens.next() else {
                    widget.history_push(crate::history_cell::new_error_event(
                        "`--hal` flag requires a value (mock|live).".to_string(),
                    ));
                    widget.request_redraw();
                    return;
                };
                hal_from_args = match HalMode::from_str(value) {
                    Some(mode) => Some(mode),
                    None => {
                        widget.history_push(crate::history_cell::new_error_event(format!(
                            "Unknown HAL mode '{value}'. Expected 'mock' or 'live'."
                        )));
                        widget.request_redraw();
                        return;
                    }
                };
                continue;
            }

            if let Some((flag, value)) = token.split_once('=') {
                if flag == "--hal" || flag == "--hal-mode" {
                    hal_from_args = match HalMode::from_str(value) {
                        Some(mode) => Some(mode),
                        None => {
                            widget.history_push(crate::history_cell::new_error_event(format!(
                                "Unknown HAL mode '{value}'. Expected 'mock' or 'live'."
                            )));
                            widget.request_redraw();
                            return;
                        }
                    };
                    continue;
                }
            }

            remainder_tokens.push(token.to_string());
        }

        remainder = remainder_tokens.join(" ");
        spec_task = if remainder_tokens.is_empty() {
            spec_id.clone()
        } else {
            format!("{spec_id} {remainder}")
        };
    }

    let script_path = if meta.script == "spec_auto.sh" || meta.script == "evidence_stats.sh" {
        format!("scripts/spec_ops_004/{}", meta.script)
    } else {
        format!("scripts/spec_ops_004/commands/{}", meta.script)
    };

    let mut banner = String::new();
    if is_stats {
        banner.push_str(&format!("Spec Ops /{}\n", meta.display));
    } else {
        banner.push_str(&format!("Spec Ops /{} → {}\n", meta.display, spec_id));
    }
    banner.push_str(&format!("  Script: {}\n", script_path));

    let mut stage_prompt_version: Option<String> = None;
    if script_path.contains("/commands/") {
        let resolution = codex_core::slash_commands::format_subagent_command(
            command.command(),
            &spec_task,
            Some(&widget.config.agents),
            Some(&widget.config.subagent_commands),
        );

        let agent_roster = if resolution.models.is_empty() {
            "claude,gemini,code".to_string()
        } else {
            resolution.models.join(",")
        };

        let prompt_hint = if resolution.orchestrator_instructions.is_some()
            || resolution.agent_instructions.is_some()
        {
            "custom prompt overrides"
        } else {
            "default prompt profile"
        };

        banner.push_str(&format!("  Agents: {}\n", agent_roster));
        banner.push_str(&format!("  Prompt: {}\n", prompt_hint));
    }

    let hal_mode = if hal_override.is_some() {
        hal_override
    } else {
        hal_from_args
    };

    if let Some(stage) = spec_stage_for_multi_agent_followup(command) {
        let stage_version = spec_prompts::stage_version_enum(stage);
        if let Some(version) = stage_version.clone() {
            stage_prompt_version = Some(version.clone());
            banner.push_str(&format!("  Prompt version: {}\n", version));
        }
        if spec_prompts::agent_prompt(stage.key(), SpecAgent::Gemini).is_some() {
            banner.push_str(&format!(
                "  Multi-agent stage available: /{}\n",
                stage.command_name()
            ));
        }
        if let Some(notes) = spec_prompts::orchestrator_notes(stage.key()) {
            if !notes.is_empty() {
                banner.push_str("  Notes:\n");
                for note in notes {
                    banner.push_str("    - ");
                    banner.push_str(&note);
                    banner.push('\n');
                }
            }
        }
    }

    if !is_stats {
        match hal_mode {
            Some(mode) => banner.push_str(&format!("  HAL mode: {}\n", mode.as_env_value())),
            None => banner.push_str("  HAL mode: mock (default)\n"),
        }
    }

    widget.insert_background_event_with_placement(banner, BackgroundPlacement::Tail);

    // Commands with scripts (guardrails, automation) execute via shell; otherwise, comments/notes only.
    if meta.script == "COMMENT_ONLY" {
        return;
    }

    let mut env = HashMap::new();
    if let Some(version) = stage_prompt_version {
        env.insert("SPEC_OPS_004_PROMPT_VERSION".to_string(), version);
    }
    if let Ok(prompt_version) = std::env::var("SPEC_OPS_004_PROMPT_VERSION") {
        env.insert("SPEC_OPS_004_PROMPT_VERSION".to_string(), prompt_version);
    }
    if let Ok(code_version) = std::env::var("SPEC_OPS_004_CODE_VERSION") {
        env.insert("SPEC_OPS_004_CODE_VERSION".to_string(), code_version);
    }

    if let Some(mode) = hal_mode {
        env.insert(
            "SPEC_OPS_HAL_MODE".to_string(),
            mode.as_env_value().to_string(),
        );
        if matches!(mode, HalMode::Live) {
            env.entry("SPEC_OPS_TELEMETRY_HAL".to_string())
                .or_insert_with(|| "1".to_string());
        }
    }

    let command_line = if is_stats {
        if trimmed.is_empty() {
            format!("scripts/env_run.sh {script_path}")
        } else {
            format!("scripts/env_run.sh {script_path} {trimmed}")
        }
    } else if remainder.is_empty() {
        format!("scripts/env_run.sh {script_path} {spec_id}")
    } else {
        format!("scripts/env_run.sh {script_path} {spec_id} {remainder}")
    };

    let wrapped = wrap_command(&command_line);
    if wrapped.is_empty() {
        widget.history_push(crate::history_cell::new_error_event(
            "Unable to build Spec Ops command invocation.".to_string(),
        ));
        widget.request_redraw();
        return;
    }

    widget.submit_op(Op::RunProjectCommand {
        name: format!("spec_ops_{}", meta.display),
        command: Some(wrapped),
        display: Some(command_line),
        env,
    });
    widget.request_redraw();
}
