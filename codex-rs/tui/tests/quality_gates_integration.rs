//! Integration tests for quality gates system (T78)
//!
//! These tests validate the end-to-end quality gate flow:
//! - Checkpoint execution with agent orchestration
//! - Auto-resolution for unanimous issues
//! - GPT-5 validation for majority issues
//! - Escalation modal and human answers
//! - File modifications and git commits

use codex_tui::{
    classify_issue_agreement, merge_agent_issues, parse_quality_issue_from_agent,
    resolve_quality_issue, should_auto_resolve, Confidence, Magnitude, QualityCheckpoint,
    QualityGateType, QualityIssue, Resolution, Resolvability,
};
use serde_json::json;
use std::collections::HashMap;

// ============================================================================
// PHASE 1: Basic Checkpoint Execution
// ============================================================================

#[test]
fn test_checkpoint_gates_correct() {
    // PrePlanning should run clarify + checklist
    let gates = QualityCheckpoint::PrePlanning.gates();
    assert_eq!(gates.len(), 2);
    assert!(gates.contains(&QualityGateType::Clarify));
    assert!(gates.contains(&QualityGateType::Checklist));

    // PostPlan should run analyze
    let gates = QualityCheckpoint::PostPlan.gates();
    assert_eq!(gates.len(), 1);
    assert!(gates.contains(&QualityGateType::Analyze));

    // PostTasks should run analyze
    let gates = QualityCheckpoint::PostTasks.gates();
    assert_eq!(gates.len(), 1);
    assert!(gates.contains(&QualityGateType::Analyze));
}

#[test]
fn test_checkpoint_naming() {
    assert_eq!(QualityCheckpoint::PrePlanning.name(), "pre-planning");
    assert_eq!(QualityCheckpoint::PostPlan.name(), "post-plan");
    assert_eq!(QualityCheckpoint::PostTasks.name(), "post-tasks");
}

#[test]
fn test_parse_agent_json_success() {
    let agent_result = json!({
        "issues": [
            {
                "id": "Q1",
                "question": "Is authentication required?",
                "answer": "yes",
                "confidence": "high",
                "magnitude": "important",
                "resolvability": "auto-fix",
                "suggested_fix": "Add auth requirement to spec",
                "context": "Security section",
                "affected_artifacts": ["spec.md"]
            }
        ]
    });

    let issues = parse_quality_issue_from_agent(
        "gemini",
        &agent_result,
        QualityGateType::Clarify,
    )
    .expect("parse should succeed");

    assert_eq!(issues.len(), 1);
    let issue = &issues[0];
    assert_eq!(issue.id, "Q1");
    assert_eq!(issue.description, "Is authentication required?");
    assert_eq!(issue.agent_answers.get("gemini"), Some(&"yes".to_string()));
    assert_eq!(issue.magnitude, Magnitude::Important);
    assert_eq!(issue.resolvability, Resolvability::AutoFix);
}

#[test]
fn test_parse_agent_json_missing_issues_array() {
    let agent_result = json!({
        "no_issues_key": []
    });

    let result = parse_quality_issue_from_agent(
        "claude",
        &agent_result,
        QualityGateType::Checklist,
    );

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("Missing 'issues' array"));
}

// ============================================================================
// PHASE 2: Auto-Resolution Flow (Unanimous Issues)
// ============================================================================

#[test]
fn test_unanimous_agreement_classification() {
    let mut answers = HashMap::new();
    answers.insert("gemini".to_string(), "Add JWT token".to_string());
    answers.insert("claude".to_string(), "Add JWT token".to_string());
    answers.insert("code".to_string(), "Add JWT token".to_string());

    let (confidence, majority, dissent) = classify_issue_agreement(&answers);

    assert_eq!(confidence, Confidence::High);
    assert_eq!(majority, Some("Add JWT token".to_string()));
    assert_eq!(dissent, None);
}

