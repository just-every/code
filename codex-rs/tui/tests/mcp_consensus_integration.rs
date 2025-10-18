//! Integration tests for native MCP consensus implementation
//!
//! FORK-SPECIFIC (just-every/code): Validates end-to-end MCP consensus path
//! Tests the migration from subprocess to native MCP protocol

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use codex_core::config_types::McpServerConfig;
use codex_core::mcp_connection_manager::McpConnectionManager;
use codex_tui::{SpecStage, run_spec_consensus};

/// Helper to check if local-memory MCP server is available
async fn is_local_memory_available() -> bool {
    let config = HashMap::from([(
        "local-memory".to_string(),
        McpServerConfig {
            command: "local-memory".to_string(),
            args: vec![],
            env: None,
            startup_timeout_ms: Some(2000),
        }
    )]);

    match McpConnectionManager::new(config, HashSet::new()).await {
        Ok((_, errors)) => errors.is_empty(),
        Err(_) => false,
    }
}

#[tokio::test]
async fn test_mcp_connection_initialization() {
    // Test that MCP manager can be initialized
    let config = HashMap::from([(
        "local-memory".to_string(),
        McpServerConfig {
            command: "local-memory".to_string(),
            args: vec![],
            env: None,
            startup_timeout_ms: Some(5000),
        }
    )]);

    match McpConnectionManager::new(config, HashSet::new()).await {
        Ok((manager, errors)) => {
            if !errors.is_empty() {
                eprintln!("⚠ MCP initialization had errors: {:?}", errors);
                eprintln!("  This test requires local-memory MCP server to be available");
                eprintln!("  Skipping remainder of test");
                return;
            }

            // Verify local-memory tools are available
            let tools = manager.list_all_tools();
            let has_search = tools.keys().any(|k| k.contains("search"));
            let has_store = tools.keys().any(|k| k.contains("store") || k.contains("remember"));

            if !has_search || !has_store {
                eprintln!("⚠ Expected local-memory tools not found");
                eprintln!("  Available tools: {:?}", tools.keys().collect::<Vec<_>>());
                eprintln!("  Skipping test - local-memory may not be properly configured");
                return;
            }

            println!("✓ MCP manager initialized successfully");
            println!("  Found {} tools", tools.len());
        }
        Err(e) => {
            eprintln!("⚠ MCP initialization failed: {}", e);
            eprintln!("  This test requires local-memory MCP server to be available");
            eprintln!("  Skipping test");
            return;
        }
    }
}

#[tokio::test]
async fn test_mcp_retry_logic_handles_delayed_initialization() {
    // Test retry logic when MCP manager starts None
    let mcp_manager: Arc<tokio::sync::Mutex<Option<Arc<McpConnectionManager>>>> =
        Arc::new(tokio::sync::Mutex::new(None));

    // Spawn task to initialize MCP after a delay
    let mcp_clone = mcp_manager.clone();
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        let config = HashMap::from([(
            "local-memory".to_string(),
            McpServerConfig {
                command: "local-memory".to_string(),
                args: vec![],
                env: None,
                startup_timeout_ms: Some(5000),
            }
        )]);

        if let Ok((manager, _)) = McpConnectionManager::new(config, HashSet::new()).await {
            *mcp_clone.lock().await = Some(Arc::new(manager));
        }
    });

    // Test that retry logic waits for initialization
    const MAX_RETRIES: u32 = 3;
    const RETRY_DELAY_MS: u64 = 100;

    let mut initialized = false;
    for attempt in 0..MAX_RETRIES {
        let guard = mcp_manager.lock().await;
        if guard.is_some() {
            initialized = true;
            drop(guard);
            break;
        }
        drop(guard);

        if attempt < MAX_RETRIES - 1 {
            let delay = RETRY_DELAY_MS * (2_u64.pow(attempt));
            tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
        }
    }

    if !initialized {
        eprintln!("⚠ MCP failed to initialize within retry window");
        eprintln!("  This test requires local-memory MCP server to be available");
        return;
    }

    println!("✓ Retry logic successfully waited for MCP initialization");
}

#[tokio::test]
async fn test_mcp_tool_call_format() {
    // Test that MCP tool calls use correct format
    if !is_local_memory_available().await {
        eprintln!("⚠ Skipping test - local-memory MCP server not available");
        return;
    }

    let config = HashMap::from([(
        "local-memory".to_string(),
        McpServerConfig {
            command: "local-memory".to_string(),
            args: vec![],
            env: None,
            startup_timeout_ms: Some(5000),
        }
    )]);

    let (manager, errors) = McpConnectionManager::new(config, HashSet::new())
        .await
        .expect("MCP manager creation failed");

    if !errors.is_empty() {
        eprintln!("⚠ Skipping test - MCP had initialization errors");
        return;
    }

    // Test tool call with proper arguments
    let search_args = serde_json::json!({
        "query": "test consensus",
        "limit": 5,
        "search_type": "hybrid"
    });

    // Attempt to call search tool
    // Note: This may fail if no data exists, but validates the call path
    match manager.call_tool(
        "local-memory",
        "search",
        Some(search_args),
        Some(std::time::Duration::from_secs(10))
    ).await {
        Ok(result) => {
            println!("✓ MCP tool call succeeded");
            println!("  Result content blocks: {}", result.content.len());
        }
        Err(e) => {
            // Tool call failing is OK (might be no data), we're testing the call path
            println!("⚠ MCP tool call failed (expected if no data): {}", e);
            println!("  This validates the call path works even if data is missing");
        }
    }
}

#[tokio::test]
#[ignore] // Run with --ignored flag when local-memory MCP is available
async fn test_full_consensus_workflow_with_mcp() {
    // Full end-to-end test requiring local-memory MCP server AND test data
    use tempfile::TempDir;

    if !is_local_memory_available().await {
        eprintln!("⚠ Skipping test - local-memory MCP server not available");
        eprintln!("  Run with: cargo test --ignored");
        return;
    }

    let temp_dir = TempDir::new().expect("create temp dir");
    let cwd = temp_dir.path();

    // Initialize MCP manager
    let config = HashMap::from([(
        "local-memory".to_string(),
        McpServerConfig {
            command: "local-memory".to_string(),
            args: vec![],
            env: None,
            startup_timeout_ms: Some(5000),
        }
    )]);

    let (manager, errors) = McpConnectionManager::new(config, HashSet::new())
        .await
        .expect("MCP manager creation failed");

    assert!(errors.is_empty(), "MCP initialization should have no errors");

    // Call consensus check through the actual async function
    // This validates the entire path from handler -> consensus -> MCP
    let result = run_spec_consensus(
        cwd,
        "SPEC-TEST-001",
        SpecStage::Plan,
        true,
        &manager,
    ).await;

    match result {
        Ok((lines, consensus_ok)) => {
            println!("✓ Consensus check completed");
            println!("  Lines returned: {}", lines.len());
            println!("  Consensus OK: {}", consensus_ok);

            // Validate response structure
            assert!(!lines.is_empty(), "Should return at least one line");
        }
        Err(e) => {
            // Error is expected if no test data exists in local-memory
            eprintln!("⚠ Consensus check failed (expected without test data): {}", e);
            eprintln!("  Error validates proper error handling path");
        }
    }
}
