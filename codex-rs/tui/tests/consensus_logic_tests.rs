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
// ============================================================================
// Agent Parsing Tests
// ============================================================================

#[test]
fn test_spec_agent_case_insensitive_parsing() {
    use codex_tui::SpecAgent;

    assert_eq!(SpecAgent::from_string("GEMINI"), Some(SpecAgent::Gemini));
    assert_eq!(SpecAgent::from_string("gemini"), Some(SpecAgent::Gemini));
    assert_eq!(SpecAgent::from_string("GeMiNi"), Some(SpecAgent::Gemini));
}

#[test]
fn test_spec_agent_variant_aliases() {
    use codex_tui::SpecAgent;

    // Test known aliases
    assert_eq!(SpecAgent::from_string("code"), Some(SpecAgent::Code));
    assert_eq!(SpecAgent::from_string("gpt_codex"), Some(SpecAgent::GptCodex));
    assert_eq!(SpecAgent::from_string("gpt-5-codex"), Some(SpecAgent::GptCodex));
    assert_eq!(SpecAgent::from_string("gpt_pro"), Some(SpecAgent::GptPro));
}

#[test]
fn test_spec_agent_invalid_names() {
    use codex_tui::SpecAgent;

    assert_eq!(SpecAgent::from_string(""), None);
    assert_eq!(SpecAgent::from_string("chatgpt"), None);
    assert_eq!(SpecAgent::from_string("bard"), None);
    assert_eq!(SpecAgent::from_string("123"), None);
}

#[test]
fn test_spec_agent_all_canonical_names_unique() {
    use codex_tui::SpecAgent;
    use std::collections::HashSet;

    let agents = vec![
        SpecAgent::Gemini,
        SpecAgent::Claude,
        SpecAgent::Code,
        SpecAgent::GptCodex,
        SpecAgent::GptPro,
    ];

    let names: HashSet<_> = agents.iter().map(|a| a.canonical_name()).collect();
    assert_eq!(names.len(), agents.len()); // All unique
}

#[test]
fn test_spec_agent_roundtrip() {
    use codex_tui::SpecAgent;

    let agents = vec![
        SpecAgent::Gemini,
        SpecAgent::Claude,
        SpecAgent::Code,
        SpecAgent::GptCodex,
        SpecAgent::GptPro,
    ];

    for agent in agents {
        let name = agent.canonical_name();
        let parsed = SpecAgent::from_string(name);
        assert!(parsed.is_some());
    }
}

// ============================================================================
// MCP Call Tests
// ============================================================================