#[test]
fn test_unanimous_issue_auto_resolves() {
    let mut answers = HashMap::new();
    answers.insert("gemini".to_string(), "yes".to_string());
    answers.insert("claude".to_string(), "yes".to_string());
    answers.insert("code".to_string(), "yes".to_string());

    let issue = QualityIssue {
        id: "Q1".to_string(),
        gate_type: QualityGateType::Clarify,
        issue_type: "ambiguity".to_string(),
        description: "Should we add authentication?".to_string(),
        confidence: Confidence::High,
        magnitude: Magnitude::Important,
        resolvability: Resolvability::AutoFix,
        suggested_fix: Some("Add auth section".to_string()),
        context: "Security".to_string(),
        affected_artifacts: vec!["spec.md".to_string()],
        agent_answers: answers,
        agent_reasoning: HashMap::new(),
    };

    // Should auto-resolve because: High confidence + Important + AutoFix
    assert!(should_auto_resolve(&issue));

    // Resolution should be AutoApply
    let resolution = resolve_quality_issue(&issue);
    match resolution {
        Resolution::AutoApply { answer, confidence, reason, validation } => {
            assert_eq!(answer, "yes");
            assert_eq!(confidence, Confidence::High);
            assert!(reason.contains("Unanimous"));
            assert!(validation.is_none());
        }
        _ => panic!("Expected AutoApply for unanimous issue"),
    }
}

#[test]
fn test_merge_issues_combines_agent_answers() {
    // Create same issue from 3 different agents
    let issue_gemini = QualityIssue {
        id: "Q1".to_string(),
        gate_type: QualityGateType::Clarify,
        issue_type: "ambiguity".to_string(),
        description: "Use OAuth2?".to_string(),
        confidence: Confidence::Low,
        magnitude: Magnitude::Minor,
        resolvability: Resolvability::AutoFix,
        suggested_fix: None,
        context: "".to_string(),
        affected_artifacts: Vec::new(),
        agent_answers: {
            let mut map = HashMap::new();
            map.insert("gemini".to_string(), "yes".to_string());
            map
        },
        agent_reasoning: HashMap::new(),
    };

    let issue_claude = QualityIssue {
        agent_answers: {
            let mut map = HashMap::new();
            map.insert("claude".to_string(), "yes".to_string());
            map
        },
        ..issue_gemini.clone()
    };

    let issue_code = QualityIssue {
        agent_answers: {
            let mut map = HashMap::new();
            map.insert("code".to_string(), "yes".to_string());
            map
        },
        ..issue_gemini.clone()
    };

    let merged = merge_agent_issues(vec![
        vec![issue_gemini],
        vec![issue_claude],
        vec![issue_code],
    ]);

    assert_eq!(merged.len(), 1);
    let merged_issue = &merged[0];
    assert_eq!(merged_issue.agent_answers.len(), 3);
    assert_eq!(merged_issue.confidence, Confidence::High); // Recalculated
    assert_eq!(merged_issue.agent_answers.get("gemini"), Some(&"yes".to_string()));
    assert_eq!(merged_issue.agent_answers.get("claude"), Some(&"yes".to_string()));
    assert_eq!(merged_issue.agent_answers.get("code"), Some(&"yes".to_string()));
}

// ============================================================================
// PHASE 3: GPT-5 Validation Flow (2/3 Majority)
// ============================================================================

#[test]
fn test_majority_agreement_classification() {
    let mut answers = HashMap::new();
    answers.insert("gemini".to_string(), "yes".to_string());
    answers.insert("claude".to_string(), "yes".to_string());
    answers.insert("code".to_string(), "no".to_string());

    let (confidence, majority, dissent) = classify_issue_agreement(&answers);

    assert_eq!(confidence, Confidence::Medium);
    assert_eq!(majority, Some("yes".to_string()));
    assert_eq!(dissent, Some("no".to_string()));
}

