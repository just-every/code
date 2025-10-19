//! Tests for spec-kit JSON schemas
//!
//! Covers quality gate schemas, spec analysis schemas, and schema validation.

use codex_tui::{
    QualityGateType,
    quality_gate_response_schema, spec_analysis_schema,
    schema_for_gate_type, provider_supports_schemas,
};
use serde_json::json;

// ===== spec_analysis_schema Tests =====

#[test]
fn test_spec_analysis_schema_structure() {
    let schema = spec_analysis_schema();

    assert_eq!(schema["name"], "spec_analysis_response");
    assert_eq!(schema["strict"], true);
    assert!(schema["schema"]["properties"]["analysis"].is_object());
    assert!(schema["schema"]["properties"]["recommendations"].is_object());
}

#[test]
fn test_spec_analysis_schema_required_fields() {
    let schema = spec_analysis_schema();

    let required = schema["schema"]["required"].as_array().unwrap();
    assert_eq!(required.len(), 2);
    assert!(required.contains(&json!("analysis")));
    assert!(required.contains(&json!("recommendations")));
}

#[test]
fn test_spec_analysis_schema_recommendations_structure() {
    let schema = spec_analysis_schema();

    let rec_props = &schema["schema"]["properties"]["recommendations"]["items"]["properties"];
    assert!(rec_props["category"].is_object());
    assert!(rec_props["recommendation"].is_object());
    assert!(rec_props["priority"].is_object());
}

#[test]
fn test_spec_analysis_category_enum() {
    let schema = spec_analysis_schema();

    let category_enum = schema["schema"]["properties"]["recommendations"]["items"]["properties"]["category"]["enum"]
        .as_array()
        .unwrap();

    assert_eq!(category_enum.len(), 5);
    assert!(category_enum.contains(&json!("architecture")));
    assert!(category_enum.contains(&json!("requirements")));
    assert!(category_enum.contains(&json!("testing")));
    assert!(category_enum.contains(&json!("security")));
    assert!(category_enum.contains(&json!("performance")));
}

#[test]
fn test_spec_analysis_priority_enum() {
    let schema = spec_analysis_schema();

    let priority_enum = schema["schema"]["properties"]["recommendations"]["items"]["properties"]["priority"]["enum"]
        .as_array()
        .unwrap();

    assert_eq!(priority_enum.len(), 3);
    assert!(priority_enum.contains(&json!("high")));
    assert!(priority_enum.contains(&json!("medium")));
    assert!(priority_enum.contains(&json!("low")));
}

// ===== quality_gate_response_schema Enum Tests =====

#[test]
fn test_magnitude_enum_values() {
    let schema = quality_gate_response_schema();

    let magnitude_enum = schema["schema"]["properties"]["issues"]["items"]["properties"]["magnitude"]["enum"]
        .as_array()
        .unwrap();

    assert_eq!(magnitude_enum.len(), 3);
    assert!(magnitude_enum.contains(&json!("critical")));
    assert!(magnitude_enum.contains(&json!("important")));
    assert!(magnitude_enum.contains(&json!("minor")));
}

#[test]
fn test_resolvability_enum_values() {
    let schema = quality_gate_response_schema();

    let resolvability_enum = schema["schema"]["properties"]["issues"]["items"]["properties"]["resolvability"]["enum"]
        .as_array()
        .unwrap();

    assert_eq!(resolvability_enum.len(), 3);
    assert!(resolvability_enum.contains(&json!("auto-fix")));
    assert!(resolvability_enum.contains(&json!("suggest-fix")));
    assert!(resolvability_enum.contains(&json!("need-human")));
}

// ===== Provider Support Tests =====

#[test]
fn test_provider_supports_openai() {
    assert!(provider_supports_schemas("openai"));
}

#[test]
fn test_provider_supports_anthropic() {
    assert!(provider_supports_schemas("anthropic"));
}

#[test]
fn test_provider_not_supports_ollama() {
    assert!(!provider_supports_schemas("ollama"));
}

#[test]
fn test_provider_not_supports_gemini() {
    assert!(!provider_supports_schemas("gemini"));
}

#[test]
fn test_provider_not_supports_unknown() {
    assert!(!provider_supports_schemas("unknown-provider"));
}

// ===== schema_for_gate_type Tests =====

#[test]
fn test_schema_for_clarify_gate() {
    let schema = schema_for_gate_type(QualityGateType::Clarify);
    assert_eq!(schema["name"], "quality_gate_response");
}

#[test]
fn test_schema_for_checklist_gate() {
    let schema = schema_for_gate_type(QualityGateType::Checklist);
    assert_eq!(schema["name"], "quality_gate_response");
}

#[test]
fn test_schema_for_analyze_gate() {
    let schema = schema_for_gate_type(QualityGateType::Analyze);
    assert_eq!(schema["name"], "quality_gate_response");
}

// ===== Schema Completeness Tests =====

#[test]
fn test_quality_gate_schema_no_additional_properties() {
    let schema = quality_gate_response_schema();

    assert_eq!(schema["schema"]["additionalProperties"], false);
    assert_eq!(schema["schema"]["properties"]["issues"]["items"]["additionalProperties"], false);
}

#[test]
fn test_spec_analysis_schema_no_additional_properties() {
    let schema = spec_analysis_schema();

    assert_eq!(schema["schema"]["additionalProperties"], false);
    assert_eq!(schema["schema"]["properties"]["recommendations"]["items"]["additionalProperties"], false);
}

#[test]
fn test_quality_gate_issue_required_fields() {
    let schema = quality_gate_response_schema();

    let required = schema["schema"]["properties"]["issues"]["items"]["required"]
        .as_array()
        .unwrap();

    assert_eq!(required.len(), 6);
    assert!(required.contains(&json!("id")));
    assert!(required.contains(&json!("question")));
    assert!(required.contains(&json!("answer")));
    assert!(required.contains(&json!("confidence")));
    assert!(required.contains(&json!("magnitude")));
    assert!(required.contains(&json!("resolvability")));
}

#[test]
fn test_spec_analysis_recommendation_required_fields() {
    let schema = spec_analysis_schema();

    let required = schema["schema"]["properties"]["recommendations"]["items"]["required"]
        .as_array()
        .unwrap();

    assert_eq!(required.len(), 3);
    assert!(required.contains(&json!("category")));
    assert!(required.contains(&json!("recommendation")));
    assert!(required.contains(&json!("priority")));
}

// ===== Schema Field Description Tests =====

#[test]
fn test_quality_gate_issue_has_descriptions() {
    let schema = quality_gate_response_schema();
    let props = &schema["schema"]["properties"]["issues"]["items"]["properties"];

    assert!(props["id"]["description"].is_string());
    assert!(props["question"]["description"].is_string());
    assert!(props["answer"]["description"].is_string());
    assert!(props["confidence"]["description"].is_string());
}

#[test]
fn test_spec_analysis_fields_have_types() {
    let schema = spec_analysis_schema();

    assert_eq!(schema["schema"]["properties"]["analysis"]["type"], "string");
    assert_eq!(schema["schema"]["properties"]["recommendations"]["type"], "array");
}