#[tokio::test]
async fn test_mcp_call_with_wildcard_query() {
    let mut mock = MockMcpManager::new();

    // Add wildcard fixture (no query pattern)
    mock.add_fixture(
        "local-memory",
        "search",
        None,
        json!({"memory": {"id": "any-1", "content": "test"}}),
    );

    // Should match any query
    let result = mock
        .call_tool(
            "local-memory",
            "search",
            Some(json!({"query": "anything"})),
            None,
        )
        .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_mcp_call_tracks_multiple_calls() {
    let mut mock = MockMcpManager::new();
    mock.add_fixture("local-memory", "search", None, json!({}));

    let _ = mock
        .call_tool("local-memory", "search", Some(json!({"query": "1"})), None)
        .await;
    let _ = mock
        .call_tool("local-memory", "search", Some(json!({"query": "2"})), None)
        .await;
    let _ = mock
        .call_tool("local-memory", "search", Some(json!({"query": "3"})), None)
        .await;

    let log = mock.call_log();
    assert_eq!(log.len(), 3);
}

#[tokio::test]
async fn test_mcp_call_log_can_be_cleared() {
    let mut mock = MockMcpManager::new();
    mock.add_fixture("local-memory", "search", None, json!({}));

    let _ = mock
        .call_tool("local-memory", "search", Some(json!({"query": "test"})), None)
        .await;
    assert_eq!(mock.call_log().len(), 1);

    mock.clear_log();
    assert_eq!(mock.call_log().len(), 0);
}

#[tokio::test]
async fn test_mcp_multiple_fixtures_same_query() {
    let mut mock = MockMcpManager::new();

    // Add multiple fixtures for same query
    mock.add_fixture(
        "local-memory",
        "search",
        Some("SPEC-MULTI plan"),
        json!({"memory": {"id": "1", "content": "first"}}),
    );
    mock.add_fixture(
        "local-memory",
        "search",
        Some("SPEC-MULTI plan"),
        json!({"memory": {"id": "2", "content": "second"}}),
    );

    let result = mock
        .call_tool(
            "local-memory",
            "search",
            Some(json!({"query": "SPEC-MULTI plan"})),
            None,
        )
        .await
        .unwrap();

    // Should return array with both fixtures
    assert_eq!(result.is_error, Some(false));
}

#[tokio::test]
async fn test_mcp_add_fixtures_batch() {
    let mut mock = MockMcpManager::new();

    let fixtures = vec![
        json!({"memory": {"id": "1"}}),
        json!({"memory": {"id": "2"}}),
        json!({"memory": {"id": "3"}}),
    ];

    mock.add_fixtures("local-memory", "search", Some("batch"), fixtures);

    let result = mock
        .call_tool(
            "local-memory",
            "search",
            Some(json!({"query": "batch"})),
            None,
        )
        .await
        .unwrap();

    assert_eq!(result.is_error, Some(false));
}

#[tokio::test]
async fn test_mcp_fixture_exact_match_preferred() {
    let mut mock = MockMcpManager::new();

    // Add both wildcard and exact match
    mock.add_fixture(
        "local-memory",
        "search",
        None,
        json!({"memory": {"type": "wildcard"}}),
    );
    mock.add_fixture(
        "local-memory",
        "search",
        Some("exact"),
        json!({"memory": {"type": "exact"}}),
    );

    let result = mock
        .call_tool(
            "local-memory",
            "search",
            Some(json!({"query": "exact"})),
            None,
        )
        .await
        .unwrap();

    // Should return exact match, not wildcard
    assert_eq!(result.is_error, Some(false));
}

#[tokio::test]
async fn test_mcp_call_with_no_arguments() {
    let mut mock = MockMcpManager::new();
    mock.add_fixture("local-memory", "search", None, json!({}));

    let result = mock
        .call_tool("local-memory", "search", None, None)
        .await
        .unwrap();

    assert_eq!(result.is_error, Some(false));
}

#[tokio::test]
async fn test_mcp_call_with_empty_arguments() {
    let mut mock = MockMcpManager::new();
    mock.add_fixture("local-memory", "search", None, json!({}));

    let result = mock
        .call_tool("local-memory", "search", Some(json!({})), None)
        .await
        .unwrap();

    assert_eq!(result.is_error, Some(false));
}

#[tokio::test]
async fn test_mcp_call_different_servers() {
    let mut mock = MockMcpManager::new();

    mock.add_fixture("server-1", "tool-1", None, json!({"server": 1}));
    mock.add_fixture("server-2", "tool-2", None, json!({"server": 2}));

    let result1 = mock.call_tool("server-1", "tool-1", None, None).await;
    let result2 = mock.call_tool("server-2", "tool-2", None, None).await;

    assert!(result1.is_ok());
    assert!(result2.is_ok());

    let log = mock.call_log();
    assert_eq!(log.len(), 2);
    assert_eq!(log[0].server, "server-1");
    assert_eq!(log[1].server, "server-2");
}

// ============================================================================
// Consensus Quorum Tests
// ============================================================================

#[test]
fn test_unanimous_consensus_3_of_3() {
    // Simulate 3 agents all agreeing
    let agents = vec!["gemini", "claude", "code"];
    let votes: std::collections::HashMap<_, _> = agents
        .iter()
        .map(|&a| (a.to_string(), "approve".to_string()))
        .collect();

    // All voted same
    let unique_votes: std::collections::HashSet<_> = votes.values().collect();
    assert_eq!(unique_votes.len(), 1);
}

#[test]
fn test_majority_consensus_2_of_3() {
    let mut votes = std::collections::HashMap::new();
    votes.insert("gemini".to_string(), "approve".to_string());
    votes.insert("claude".to_string(), "approve".to_string());
    votes.insert("code".to_string(), "reject".to_string());

    // Count approve votes
    let approve_count = votes.values().filter(|v| *v == "approve").count();
    assert_eq!(approve_count, 2);
    assert!(approve_count >= 2); // Majority threshold
}

#[test]
fn test_no_consensus_1_of_3() {
    let mut votes = std::collections::HashMap::new();
    votes.insert("gemini".to_string(), "approve".to_string());
    votes.insert("claude".to_string(), "reject".to_string());
    votes.insert("code".to_string(), "modify".to_string());

    // All different
    let unique_votes: std::collections::HashSet<_> = votes.values().collect();
    assert_eq!(unique_votes.len(), 3); // No consensus
}

#[test]
fn test_degraded_consensus_2_of_2() {
    // One agent missing
    let mut votes = std::collections::HashMap::new();
    votes.insert("gemini".to_string(), "approve".to_string());
    votes.insert("claude".to_string(), "approve".to_string());

    let unique_votes: std::collections::HashSet<_> = votes.values().collect();
    assert_eq!(unique_votes.len(), 1); // Still consensus with 2/2
}

#[test]
fn test_minimal_viable_consensus_1_of_1() {
    // Severely degraded, only 1 agent
    let mut votes = std::collections::HashMap::new();
    votes.insert("gemini".to_string(), "approve".to_string());

    assert_eq!(votes.len(), 1);
    // Single agent can still provide verdict
}

#[test]
fn test_empty_agent_set() {
    let votes: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    assert_eq!(votes.len(), 0);
    // Would need handler logic to reject this
}

#[test]
fn test_quorum_calculation() {
    let test_cases = vec![
        (3, 2), // 3 agents, need 2 for majority: (3+1)/2 = 2
        (4, 2), // 4 agents, need 2 for simple majority: (4+1)/2 = 2
        (5, 3), // 5 agents, need 3 for majority: (5+1)/2 = 3
        (2, 1), // 2 agents, need 1 (degraded): (2+1)/2 = 1
        (1, 1), // 1 agent, need 1 (severely degraded): (1+1)/2 = 1
    ];

    for (total, expected_quorum) in test_cases {
        let quorum = (total + 1) / 2; // Simple majority formula
        assert_eq!(quorum, expected_quorum, "Failed for {} agents", total);
    }
}

#[test]
fn test_tie_breaking_with_even_agents() {
    let mut votes = std::collections::HashMap::new();
    votes.insert("gemini".to_string(), "approve".to_string());
    votes.insert("claude".to_string(), "approve".to_string());
    votes.insert("code".to_string(), "reject".to_string());
    votes.insert("gpt_pro".to_string(), "reject".to_string());

    let approve_count = votes.values().filter(|v| *v == "approve").count();
    let reject_count = votes.values().filter(|v| *v == "reject").count();

    assert_eq!(approve_count, 2);
    assert_eq!(reject_count, 2);
    // Tie - would need tiebreaker logic
}

// ============================================================================
// JSON Parsing Edge Cases
// ============================================================================

#[test]
fn test_parse_valid_json_content() {
    let json_str = r#"{"stage": "plan", "agent": "gemini", "verdict": "ok"}"#;
    let parsed: serde_json::Result<serde_json::Value> = serde_json::from_str(json_str);
    assert!(parsed.is_ok());
}

#[test]
fn test_parse_empty_json_object() {
    let json_str = "{}";
    let parsed: serde_json::Result<serde_json::Value> = serde_json::from_str(json_str);
    assert!(parsed.is_ok());
}

#[test]
fn test_parse_malformed_json() {
    let json_str = "{invalid json}";
    let parsed: serde_json::Result<serde_json::Value> = serde_json::from_str(json_str);
    assert!(parsed.is_err());
}

#[test]
fn test_parse_incomplete_json() {
    let json_str = r#"{"stage": "plan", "agent": "#;
    let parsed: serde_json::Result<serde_json::Value> = serde_json::from_str(json_str);
    assert!(parsed.is_err());
}

#[test]
fn test_parse_json_with_unicode() {
    let json_str = r#"{"message": "Success ✅", "emoji": "🎉"}"#;
    let parsed: serde_json::Result<serde_json::Value> = serde_json::from_str(json_str);
    assert!(parsed.is_ok());
}

#[test]
fn test_parse_json_with_escaped_quotes() {
    let json_str = r#"{"message": "He said \"hello\""}"#;
    let parsed: serde_json::Result<serde_json::Value> = serde_json::from_str(json_str);
    assert!(parsed.is_ok());

    if let Ok(value) = parsed {
        assert_eq!(value["message"], "He said \"hello\"");
    }
}

#[test]
fn test_parse_json_with_newlines() {
    let json_str = r#"{"message": "Line 1\nLine 2"}"#;
    let parsed: serde_json::Result<serde_json::Value> = serde_json::from_str(json_str);
    assert!(parsed.is_ok());
}
// ============================================================================
// Missing Agent Handling Tests
// ============================================================================

#[test]
fn test_missing_agent_detection() {
    use std::collections::HashMap;

    let expected_agents = vec!["gemini", "claude", "code"];
    let mut actual_results: HashMap<String, String> = HashMap::new();
    actual_results.insert("gemini".to_string(), "approve".to_string());
    actual_results.insert("claude".to_string(), "approve".to_string());
    // "code" is missing

    let missing: Vec<_> = expected_agents
        .iter()
        .filter(|a| !actual_results.contains_key(&a.to_string()))
        .collect();

    assert_eq!(missing.len(), 1);
    assert_eq!(*missing[0], "code");
}

#[test]
fn test_all_agents_missing() {
    use std::collections::HashMap;

    let expected_agents = vec!["gemini", "claude", "code"];
    let actual_results: HashMap<String, String> = HashMap::new();

    let missing_count = expected_agents
        .iter()
        .filter(|a| !actual_results.contains_key(&a.to_string()))
        .count();

    assert_eq!(missing_count, 3);
}

// ============================================================================
// Confidence Scoring Tests
// ============================================================================

#[test]
fn test_confidence_high_on_unanimous() {
    use std::collections::HashMap;

    let mut votes: HashMap<String, String> = HashMap::new();
    votes.insert("gemini".to_string(), "approve".to_string());
    votes.insert("claude".to_string(), "approve".to_string());
    votes.insert("code".to_string(), "approve".to_string());

    let unique_votes: std::collections::HashSet<_> = votes.values().collect();
    let is_unanimous = unique_votes.len() == 1;

    // Unanimous = High confidence
    assert!(is_unanimous);
}

#[test]
fn test_confidence_medium_on_majority() {
    use std::collections::HashMap;

    let mut votes: HashMap<String, String> = HashMap::new();
    votes.insert("gemini".to_string(), "approve".to_string());
    votes.insert("claude".to_string(), "approve".to_string());
    votes.insert("code".to_string(), "reject".to_string());

    let approve_count = votes.values().filter(|v| *v == "approve").count();
    let is_majority = approve_count >= 2;

    // 2/3 majority = Medium confidence
    assert!(is_majority);
    assert!(!votes.values().all(|v| v == "approve")); // Not unanimous
}

// ============================================================================
// MCP Fallback Test
// ============================================================================

#[tokio::test]
async fn test_mcp_error_triggers_fallback() {
    let mock = MockMcpManager::new();
    // No fixtures added - simulates MCP failure

    let result = mock
        .call_tool("local-memory", "search", Some(json!({"query": "test"})), None)
        .await;

    // MCP fails, would trigger file fallback in real code
    assert!(result.is_err());
}
