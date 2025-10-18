//! Tests for MockMcpManager
//!
//! FORK-SPECIFIC (just-every/code): Spec-kit test infrastructure (MAINT-3)

mod common;

use common::MockMcpManager;
use serde_json::json;

#[tokio::test]
async fn mock_mcp_returns_fixture() {
    let mut mock = MockMcpManager::new();
    mock.add_fixture(
        "local-memory",
        "search",
        Some("SPEC-TEST plan"),
        json!({"memory": {"id": "test-1", "content": "Test content"}}),
    );

    let args = json!({"query": "SPEC-TEST plan"});
    let result = mock.call_tool("local-memory", "search", Some(args), None).await.unwrap();

    assert_eq!(result.content.len(), 1);
    assert_eq!(result.is_error, Some(false));
}

#[tokio::test]
async fn mock_mcp_logs_calls() {
    let mut mock = MockMcpManager::new();
    mock.add_fixture("local-memory", "search", None, json!({}));

    let _ = mock.call_tool("local-memory", "search", Some(json!({"query": "test"})), None).await;

    let log = mock.call_log();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].server, "local-memory");
    assert_eq!(log[0].tool, "search");
}

#[tokio::test]
async fn mock_mcp_wildcard_matches() {
    let mut mock = MockMcpManager::new();
    // Add wildcard fixture (no query pattern)
    mock.add_fixture("local-memory", "search", None, json!({"memory": {"content": "Wildcard"}}));

    // Should match any query
    let result = mock.call_tool("local-memory", "search", Some(json!({"query": "anything"})), None).await.unwrap();

    assert_eq!(result.content.len(), 1);
}

#[tokio::test]
async fn mock_mcp_returns_error_when_no_fixture() {
    let mock = MockMcpManager::new();

    let result = mock.call_tool("local-memory", "search", Some(json!({"query": "missing"})), None).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("No fixture found"));
}
