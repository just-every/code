//! Guardrail validation infrastructure for spec-kit automation
//!
//! This module handles guardrail script validation, telemetry parsing,
//! schema compliance checking, and outcome evaluation.

use crate::spec_prompts::SpecStage;
use serde_json::Value;
use super::state::{GuardrailEvaluation, expected_guardrail_command, require_string_field, require_object};

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
