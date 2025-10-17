//! Quality gate resolution logic (T85)
//!
//! Implements agent-driven auto-resolution with intelligent escalation

use super::error::{Result, SpecKitError};
use super::state::*;
use std::collections::HashMap;

/// Classify agent agreement level and find majority answer
///
/// Returns (confidence, majority_answer, dissenting_answer)
pub fn classify_issue_agreement(
    agent_answers: &HashMap<String, String>,
) -> (Confidence, Option<String>, Option<String>) {
    if agent_answers.len() < 3 {
        return (Confidence::Low, None, None);
    }

    // Count occurrences of each unique answer
    let mut answer_counts: HashMap<&String, usize> = HashMap::new();
    for answer in agent_answers.values() {
        *answer_counts.entry(answer).or_insert(0) += 1;
    }

    // Find answers by count
    let mut counts: Vec<(String, usize)> = answer_counts
        .into_iter()
        .map(|(k, v)| (k.clone(), v))
        .collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1)); // Sort by count descending

    let (majority_answer, majority_count) = counts
        .first()
        .map(|(ans, cnt)| (ans.clone(), *cnt))
        .unwrap_or((String::new(), 0));

    match majority_count {
        3 => {
            // All 3 agree
            (
                Confidence::High,
                Some(majority_answer.clone()),
                None,
            )
        }
        2 => {
            // 2 out of 3 agree
            let dissent = agent_answers
                .values()
                .find(|v| **v != majority_answer)
                .cloned();
            (
                Confidence::Medium,
                Some(majority_answer.clone()),
                dissent,
            )
        }
        _ => {
            // All different or only 1 agrees
            (Confidence::Low, None, None)
        }
    }
}

/// Determine if issue should be auto-resolved or escalated
///
/// Decision matrix:
/// - High confidence + Minor/Important → Auto-resolve
/// - Medium confidence + Minor + has fix → Auto-resolve
/// - Everything else → Escalate
pub fn should_auto_resolve(issue: &QualityIssue) -> bool {
    use Confidence::*;
    use Magnitude::*;
    use Resolvability::*;

    match (issue.confidence, issue.magnitude, issue.resolvability) {
        // High confidence cases
        (High, Minor, AutoFix) => true,
        (High, Minor, SuggestFix) => true,
        (High, Important, AutoFix) => true,

        // Medium confidence, only minor issues with auto-fix
        (Medium, Minor, AutoFix) => true,

        // Everything else escalates
        _ => false,
    }
}

/// Find majority answer from agent responses
pub fn find_majority_answer(agent_answers: &HashMap<String, String>) -> Option<String> {
    let (_, majority, _) = classify_issue_agreement(agent_answers);
    majority
}

/// Find dissenting answer (if exists)
pub fn find_dissent(agent_answers: &HashMap<String, String>) -> Option<String> {
    let (_, _, dissent) = classify_issue_agreement(agent_answers);
    dissent
}

/// Resolve a quality issue using classification + optional GPT-5 validation
pub fn resolve_quality_issue(issue: &QualityIssue) -> Resolution {
    let (confidence, majority_answer_opt, _dissent_opt) =
        classify_issue_agreement(&issue.agent_answers);

    match confidence {
        Confidence::High => {
            // Unanimous - auto-apply
            let answer = majority_answer_opt.expect("High confidence should have majority answer");
            Resolution::AutoApply {
                answer,
                confidence,
                reason: "Unanimous (3/3 agents agree)".to_string(),
                validation: None,
            }
        }

        Confidence::Medium => {
            // 2/3 majority - needs GPT-5 validation
            // For now, return placeholder - will be replaced with actual GPT-5 call
            let majority = majority_answer_opt.expect("Medium confidence should have majority");

            // Placeholder: In real implementation, this calls GPT-5
            // For now, we'll mark for validation
            Resolution::Escalate {
                reason: "Majority (2/3) - GPT-5 validation needed".to_string(),
                all_answers: issue.agent_answers.clone(),
                gpt5_reasoning: None,
                recommended: Some(majority),
            }
        }

        Confidence::Low => {
            // No consensus - escalate
            Resolution::Escalate {
                reason: "No agent consensus (0-1/3 agreement)".to_string(),
                all_answers: issue.agent_answers.clone(),
                gpt5_reasoning: None,
                recommended: None,
            }
        }
    }
}