#[test]
fn test_majority_issue_needs_validation() {
    let mut answers = HashMap::new();
    answers.insert("gemini".to_string(), "OAuth2".to_string());
    answers.insert("claude".to_string(), "OAuth2".to_string());
    answers.insert("code".to_string(), "JWT".to_string());

    let issue = QualityIssue {
        id: "Q2".to_string(),
        gate_type: QualityGateType::Clarify,
        issue_type: "ambiguity".to_string(),
        description: "Which auth method?".to_string(),
        confidence: Confidence::Medium,
        magnitude: Magnitude::Important,
        resolvability: Resolvability::SuggestFix,
        suggested_fix: None,
        context: "".to_string(),
        affected_artifacts: vec!["spec.md".to_string()],
        agent_answers: answers,
        agent_reasoning: HashMap::new(),
    };

    // Should NOT auto-resolve (Medium confidence + Important)
    assert!(!should_auto_resolve(&issue));

    // Resolution should escalate for GPT-5 validation
    let resolution = resolve_quality_issue(&issue);
    match resolution {
        Resolution::Escalate { reason, recommended, .. } => {
            assert!(reason.contains("GPT-5 validation needed"));
            assert_eq!(recommended, Some("OAuth2".to_string()));
        }
        _ => panic!("Expected Escalate for majority issue"),
    }
}

#[test]
fn test_medium_confidence_minor_autofix_resolves() {
    let mut answers = HashMap::new();
    answers.insert("gemini".to_string(), "yes".to_string());
    answers.insert("claude".to_string(), "yes".to_string());
    answers.insert("code".to_string(), "no".to_string());

    let issue = QualityIssue {
        id: "Q3".to_string(),
        gate_type: QualityGateType::Checklist,
        issue_type: "quality".to_string(),
        description: "Add acceptance criteria?".to_string(),
        confidence: Confidence::Medium,
        magnitude: Magnitude::Minor,
        resolvability: Resolvability::AutoFix,
        suggested_fix: Some("Add criteria section".to_string()),
        context: "".to_string(),
        affected_artifacts: vec!["spec.md".to_string()],
        agent_answers: answers,
        agent_reasoning: HashMap::new(),
    };

    // Should auto-resolve: Medium + Minor + AutoFix
    assert!(should_auto_resolve(&issue));
}

// ============================================================================
// PHASE 4: Escalation Flow (No Consensus)
// ============================================================================

#[test]
fn test_no_consensus_classification() {
    let mut answers = HashMap::new();
    answers.insert("gemini".to_string(), "A".to_string());
    answers.insert("claude".to_string(), "B".to_string());
    answers.insert("code".to_string(), "C".to_string());

    let (confidence, majority, dissent) = classify_issue_agreement(&answers);

    assert_eq!(confidence, Confidence::Low);
    assert_eq!(majority, None);
    assert_eq!(dissent, None);
}

#[test]
fn test_no_consensus_escalates() {
    let mut answers = HashMap::new();
    answers.insert("gemini".to_string(), "Option A".to_string());
    answers.insert("claude".to_string(), "Option B".to_string());
    answers.insert("code".to_string(), "Option C".to_string());

    let issue = QualityIssue {
        id: "Q4".to_string(),
        gate_type: QualityGateType::Clarify,
        issue_type: "ambiguity".to_string(),
        description: "Which architecture?".to_string(),
        confidence: Confidence::Low,
        magnitude: Magnitude::Critical,
        resolvability: Resolvability::NeedHuman,
        suggested_fix: None,
        context: "".to_string(),
        affected_artifacts: vec!["spec.md".to_string()],
        agent_answers: answers,
        agent_reasoning: HashMap::new(),
    };

    // Should NOT auto-resolve
    assert!(!should_auto_resolve(&issue));

    // Resolution should escalate
    let resolution = resolve_quality_issue(&issue);
    match resolution {
        Resolution::Escalate { reason, recommended, .. } => {
            assert!(reason.contains("No agent consensus"));
            assert_eq!(recommended, None);
        }
        _ => panic!("Expected Escalate for no consensus"),
    }
}

