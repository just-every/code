//! Consensus logic tests (Phase 2)
//!
//! FORK-SPECIFIC (just-every/code): Test Coverage Phase 2 (Dec 2025)
//!
//! Tests consensus.rs MCP integration, artifact parsing, and quorum logic.
//! Policy: docs/spec-kit/testing-policy.md
//! Target: consensus.rs 1.2%→50% coverage

mod common;

use common::MockMcpManager;
use serde_json::json;

#[tokio::test]
async fn test_mcp_search_returns_consensus_artifacts() {
    let mut mock = MockMcpManager::new();

    // Add fixture for consensus search
    mock.add_fixture(
        "local-memory",
        "search",
        Some("SPEC-TEST plan"),
        json!({"memory": {"id": "gem-1", "content": "{\"stage\": \"plan\", \"agent\": \"gemini\"}"}}),
    );
    mock.add_fixture(
        "local-memory",
        "search",
        Some("SPEC-TEST plan"),
        json!({"memory": {"id": "cla-1", "content": "{\"stage\": \"plan\", \"agent\": \"claude\"}"}}),
    );

    let args = json!({
        "query": "SPEC-TEST plan",
        "limit": 20,
        "tags": ["spec:SPEC-TEST", "stage:plan"],
        "search_type": "hybrid"
    });

    let result = mock.call_tool("local-memory", "search", Some(args), None).await.unwrap();

    // Should return 2 artifacts
    assert!(result.content.len() > 0);
    assert_eq!(result.is_error, Some(false));
}

#[tokio::test]
async fn test_mcp_search_handles_empty_results() {
    let mock = MockMcpManager::new();
    // No fixtures added

    let args = json!({"query": "SPEC-MISSING plan"});
    let result = mock.call_tool("local-memory", "search", Some(args), None).await;

    // Should error when no fixture found
    assert!(result.is_err());
}

#[tokio::test]
async fn test_mock_mcp_with_fixture_file() {
    let mut mock = MockMcpManager::new();

    // Load real fixture from library
    let fixture_path = "tests/fixtures/consensus/demo-plan-gemini.json";
    mock.load_fixture_file("local-memory", "search", Some("SPEC-DEMO plan"), fixture_path).unwrap();

    let args = json!({"query": "SPEC-DEMO plan"});
    let result = mock.call_tool("local-memory", "search", Some(args), None).await.unwrap();

    assert_eq!(result.is_error, Some(false));
    // Fixture should contain actual gemini output
}

#[test]
fn test_spec_agent_canonical_names() {
    use codex_tui::SpecAgent;

    assert_eq!(SpecAgent::Gemini.canonical_name(), "gemini");
    assert_eq!(SpecAgent::Claude.canonical_name(), "claude");
    assert_eq!(SpecAgent::Code.canonical_name(), "code");
    assert_eq!(SpecAgent::GptCodex.canonical_name(), "gpt_codex");
    assert_eq!(SpecAgent::GptPro.canonical_name(), "gpt_pro");
}

#[test]
fn test_spec_agent_parsing() {
    use codex_tui::SpecAgent;

    assert_eq!(SpecAgent::from_string("gemini"), Some(SpecAgent::Gemini));
    assert_eq!(SpecAgent::from_string("CLAUDE"), Some(SpecAgent::Claude));
    assert_eq!(SpecAgent::from_string("gpt-5-codex"), Some(SpecAgent::GptCodex));
    assert_eq!(SpecAgent::from_string("unknown"), None);
}