/// Parse agent JSON result into QualityIssue
pub fn parse_quality_issue_from_agent(
    agent_name: &str,
    agent_result: &serde_json::Value,
    gate_type: QualityGateType,
) -> Result<Vec<QualityIssue>> {
    let issues_array = agent_result
        .get("issues")
        .and_then(|v| v.as_array())
        .ok_or_else(|| SpecKitError::from_string("Missing 'issues' array in agent result"))?;

    let mut parsed_issues = Vec::new();

    for (idx, issue_value) in issues_array.iter().enumerate() {
        let id = issue_value
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or(&format!("{}-{}", agent_name, idx))
            .to_string();

        let question = issue_value
            .get("question")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let answer = issue_value
            .get("answer")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let confidence_str = issue_value
            .get("confidence")
            .and_then(|v| v.as_str())
            .unwrap_or("low");
        let confidence = match confidence_str {
            "high" => Confidence::High,
            "medium" => Confidence::Medium,
            _ => Confidence::Low,
        };

        let magnitude_str = issue_value
            .get("magnitude")
            .or_else(|| issue_value.get("severity"))
            .and_then(|v| v.as_str())
            .unwrap_or("minor");
        let magnitude = match magnitude_str {
            "critical" => Magnitude::Critical,
            "important" => Magnitude::Important,
            _ => Magnitude::Minor,
        };

        let resolvability_str = issue_value
            .get("resolvability")
            .and_then(|v| v.as_str())
            .unwrap_or("need-human");
        let resolvability = match resolvability_str {
            "auto-fix" => Resolvability::AutoFix,
            "suggest-fix" => Resolvability::SuggestFix,
            _ => Resolvability::NeedHuman,
        };

        let reasoning = issue_value
            .get("reasoning")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let context = issue_value
            .get("context")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Single-agent issue (will be merged with other agents later)
        let mut agent_answers = HashMap::new();
        agent_answers.insert(agent_name.to_string(), answer.clone());

        let mut agent_reasoning = HashMap::new();
        agent_reasoning.insert(agent_name.to_string(), reasoning.clone());

        parsed_issues.push(QualityIssue {
            id: id.clone(),
            gate_type,
            issue_type: question.clone(),
            description: question,
            confidence, // Will be recalculated after merging agents
            magnitude,
            resolvability,
            suggested_fix: issue_value
                .get("suggested_fix")
                .or_else(|| issue_value.get("suggested_improvement"))
                .and_then(|v| v.as_str())
                .map(String::from),
            context,
            affected_artifacts: Vec::new(), // TODO: Parse from agent output
            agent_answers,
            agent_reasoning,
        });
    }

    Ok(parsed_issues)
}

