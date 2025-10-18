//! JSON schemas for agent responses
//!
//! Enforces structured output from agents to prevent malformed JSON.
//! Used with OpenAI structured output mode when available.
//!
//! FORK-SPECIFIC (just-every/code): Agent response validation

use serde_json::json;

/// JSON schema for quality gate responses (clarify, checklist, analyze)
pub fn quality_gate_response_schema() -> serde_json::Value {
    json!({
        "name": "quality_gate_response",
        "strict": true,
        "schema": {
            "type": "object",
            "properties": {
                "issues": {
                    "type": "array",
                    "description": "List of quality issues found",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": {
                                "type": "string",
                                "description": "Unique issue identifier (e.g. Q1, Q2)"
                            },
                            "question": {
                                "type": "string",
                                "description": "The ambiguity, inconsistency, or issue found"
                            },
                            "answer": {
                                "type": "string",
                                "description": "Proposed resolution or answer"
                            },
                            "confidence": {
                                "type": "string",
                                "enum": ["high", "medium", "low"],
                                "description": "Confidence in the answer"
                            },
                            "magnitude": {
                                "type": "string",
                                "enum": ["critical", "important", "minor"],
                                "description": "Severity/impact of the issue"
                            },
                            "resolvability": {
                                "type": "string",
                                "enum": ["auto-fix", "suggest-fix", "need-human"],
                                "description": "Whether issue can be auto-resolved"
                            },
                            "context": {
                                "type": "string",
                                "description": "Additional context about the issue"
                            },
                            "suggested_fix": {
                                "type": "string",
                                "description": "Specific text or change to apply"
                            },
                            "reasoning": {
                                "type": "string",
                                "description": "Why this is an issue and why this answer is correct"
                            }
                        },
                        "required": ["id", "question", "answer", "confidence", "magnitude", "resolvability"],
                        "additionalProperties": false
                    }
                }
            },
            "required": ["issues"],
            "additionalProperties": false
        }
    })
}

/// Schema for spec analysis (plan, tasks stages)
pub fn spec_analysis_schema() -> serde_json::Value {
    json!({
        "name": "spec_analysis_response",
        "strict": true,
        "schema": {
            "type": "object",
            "properties": {
                "analysis": {
                    "type": "string",
                    "description": "Overall analysis of the spec/plan/tasks"
                },
                "recommendations": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "category": {
                                "type": "string",
                                "enum": ["architecture", "requirements", "testing", "security", "performance"]
                            },
                            "recommendation": {
                                "type": "string"
                            },
                            "priority": {
                                "type": "string",
                                "enum": ["high", "medium", "low"]
                            }
                        },
                        "required": ["category", "recommendation", "priority"],
                        "additionalProperties": false
                    }
                }
            },
            "required": ["analysis", "recommendations"],
            "additionalProperties": false
        }
    })
}

/// Get appropriate schema for quality gate type
pub fn schema_for_gate_type(gate_type: super::state::QualityGateType) -> serde_json::Value {
    use super::state::QualityGateType;

    match gate_type {
        QualityGateType::Clarify | QualityGateType::Checklist | QualityGateType::Analyze => {
            quality_gate_response_schema()
        }
    }
}

/// Check if provider supports structured output
pub fn provider_supports_schemas(provider_id: &str) -> bool {
    matches!(
        provider_id,
        "openai" | "anthropic"  // Add more as supported
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quality_gate_schema_structure() {
        let schema = quality_gate_response_schema();
        assert_eq!(schema["name"], "quality_gate_response");
        assert_eq!(schema["strict"], true);
        assert!(schema["schema"]["properties"]["issues"].is_object());
    }

    #[test]
    fn test_schema_has_required_fields() {
        let schema = quality_gate_response_schema();
        let required = schema["schema"]["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "issues");
    }

    #[test]
    fn test_issue_item_schema_complete() {
        let schema = quality_gate_response_schema();
        let item_props = &schema["schema"]["properties"]["issues"]["items"]["properties"];

        assert!(item_props["id"].is_object());
        assert!(item_props["question"].is_object());
        assert!(item_props["answer"].is_object());
        assert!(item_props["confidence"].is_object());
        assert!(item_props["magnitude"].is_object());
        assert!(item_props["resolvability"].is_object());
    }

    #[test]
    fn test_confidence_enum_values() {
        let schema = quality_gate_response_schema();
        let confidence_enum = schema["schema"]["properties"]["issues"]["items"]["properties"]["confidence"]["enum"]
            .as_array()
            .unwrap();

        assert_eq!(confidence_enum.len(), 3);
        assert!(confidence_enum.contains(&json!("high")));
        assert!(confidence_enum.contains(&json!("medium")));
        assert!(confidence_enum.contains(&json!("low")));
    }

    #[test]
    fn test_provider_support_detection() {
        assert!(provider_supports_schemas("openai"));
        assert!(!provider_supports_schemas("ollama"));
    }

    #[test]
    fn test_schema_for_gate_type() {
        use super::super::state::QualityGateType;

        let clarify_schema = schema_for_gate_type(QualityGateType::Clarify);
        assert_eq!(clarify_schema["name"], "quality_gate_response");

        let checklist_schema = schema_for_gate_type(QualityGateType::Checklist);
        assert_eq!(checklist_schema["name"], "quality_gate_response");
    }
}