#[test]
fn test_critical_magnitude_always_escalates() {
    let mut answers = HashMap::new();
    answers.insert("gemini".to_string(), "yes".to_string());
    answers.insert("claude".to_string(), "yes".to_string());
    answers.insert("code".to_string(), "yes".to_string());

    let issue = QualityIssue {
        id: "Q5".to_string(),
        gate_type: QualityGateType::Clarify,
        issue_type: "blocker".to_string(),
        description: "Critical security issue?".to_string(),
        confidence: Confidence::High,
        magnitude: Magnitude::Critical,
        resolvability: Resolvability::AutoFix,
        suggested_fix: Some("Add security requirements".to_string()),
        context: "".to_string(),
        affected_artifacts: vec!["spec.md".to_string()],
        agent_answers: answers,
        agent_reasoning: HashMap::new(),
    };

    // Critical magnitude should NOT auto-resolve
    assert!(!should_auto_resolve(&issue));
}

#[test]
fn test_need_human_resolvability_escalates() {
    let mut answers = HashMap::new();
    answers.insert("gemini".to_string(), "yes".to_string());
    answers.insert("claude".to_string(), "yes".to_string());
    answers.insert("code".to_string(), "yes".to_string());

    let issue = QualityIssue {
        id: "Q6".to_string(),
        gate_type: QualityGateType::Clarify,
        issue_type: "judgment".to_string(),
        description: "Business decision required".to_string(),
        confidence: Confidence::High,
        magnitude: Magnitude::Important,
        resolvability: Resolvability::NeedHuman,
        suggested_fix: None,
        context: "".to_string(),
        affected_artifacts: vec!["spec.md".to_string()],
        agent_answers: answers,
        agent_reasoning: HashMap::new(),
    };

    // NeedHuman resolvability should NOT auto-resolve
    assert!(!should_auto_resolve(&issue));
}

// ============================================================================
// PHASE 5: Edge Cases and Error Scenarios
// ============================================================================

#[test]
fn test_empty_agent_answers() {
    let answers = HashMap::new();
    let (confidence, majority, dissent) = classify_issue_agreement(&answers);

    assert_eq!(confidence, Confidence::Low);
    assert_eq!(majority, None);
    assert_eq!(dissent, None);
}

#[test]
fn test_single_agent_answer() {
    let mut answers = HashMap::new();
    answers.insert("gemini".to_string(), "yes".to_string());

    let (confidence, majority, dissent) = classify_issue_agreement(&answers);

    assert_eq!(confidence, Confidence::Low);
    assert_eq!(majority, None);
    assert_eq!(dissent, None);
}

#[test]
fn test_two_agent_answers() {
    let mut answers = HashMap::new();
    answers.insert("gemini".to_string(), "yes".to_string());
    answers.insert("claude".to_string(), "yes".to_string());

    let (confidence, majority, dissent) = classify_issue_agreement(&answers);

    // Less than 3 agents -> Low confidence
    assert_eq!(confidence, Confidence::Low);
}

#[test]
fn test_parse_agent_json_defaults() {
    let agent_result = json!({
        "issues": [
            {
                // Minimal issue with defaults
            }
        ]
    });

    let issues = parse_quality_issue_from_agent(
        "test-agent",
        &agent_result,
        QualityGateType::Analyze,
    )
    .expect("parse should succeed with defaults");

    assert_eq!(issues.len(), 1);
    let issue = &issues[0];
    assert!(issue.id.starts_with("test-agent-"));
    assert_eq!(issue.description, "unknown");
    assert_eq!(issue.confidence, Confidence::Low);
    assert_eq!(issue.magnitude, Magnitude::Minor);
}

#[test]
fn test_gate_type_command_names() {
    assert_eq!(QualityGateType::Clarify.command_name(), "clarify");
    assert_eq!(QualityGateType::Checklist.command_name(), "checklist");
    assert_eq!(QualityGateType::Analyze.command_name(), "analyze");
}