/// Merge issues from multiple agents (same question ID)
pub fn merge_agent_issues(agent_issues: Vec<Vec<QualityIssue>>) -> Vec<QualityIssue> {
    let mut merged: HashMap<String, QualityIssue> = HashMap::new();

    for issues in agent_issues {
        for mut issue in issues {
            let id = issue.id.clone();

            if let Some(existing) = merged.get_mut(&id) {
                // Merge answers and reasoning
                for (agent, answer) in issue.agent_answers.drain() {
                    existing.agent_answers.insert(agent.clone(), answer);
                }
                for (agent, reasoning) in issue.agent_reasoning.drain() {
                    existing.agent_reasoning.insert(agent, reasoning);
                }

                // Recalculate confidence based on agreement
                let (new_confidence, _, _) = classify_issue_agreement(&existing.agent_answers);
                existing.confidence = new_confidence;

                // Use highest magnitude
                if matches!(issue.magnitude, Magnitude::Critical) {
                    existing.magnitude = Magnitude::Critical;
                } else if matches!(
                    (existing.magnitude, issue.magnitude),
                    (Magnitude::Minor, Magnitude::Important)
                ) {
                    existing.magnitude = Magnitude::Important;
                }

                // Use most conservative resolvability
                if matches!(issue.resolvability, Resolvability::NeedHuman) {
                    existing.resolvability = Resolvability::NeedHuman;
                }
            } else {
                merged.insert(id, issue);
            }
        }
    }

    merged.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_unanimous() {
        let mut answers = HashMap::new();
        answers.insert("gemini".to_string(), "yes".to_string());
        answers.insert("claude".to_string(), "yes".to_string());
        answers.insert("code".to_string(), "yes".to_string());

        let (confidence, majority, dissent) = classify_issue_agreement(&answers);

        assert_eq!(confidence, Confidence::High);
        assert_eq!(majority, Some("yes".to_string()));
        assert_eq!(dissent, None);
    }

    #[test]
    fn test_classify_majority() {
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
    fn test_classify_no_consensus() {
        let mut answers = HashMap::new();
        answers.insert("gemini".to_string(), "multiple".to_string());
        answers.insert("claude".to_string(), "single".to_string());
        answers.insert("code".to_string(), "maybe".to_string());

        let (confidence, majority, _dissent) = classify_issue_agreement(&answers);

        assert_eq!(confidence, Confidence::Low);
        assert_eq!(majority, None);
    }

    #[test]
    fn test_should_auto_resolve_high_minor() {
        let mut answers = HashMap::new();
        answers.insert("gemini".to_string(), "yes".to_string());
        answers.insert("claude".to_string(), "yes".to_string());
        answers.insert("code".to_string(), "yes".to_string());

        let issue = QualityIssue {
            id: "TEST-1".to_string(),
            gate_type: QualityGateType::Clarify,
            issue_type: "ambiguity".to_string(),
            description: "Should we log errors?".to_string(),
            confidence: Confidence::High,
            magnitude: Magnitude::Minor,
            resolvability: Resolvability::AutoFix,
            suggested_fix: Some("yes".to_string()),
            context: "Security best practice".to_string(),
            affected_artifacts: Vec::new(),
            agent_answers: answers,
            agent_reasoning: HashMap::new(),
        };

        assert!(should_auto_resolve(&issue));
    }

    #[test]
    fn test_should_escalate_critical() {
        let mut answers = HashMap::new();
        answers.insert("gemini".to_string(), "yes".to_string());
        answers.insert("claude".to_string(), "yes".to_string());
        answers.insert("code".to_string(), "yes".to_string());

        let issue = QualityIssue {
            id: "TEST-2".to_string(),
            gate_type: QualityGateType::Clarify,
            issue_type: "architectural".to_string(),
            description: "Microservices or monolith?".to_string(),
            confidence: Confidence::High,
            magnitude: Magnitude::Critical,  // Critical always escalates
            resolvability: Resolvability::SuggestFix,
            suggested_fix: Some("microservices".to_string()),
            context: "Architectural decision".to_string(),
            affected_artifacts: Vec::new(),
            agent_answers: answers,
            agent_reasoning: HashMap::new(),
        };

        assert!(!should_auto_resolve(&issue));
    }

    #[test]
    fn test_should_escalate_low_confidence() {
        let mut answers = HashMap::new();
        answers.insert("gemini".to_string(), "option A".to_string());
        answers.insert("claude".to_string(), "option B".to_string());
        answers.insert("code".to_string(), "option C".to_string());

        let issue = QualityIssue {
            id: "TEST-3".to_string(),
            gate_type: QualityGateType::Clarify,
            issue_type: "ambiguity".to_string(),
            description: "Which approach?".to_string(),
            confidence: Confidence::Low,
            magnitude: Magnitude::Minor,
            resolvability: Resolvability::AutoFix,
            suggested_fix: None,
            context: "".to_string(),
            affected_artifacts: Vec::new(),
            agent_answers: answers,
            agent_reasoning: HashMap::new(),
        };

        assert!(!should_auto_resolve(&issue));
    }

    #[test]
    fn test_resolution_unanimous() {
        let mut answers = HashMap::new();
        answers.insert("gemini".to_string(), "yes".to_string());
        answers.insert("claude".to_string(), "yes".to_string());
        answers.insert("code".to_string(), "yes".to_string());

        let mut reasoning = HashMap::new();
        reasoning.insert("gemini".to_string(), "Standard practice".to_string());

        let issue = QualityIssue {
            id: "TEST-4".to_string(),
            gate_type: QualityGateType::Clarify,
            issue_type: "ambiguity".to_string(),
            description: "Log errors?".to_string(),
            confidence: Confidence::High,
            magnitude: Magnitude::Minor,
            resolvability: Resolvability::AutoFix,
            suggested_fix: Some("yes".to_string()),
            context: "".to_string(),
            affected_artifacts: Vec::new(),
            agent_answers: answers,
            agent_reasoning: reasoning,
        };

        let resolution = resolve_quality_issue(&issue);

        match resolution {
            Resolution::AutoApply { answer, confidence, reason, .. } => {
                assert_eq!(answer, "yes");
                assert_eq!(confidence, Confidence::High);
                assert!(reason.contains("Unanimous"));
            }
            _ => panic!("Expected AutoApply for unanimous agreement"),
        }
    }

    #[test]
    fn test_resolution_majority_needs_validation() {
        let mut answers = HashMap::new();
        answers.insert("gemini".to_string(), "yes".to_string());
        answers.insert("claude".to_string(), "yes".to_string());
        answers.insert("code".to_string(), "no".to_string());

        let issue = QualityIssue {
            id: "TEST-5".to_string(),
            gate_type: QualityGateType::Clarify,
            issue_type: "ambiguity".to_string(),
            description: "Support feature X?".to_string(),
            confidence: Confidence::Medium,
            magnitude: Magnitude::Important,
            resolvability: Resolvability::SuggestFix,
            suggested_fix: Some("yes".to_string()),
            context: "".to_string(),
            affected_artifacts: Vec::new(),
            agent_answers: answers,
            agent_reasoning: HashMap::new(),
        };

        let resolution = resolve_quality_issue(&issue);

        match resolution {
            Resolution::Escalate { reason, recommended, .. } => {
                assert!(reason.contains("GPT-5 validation needed"));
                assert_eq!(recommended, Some("yes".to_string()));
            }
            _ => panic!("Expected Escalate for majority without validation"),
        }
    }

    #[test]
    fn test_resolution_no_consensus() {
        let mut answers = HashMap::new();
        answers.insert("gemini".to_string(), "A".to_string());
        answers.insert("claude".to_string(), "B".to_string());
        answers.insert("code".to_string(), "C".to_string());

        let issue = QualityIssue {
            id: "TEST-6".to_string(),
            gate_type: QualityGateType::Clarify,
            issue_type: "ambiguity".to_string(),
            description: "Which option?".to_string(),
            confidence: Confidence::Low,
            magnitude: Magnitude::Critical,
            resolvability: Resolvability::NeedHuman,
            suggested_fix: None,
            context: "".to_string(),
            affected_artifacts: Vec::new(),
            agent_answers: answers,
            agent_reasoning: HashMap::new(),
        };

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
    fn test_merge_agent_issues() {
        // Create same issue from 3 different agents
        let mut agent1_answers = HashMap::new();
        agent1_answers.insert("gemini".to_string(), "yes".to_string());

        let issue1 = QualityIssue {
            id: "Q1".to_string(),
            gate_type: QualityGateType::Clarify,
            issue_type: "ambiguity".to_string(),
            description: "Test question".to_string(),
            confidence: Confidence::Low, // Will be recalculated
            magnitude: Magnitude::Minor,
            resolvability: Resolvability::AutoFix,
            suggested_fix: None,
            context: "".to_string(),
            affected_artifacts: Vec::new(),
            agent_answers: agent1_answers,
            agent_reasoning: HashMap::new(),
        };

        let mut agent2_answers = HashMap::new();
        agent2_answers.insert("claude".to_string(), "yes".to_string());
        let mut issue2 = issue1.clone();
        issue2.agent_answers = agent2_answers;

        let mut agent3_answers = HashMap::new();
        agent3_answers.insert("code".to_string(), "yes".to_string());
        let mut issue3 = issue1.clone();
        issue3.agent_answers = agent3_answers;

        let merged = merge_agent_issues(vec![vec![issue1], vec![issue2], vec![issue3]]);

        assert_eq!(merged.len(), 1);
        let merged_issue = &merged[0];
        assert_eq!(merged_issue.id, "Q1");
        assert_eq!(merged_issue.agent_answers.len(), 3);
        assert_eq!(merged_issue.confidence, Confidence::High); // Recalculated to unanimous
    }
}
