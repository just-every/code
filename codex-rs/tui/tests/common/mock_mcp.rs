//! Mock MCP Connection Manager for testing
//!
//! FORK-SPECIFIC (just-every/code): Spec-kit test infrastructure (MAINT-3)
//!
//! Provides fixture-based MockMcpManager for testing MCP-dependent code
//! without requiring actual local-memory MCP server.

use anyhow::{Result, anyhow};
use mcp_types::{CallToolResult, ContentBlock, TextContent};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Mock MCP manager that returns fixture responses
///
/// Usage:
/// ```rust
/// let mut mock = MockMcpManager::new();
/// mock.add_fixture("local-memory", "search", "SPEC-KIT-065", fixture_json);
/// let result = mock.call_tool("local-memory", "search", args, None).await?;
/// ```
pub struct MockMcpManager {
    fixtures: Arc<Mutex<HashMap<FixtureKey, Vec<Value>>>>,
    call_log: Arc<Mutex<Vec<CallLogEntry>>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FixtureKey {
    server: String,
    tool: String,
    query_pattern: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CallLogEntry {
    pub server: String,
    pub tool: String,
    pub arguments: Option<Value>,
}

impl MockMcpManager {
    /// Create new mock MCP manager
    pub fn new() -> Self {
        Self {
            fixtures: Arc::new(Mutex::new(HashMap::new())),
            call_log: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Add fixture response for specific server/tool/query
    ///
    /// If query_pattern is None, fixture matches any query for that server/tool.
    /// Multiple fixtures can be added; they'll be returned as array.
    pub fn add_fixture(
        &mut self,
        server: &str,
        tool: &str,
        query_pattern: Option<&str>,
        fixture: Value,
    ) {
        let key = FixtureKey {
            server: server.to_string(),
            tool: tool.to_string(),
            query_pattern: query_pattern.map(String::from),
        };

        let mut fixtures = self.fixtures.lock().unwrap();
        fixtures.entry(key).or_insert_with(Vec::new).push(fixture);
    }

    /// Add multiple fixtures from array
    pub fn add_fixtures(
        &mut self,
        server: &str,
        tool: &str,
        query_pattern: Option<&str>,
        fixture_array: Vec<Value>,
    ) {
        for fixture in fixture_array {
            self.add_fixture(server, tool, query_pattern, fixture);
        }
    }

    /// Load fixture from JSON file
    pub fn load_fixture_file(&mut self, server: &str, tool: &str, query_pattern: Option<&str>, path: &str) -> Result<()> {
        let contents = std::fs::read_to_string(path)?;
        let fixture: Value = serde_json::from_str(&contents)?;
        self.add_fixture(server, tool, query_pattern, fixture);
        Ok(())
    }

    /// Get call log (for assertions)
    pub fn call_log(&self) -> Vec<CallLogEntry> {
        self.call_log.lock().unwrap().clone()
    }

    /// Clear call log
    pub fn clear_log(&mut self) {
        self.call_log.lock().unwrap().clear();
    }

    /// Simulate call_tool - returns fixtures matching server/tool/query
    pub async fn call_tool(
        &self,
        server: &str,
        tool: &str,
        arguments: Option<Value>,
        _timeout: Option<Duration>,
    ) -> Result<CallToolResult> {
        // Log the call
        {
            let mut log = self.call_log.lock().unwrap();
            log.push(CallLogEntry {
                server: server.to_string(),
                tool: tool.to_string(),
                arguments: arguments.clone(),
            });
        }

        // Extract query from arguments if present
        let query = arguments
            .as_ref()
            .and_then(|args| args.get("query"))
            .and_then(|q| q.as_str());

        // Try to find matching fixture
        let fixtures_guard = self.fixtures.lock().unwrap();

        // First try exact query match
        if let Some(q) = query {
            let key = FixtureKey {
                server: server.to_string(),
                tool: tool.to_string(),
                query_pattern: Some(q.to_string()),
            };

            if let Some(fixture_list) = fixtures_guard.get(&key) {
                return Ok(build_call_tool_result(fixture_list.clone()));
            }
        }

        // Fall back to wildcard (no query pattern)
        let wildcard_key = FixtureKey {
            server: server.to_string(),
            tool: tool.to_string(),
            query_pattern: None,
        };

        if let Some(fixture_list) = fixtures_guard.get(&wildcard_key) {
            return Ok(build_call_tool_result(fixture_list.clone()));
        }

        // No fixture found
        Err(anyhow!(
            "No fixture found for {}/{} (query: {:?})",
            server,
            tool,
            query
        ))
    }
}

impl Default for MockMcpManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Build CallToolResult from fixture array
fn build_call_tool_result(fixtures: Vec<Value>) -> CallToolResult {
    // Return fixtures as JSON array in TextContent
    let json_array = Value::Array(fixtures);
    let text = serde_json::to_string(&json_array).unwrap_or_else(|_| "[]".to_string());

    CallToolResult {
        content: vec![ContentBlock::TextContent(TextContent {
            text,
            r#type: "text".to_string(),
            annotations: None,
        })],
        is_error: Some(false),
        structured_content: None,
    }
}

// Tests for MockMcpManager (run with cargo test --test mock_mcp_tests)
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_mock_mcp_returns_fixture() {
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
        if let ContentBlock::TextContent(text) = &result.content[0] {
            assert!(text.text.contains("Test content"));
        } else {
            panic!("Expected TextContent");
        }
    }

    #[tokio::test]
    async fn test_mock_mcp_logs_calls() {
        let mut mock = MockMcpManager::new();
        mock.add_fixture("local-memory", "search", None, json!({}));

        let _ = mock.call_tool("local-memory", "search", Some(json!({"query": "test"})), None).await;

        let log = mock.call_log();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].server, "local-memory");
        assert_eq!(log[0].tool, "search");
    }

    #[tokio::test]
    async fn test_mock_mcp_wildcard_matches() {
        let mut mock = MockMcpManager::new();
        // Add wildcard fixture (no query pattern)
        mock.add_fixture("local-memory", "search", None, json!({"memory": {"content": "Wildcard"}}));

        // Should match any query
        let result = mock.call_tool("local-memory", "search", Some(json!({"query": "anything"})), None).await.unwrap();

        if let ContentBlock::TextContent(text) = &result.content[0] {
            assert!(text.text.contains("Wildcard"));
        } else {
            panic!("Expected TextContent");
        }
    }
}
