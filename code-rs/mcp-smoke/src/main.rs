use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use assert_cmd::cargo::cargo_bin;
use code_core::config_types::{McpServerConfig, McpServerTransportConfig};
use code_core::mcp_connection_manager::McpConnectionManager;

fn server_bin_path() -> PathBuf {
    cargo_bin("code-mcp-test-server")
}

#[tokio::main]
async fn main() -> Result<()> {
    let server = server_bin_path();
    if !server.exists() {
        eprintln!("Build the test server first: cargo build -p code-mcp-test-server");
        std::process::exit(2);
    }

    // Fast server
    let fast = McpServerConfig {
        transport: McpServerTransportConfig::Stdio {
            command: server.to_string_lossy().to_string(),
            args: vec![],
            env: None,
        },
        startup_timeout_sec: Some(Duration::from_millis(500)),
        tool_timeout_sec: None,
    };
    // Slow-one: 2s but we allow 3s
    let slow_ok = McpServerConfig {
        transport: McpServerTransportConfig::Stdio {
            command: "bash".to_string(),
            args: vec![
                "-lc".to_string(),
                format!("SLOW_INIT_MS=500 SLOW_LIST_MS=2000 {}", server.display()),
            ],
            env: None,
        },
        startup_timeout_sec: Some(Duration::from_millis(3000)),
        tool_timeout_sec: None,
    };
    // Slow-two: 3s but we allow 1s (should fail)
    let slow_fail = McpServerConfig {
        transport: McpServerTransportConfig::Stdio {
            command: "bash".to_string(),
            args: vec![
                "-lc".to_string(),
                format!("SLOW_INIT_MS=500 SLOW_LIST_MS=3000 {}", server.display()),
            ],
            env: None,
        },
        startup_timeout_sec: Some(Duration::from_millis(1000)),
        tool_timeout_sec: None,
    };

    let mut servers = HashMap::new();
    servers.insert("fast".to_string(), fast);
    servers.insert("slow_ok".to_string(), slow_ok);
    servers.insert("slow_fail".to_string(), slow_fail);

    let (mgr, errs) =
        McpConnectionManager::new(servers, std::collections::HashSet::new()).await?;
    println!("Errors: {}", errs.len());
    for (name, e) in &errs {
        println!("  {}: {}", name, e);
    }
    let tools = mgr.list_all_tools();
    println!("Tools ({}):", tools.len());
    for k in tools.keys() {
        println!("  {}", k);
    }
    Ok(())
}
